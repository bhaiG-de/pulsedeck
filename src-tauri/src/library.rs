use crate::suggest::{PendingFeatures, TrackFeatures, FEATURES_VERSION};
use audio_analysis::{
    AutoCues, BeatModelStage, KeyCandidate, SectionStage, TempoCandidate, WaveformBands,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRecord {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration_seconds: f64,
    pub bpm: f64,
    pub key: Option<String>,
    pub sample_rate: u32,
    pub waveform: Vec<f32>,
    pub added_at: String,
    pub hot_cues: Vec<Option<f64>>,
    pub memory_cues: Vec<Option<f64>>,
    pub cue_seconds: f64,
    pub camelot: Option<String>,
    pub key_confidence: Option<f32>,
    pub beatgrid_offset_seconds: f64,
    #[serde(flatten, default)]
    pub analysis: AnalysisMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisMetadata {
    pub analysis_version: u32,
    pub bpm_confidence: f32,
    pub bpm_candidates: Vec<TempoCandidate>,
    pub bpm_ambiguous: bool,
    pub bpm_source: Option<String>,
    pub key_candidates: Vec<KeyCandidate>,
    pub waveform_bands: Option<WaveformBands>,
    pub beatgrid_confidence: Option<f32>,
    pub beatgrid_source: Option<String>,
    pub beat_model: Option<BeatModelStage>,
    pub sections: Option<SectionStage>,
    pub auto_cues: Option<AutoCues>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_ids: Vec<i64>,
}

pub struct Library {
    db: Connection,
    /// Suggestion features by source, loaded on first use and kept in step
    /// with every write below.
    features: RefCell<Option<HashMap<String, TrackFeatures>>>,
    /// Set once no stored track lacks current features; writes keep it so.
    features_complete: Cell<bool>,
}

/// Whether a cached Spotify analysis is complete enough to display and reuse.
pub fn usable_spotify_analysis(uri: &str, a: &deck_engine::TrackAnalysis) -> bool {
    a.path == uri
        && (a.analysis_version == audio_analysis::ANALYSIS_VERSION
            || (audio_analysis::ANALYSIS_VERSION == 3 && a.analysis_version == 2))
        && !a.recording_allowed
        && a.waveform.len() == 4096
        && a.waveform_bands
            .as_ref()
            .is_some_and(|b| b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096)
}

const FEATURE_COLUMNS: &str =
    "source,track_id,title,artist,duration_seconds,bpm,bpm_ambiguous,camelot,key_confidence,energy";

fn feature_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackFeatures> {
    Ok(TrackFeatures {
        source: row.get(0)?,
        track_id: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        duration_seconds: row.get(4)?,
        bpm: row.get(5)?,
        bpm_ambiguous: row.get(6)?,
        camelot: row.get(7)?,
        key_confidence: row.get(8)?,
        energy: row.get(9)?,
    })
}

impl Library {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        Self::from_connection(db)
    }

    fn from_connection(db: Connection) -> Result<Self, String> {
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS tracks(
            id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
            artist TEXT NOT NULL DEFAULT '', duration_seconds REAL NOT NULL, bpm REAL NOT NULL,
            key_name TEXT, sample_rate INTEGER NOT NULL, waveform TEXT NOT NULL,
            added_at TEXT NOT NULL, hot_cues TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null,null]',
            beatgrid_offset_seconds REAL NOT NULL DEFAULT 0,
            memory_cues TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null,null,null,null]',
            cue_seconds REAL NOT NULL DEFAULT 0,camelot TEXT,key_confidence REAL);
          CREATE TABLE IF NOT EXISTS spotify_analysis(uri TEXT PRIMARY KEY, version INTEGER NOT NULL, metadata TEXT NOT NULL, updated_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS playlists(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE);
          CREATE TABLE IF NOT EXISTS playlist_tracks(playlist_id INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
            track_id INTEGER REFERENCES tracks(id) ON DELETE CASCADE, position INTEGER NOT NULL,
            PRIMARY KEY(playlist_id,track_id));")
            .map_err(|e| e.to_string())?;
        let columns: std::collections::HashSet<String> = db
            .prepare("PRAGMA table_info(tracks)")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        for (name, declaration) in [
            (
                "memory_cues",
                "TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null,null,null,null]'",
            ),
            ("cue_seconds", "REAL NOT NULL DEFAULT 0"),
            ("camelot", "TEXT"),
            ("key_confidence", "REAL"),
            ("analysis_json", "TEXT NOT NULL DEFAULT '{}'"),
        ] {
            if !columns.contains(name) {
                db.execute(
                    &format!("ALTER TABLE tracks ADD COLUMN {name} {declaration}"),
                    [],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        // Compact per-track data for next-track suggestions, so they never
        // read the large analysis records (see suggest.rs).
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS suggest_features(source TEXT PRIMARY KEY,
               track_id INTEGER REFERENCES tracks(id) ON DELETE CASCADE, version INTEGER NOT NULL,
               title TEXT NOT NULL, artist TEXT NOT NULL, duration_seconds REAL NOT NULL, bpm REAL,
               bpm_ambiguous INTEGER NOT NULL, camelot TEXT, key_confidence REAL, energy REAL);
             CREATE INDEX IF NOT EXISTS suggest_features_track ON suggest_features(track_id);",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            features: RefCell::new(None),
            features_complete: Cell::new(false),
        })
    }

    pub fn spotify_analysis(
        &self,
        uri: &str,
    ) -> Result<Option<deck_engine::TrackAnalysis>, String> {
        let json: Option<String> = self
            .db
            .query_row(
                "SELECT metadata FROM spotify_analysis WHERE uri=?1 AND (version=?2 OR (?2=3 AND version=2))",
                params![uri, audio_analysis::ANALYSIS_VERSION],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(json
            .and_then(|json| serde_json::from_str::<deck_engine::TrackAnalysis>(&json).ok())
            .filter(|a| usable_spotify_analysis(uri, a)))
    }
    pub fn cache_spotify_analysis(
        &self,
        analysis: &deck_engine::TrackAnalysis,
    ) -> Result<(), String> {
        if !analysis.path.starts_with("spotify:track:") || analysis.recording_allowed {
            return Err("Only Spotify analysis metadata can enter this cache".into());
        }
        let json = serde_json::to_string(analysis).map_err(|e| e.to_string())?;
        self.db.execute("INSERT INTO spotify_analysis(uri,version,metadata,updated_at) VALUES(?1,?2,?3,unixepoch()) ON CONFLICT(uri) DO UPDATE SET version=excluded.version,metadata=excluded.metadata,updated_at=excluded.updated_at", params![analysis.path, analysis.analysis_version, json]).map_err(|e| e.to_string())?;
        self.store_features(&[if usable_spotify_analysis(&analysis.path, analysis) {
            TrackFeatures::from_analysis(analysis)
        } else {
            TrackFeatures::unusable(analysis.path.clone())
        }])
    }

    /// Store suggestion features, replacing older ones for the same source.
    /// Local rows whose track was deleted meanwhile are skipped.
    pub fn store_features(&self, features: &[TrackFeatures]) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut stored = Vec::with_capacity(features.len());
        {
            let mut insert = tx
                .prepare_cached(&format!(
                    "INSERT INTO suggest_features(version,{FEATURE_COLUMNS})
                     SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11
                     WHERE ?3 IS NULL OR EXISTS(SELECT 1 FROM tracks WHERE id=?3)
                     ON CONFLICT(source) DO UPDATE SET version=excluded.version,track_id=excluded.track_id,
                       title=excluded.title,artist=excluded.artist,duration_seconds=excluded.duration_seconds,
                       bpm=excluded.bpm,bpm_ambiguous=excluded.bpm_ambiguous,camelot=excluded.camelot,
                       key_confidence=excluded.key_confidence,energy=excluded.energy"
                ))
                .map_err(|e| e.to_string())?;
            for f in features {
                let changed = insert
                    .execute(params![
                        FEATURES_VERSION,
                        f.source,
                        f.track_id,
                        f.title,
                        f.artist,
                        f.duration_seconds,
                        f.bpm,
                        f.bpm_ambiguous,
                        f.camelot,
                        f.key_confidence,
                        f.energy
                    ])
                    .map_err(|e| e.to_string())?;
                if changed > 0 {
                    stored.push(f);
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        if let Some(cache) = self.features.borrow_mut().as_mut() {
            for f in stored {
                cache.insert(f.source.clone(), f.clone());
            }
        }
        Ok(())
    }

    /// Run `score` over the current suggestion features of every source,
    /// loading them from the database on first use.
    pub fn with_features<R>(
        &self,
        score: impl FnOnce(&HashMap<String, TrackFeatures>) -> R,
    ) -> Result<R, String> {
        if self.features.borrow().is_none() {
            let mut stmt = self
                .db
                .prepare(&format!(
                    "SELECT {FEATURE_COLUMNS} FROM suggest_features WHERE version=?1"
                ))
                .map_err(|e| e.to_string())?;
            let loaded = stmt
                .query_map([FEATURES_VERSION], feature_row)
                .map_err(|e| e.to_string())?
                .map(|row| row.map(|f| (f.source.clone(), f)))
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(|e| e.to_string())?;
            *self.features.borrow_mut() = Some(loaded);
        }
        let cache = self.features.borrow();
        Ok(score(
            cache.as_ref().ok_or("Suggestion features unavailable")?,
        ))
    }

    /// How many stored tracks still lack current suggestion features. Free
    /// once none do, because every write keeps the features current.
    pub fn features_pending(&self) -> Result<usize, String> {
        if self.features_complete.get() {
            return Ok(0);
        }
        let count: i64 = self
            .db
            .query_row(
                "SELECT (SELECT count(*) FROM tracks t WHERE NOT EXISTS(SELECT 1 FROM suggest_features f WHERE f.source=t.path AND f.version=?1))
                      + (SELECT count(*) FROM spotify_analysis s WHERE NOT EXISTS(SELECT 1 FROM suggest_features f WHERE f.source=s.uri AND f.version=?1))",
                [FEATURES_VERSION],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        self.features_complete.set(count == 0);
        Ok(count as usize)
    }

    /// Up to `limit` stored records that lack current suggestion features,
    /// raw, so they can be parsed without holding the library.
    pub fn pending_features(&self, limit: usize) -> Result<Vec<PendingFeatures>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT t.id,t.path,t.title,t.artist,t.duration_seconds,t.bpm,t.camelot,t.key_confidence,t.analysis_json
                 FROM tracks t WHERE NOT EXISTS(SELECT 1 FROM suggest_features f WHERE f.source=t.path AND f.version=?1) LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let mut pending = stmt
            .query_map(params![FEATURES_VERSION, limit as i64], |r| {
                let mut features = TrackFeatures::unusable(r.get(1)?);
                features.track_id = Some(r.get(0)?);
                features.title = r.get(2)?;
                features.artist = r.get(3)?;
                features.duration_seconds = r.get(4)?;
                features.bpm = crate::suggest::usable_bpm(r.get(5)?);
                features.camelot = r.get(6)?;
                features.key_confidence = r.get(7)?;
                Ok(PendingFeatures::Local {
                    features,
                    analysis_json: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if pending.len() < limit {
            let mut stmt = self
                .db
                .prepare(
                    "SELECT s.uri,s.metadata FROM spotify_analysis s
                     WHERE NOT EXISTS(SELECT 1 FROM suggest_features f WHERE f.source=s.uri AND f.version=?1) LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let spotify = stmt
                .query_map(
                    params![FEATURES_VERSION, (limit - pending.len()) as i64],
                    |r| {
                        Ok(PendingFeatures::Spotify {
                            uri: r.get(0)?,
                            metadata: r.get(1)?,
                        })
                    },
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            pending.extend(spotify);
        }
        Ok(pending)
    }

    pub fn tracks(&self) -> Result<Vec<TrackRecord>, String> {
        let mut stmt = self.db.prepare("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,analysis_json FROM tracks ORDER BY added_at DESC,id DESC").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], Self::row).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRecord> {
        Ok(TrackRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            duration_seconds: row.get(4)?,
            bpm: row.get(5)?,
            key: row.get(6)?,
            sample_rate: row.get(7)?,
            waveform: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
            added_at: row.get(9)?,
            hot_cues: serde_json::from_str(&row.get::<_, String>(10)?)
                .unwrap_or_else(|_| vec![None; 8]),
            beatgrid_offset_seconds: row.get(11)?,
            memory_cues: serde_json::from_str(&row.get::<_, String>(12)?)
                .unwrap_or_else(|_| vec![None; 10]),
            cue_seconds: row.get(13)?,
            camelot: row.get(14)?,
            key_confidence: row.get(15)?,
            analysis: serde_json::from_str(&row.get::<_, String>(16)?).unwrap_or_default(),
        })
    }

    /// Local tracks whose stored analysis lacks the current beat-model stage.
    pub fn paths_needing_beat_model(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT path FROM tracks WHERE coalesce(json_extract(analysis_json,'$.beat_model.version'),0) < ?1 ORDER BY added_at DESC,id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([audio_analysis::BEAT_MODEL_VERSION], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn track_by_path(&self, path: &str) -> Result<Option<TrackRecord>, String> {
        self.db.query_row("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,analysis_json FROM tracks WHERE path=?1",[path],Self::row).optional().map_err(|e|e.to_string())
    }

    pub fn upsert(&self, track: &TrackRecord) -> Result<TrackRecord, String> {
        let waveform = serde_json::to_string(&track.waveform).map_err(|e| e.to_string())?;
        let mut metadata = track.analysis.clone();
        let mut bpm = track.bpm;
        let mut grid_offset = track.beatgrid_offset_seconds;
        if let Some(previous) = self.track_by_path(&track.path)? {
            if previous.analysis.beatgrid_source.as_deref() == Some("manual")
                || (previous.analysis.beatgrid_source.is_none()
                    && previous.beatgrid_offset_seconds != 0.0)
            {
                grid_offset = previous.beatgrid_offset_seconds;
                metadata.beatgrid_source = Some("manual".into());
                metadata.beatgrid_confidence = None;
            }

            if previous
                .analysis
                .bpm_source
                .as_deref()
                .is_some_and(|s| s == "manual" || s == "import")
            {
                bpm = previous.bpm;
                metadata.bpm_source = previous.analysis.bpm_source;
                metadata.bpm_confidence = previous.analysis.bpm_confidence;
            }
        }
        let analysis_json = serde_json::to_string(&metadata).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
            .to_string();
        self.db.execute("INSERT INTO tracks(path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,camelot,key_confidence,analysis_json,beatgrid_offset_seconds)
          VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
          ON CONFLICT(path) DO UPDATE SET title=excluded.title,artist=excluded.artist,duration_seconds=excluded.duration_seconds,
          bpm=excluded.bpm,key_name=excluded.key_name,sample_rate=excluded.sample_rate,waveform=excluded.waveform,camelot=excluded.camelot,key_confidence=excluded.key_confidence,analysis_json=excluded.analysis_json,beatgrid_offset_seconds=excluded.beatgrid_offset_seconds",
          params![track.path,track.title,track.artist,track.duration_seconds,bpm,track.key,track.sample_rate,waveform,now,track.camelot,track.key_confidence,analysis_json,grid_offset]).map_err(|e|e.to_string())?;
        let record = self
            .track_by_path(&track.path)?
            .ok_or_else(|| "Imported track was not found".to_string())?;
        self.store_features(&[TrackFeatures::from_record(&record)])?;
        Ok(record)
    }

    pub fn delete_track(&self, id: i64) -> Result<(), String> {
        // Its suggestion features go with it (ON DELETE CASCADE).
        self.db
            .execute("DELETE FROM tracks WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        if let Some(cache) = self.features.borrow_mut().as_mut() {
            cache.retain(|_, f| f.track_id != Some(id));
        }
        Ok(()) // Original music files are never removed.
    }

    pub fn metadata(
        &self,
        path: &str,
        bpm: Option<f64>,
        cues: Option<Vec<Option<f64>>>,
        offset: Option<f64>,
    ) -> Result<(), String> {
        if let Some(bpm) = bpm {
            let mut analysis = self
                .track_by_path(path)?
                .map(|t| t.analysis)
                .unwrap_or_default();
            analysis.bpm_source = Some("manual".into());
            analysis.bpm_confidence = 1.;
            self.db
                .execute(
                    "UPDATE tracks SET bpm=?1,analysis_json=?3 WHERE path=?2",
                    params![
                        bpm,
                        path,
                        serde_json::to_string(&analysis).map_err(|e| e.to_string())?
                    ],
                )
                .map_err(|e| e.to_string())?;
            // A manual BPM is the DJ's reading, so it is no longer ambiguous.
            let bpm = crate::suggest::usable_bpm(Some(bpm));
            self.db
                .execute(
                    "UPDATE suggest_features SET bpm=?1,bpm_ambiguous=0 WHERE source=?2",
                    params![bpm, path],
                )
                .map_err(|e| e.to_string())?;
            if let Some(f) = self
                .features
                .borrow_mut()
                .as_mut()
                .and_then(|c| c.get_mut(path))
            {
                f.bpm = bpm;
                f.bpm_ambiguous = false;
            }
        }
        if let Some(cues) = cues {
            if cues.len() != 8 || cues.iter().flatten().any(|x| !x.is_finite() || *x < 0.) {
                return Err("Invalid hot cues".into());
            }
            self.db
                .execute(
                    "UPDATE tracks SET hot_cues=?1 WHERE path=?2",
                    params![
                        serde_json::to_string(&cues).map_err(|e| e.to_string())?,
                        path
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(offset) = offset {
            let mut analysis = self
                .track_by_path(path)?
                .map(|t| t.analysis)
                .unwrap_or_default();
            analysis.beatgrid_source = Some("manual".into());
            analysis.beatgrid_confidence = None;
            self.db
                .execute(
                    "UPDATE tracks SET analysis_json=?1 WHERE path=?2",
                    params![
                        serde_json::to_string(&analysis).map_err(|e| e.to_string())?,
                        path
                    ],
                )
                .map_err(|e| e.to_string())?;
            self.db
                .execute(
                    "UPDATE tracks SET beatgrid_offset_seconds=?1 WHERE path=?2",
                    params![offset, path],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>, String> {
        let mut stmt = self
            .db
            .prepare("SELECT id,name FROM playlists ORDER BY name")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name) = row.map_err(|e| e.to_string())?;
            let mut tracks = self
                .db
                .prepare(
                    "SELECT track_id FROM playlist_tracks WHERE playlist_id=?1 ORDER BY position",
                )
                .map_err(|e| e.to_string())?;
            let track_ids = tracks
                .query_map([id], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<i64>, _>>()
                .map_err(|e| e.to_string())?;
            out.push(Playlist {
                id,
                name,
                track_ids,
            });
        }
        Ok(out)
    }

    pub fn memory(&self, path: &str, cues: &[Option<f64>], cue_seconds: f64) -> Result<(), String> {
        if cues.len() != 10
            || cues.iter().flatten().any(|x| !x.is_finite() || *x < 0.)
            || !cue_seconds.is_finite()
            || cue_seconds < 0.
        {
            return Err("Invalid memory cue data".into());
        }
        self.db
            .execute(
                "UPDATE tracks SET memory_cues=?1,cue_seconds=?2 WHERE path=?3",
                params![
                    serde_json::to_string(cues).map_err(|e| e.to_string())?,
                    cue_seconds,
                    path
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn tags(
        &self,
        path: &str,
        title: &str,
        artist: &str,
        key: Option<&str>,
    ) -> Result<(), String> {
        self.db.execute("UPDATE tracks SET title=CASE WHEN ?1='' THEN title ELSE ?1 END,artist=CASE WHEN ?2='' THEN artist ELSE ?2 END,key_name=COALESCE(?3,key_name) WHERE path=?4",params![title,artist,key,path]).map_err(|e|e.to_string())?;
        self.db.execute("UPDATE suggest_features SET title=CASE WHEN ?1='' THEN title ELSE ?1 END,artist=CASE WHEN ?2='' THEN artist ELSE ?2 END WHERE source=?3",params![title,artist,path]).map_err(|e|e.to_string())?;
        if let Some(f) = self
            .features
            .borrow_mut()
            .as_mut()
            .and_then(|c| c.get_mut(path))
        {
            if !title.is_empty() {
                f.title = title.into();
            }
            if !artist.is_empty() {
                f.artist = artist.into();
            }
        }
        Ok(())
    }

    pub fn create_playlist(&self, name: &str) -> Result<Playlist, String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err("Choose a playlist name between 1 and 200 characters".into());
        }
        self.db
            .execute("INSERT OR IGNORE INTO playlists(name) VALUES(?1)", [name])
            .map_err(|e| e.to_string())?;
        self.playlists()?
            .into_iter()
            .find(|p| p.name == name)
            .ok_or_else(|| "Playlist not found".into())
    }

    pub fn add_to_playlist(&self, playlist_id: i64, track_id: i64) -> Result<(), String> {
        self.db.execute("INSERT OR IGNORE INTO playlist_tracks(playlist_id,track_id,position)
          VALUES(?1,?2,COALESCE((SELECT MAX(position)+1 FROM playlist_tracks WHERE playlist_id=?1),0))",params![playlist_id,track_id]).map_err(|e|e.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "DJ_PLAYLISTS")]
pub struct RekordboxXml {
    #[serde(rename = "COLLECTION", default)]
    pub collection: XmlCollection,
    #[serde(rename = "PLAYLISTS", default)]
    pub playlists: XmlPlaylists,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlCollection {
    #[serde(rename = "TRACK", default)]
    pub tracks: Vec<XmlTrack>,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlTrack {
    #[serde(rename = "@TrackID", default)]
    pub id: String,
    #[serde(rename = "@Location", default)]
    pub location: String,
    #[serde(rename = "@Name", default)]
    pub name: String,
    #[serde(rename = "@Artist", default)]
    pub artist: String,
    #[serde(rename = "@AverageBpm", default)]
    pub bpm: Option<f64>,
    #[serde(rename = "@Tonality", default)]
    pub key: Option<String>,
    #[serde(rename = "POSITION_MARK", default)]
    pub marks: Vec<XmlMark>,
    #[serde(rename = "TEMPO", default)]
    pub tempos: Vec<XmlTempo>,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlMark {
    #[serde(rename = "@Num", default)]
    pub number: i32,
    #[serde(rename = "@Start", default)]
    pub start: f64,
    #[serde(rename = "@Type", default)]
    pub kind: i32,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlTempo {
    #[serde(rename = "@Inizio", default)]
    pub start: f64,
    #[serde(rename = "@Bpm", default)]
    pub bpm: f64,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlPlaylists {
    #[serde(rename = "NODE", default)]
    pub nodes: Vec<XmlNode>,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlNode {
    #[serde(rename = "@Name", default)]
    pub name: String,
    #[serde(rename = "NODE", default)]
    pub children: Vec<XmlNode>,
    #[serde(rename = "TRACK", default)]
    pub tracks: Vec<XmlPlaylistTrack>,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlPlaylistTrack {
    #[serde(rename = "@Key", default)]
    pub key: String,
}

pub fn parse_rekordbox_xml(path: &Path) -> Result<RekordboxXml, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err("XML exceeds the 32 MB import limit".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    quick_xml::de::from_str(&text).map_err(|e| format!("Invalid rekordbox XML: {e}"))
}

pub fn xml_path(location: &str) -> Result<PathBuf, String> {
    let url = url::Url::parse(location).map_err(|e| e.to_string())?;
    if url.scheme() != "file" {
        return Err("Only local file URLs can be imported".into());
    }
    if let Some(host) = url.host_str() {
        if !host.is_empty() && host != "localhost" {
            return Err("Network paths are not supported".into());
        }
    }
    // rekordbox commonly exports file://localhost/... .
    let mut url = url;
    let _ = url.set_host(None);
    url.to_file_path()
        .map_err(|_| "Invalid local track URL".into())
}

pub fn export_xml(tracks: &[TrackRecord], playlists: &[Playlist]) -> Result<String, String> {
    fn escape(value: &str) -> String {
        quick_xml::escape::escape(value).into_owned()
    }
    let mut xml=String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\"><PRODUCT Name=\"PulseDeck\" Version=\"0.1.0\" Company=\"PulseDeck\"/><COLLECTION Entries=\"");
    xml.push_str(&tracks.len().to_string());
    xml.push_str("\">\n");
    for t in tracks {
        let location = url::Url::from_file_path(&t.path)
            .map_err(|_| "Track path must be absolute".to_string())?;
        xml.push_str(&format!("<TRACK TrackID=\"{}\" Name=\"{}\" Artist=\"{}\" TotalTime=\"{}\" AverageBpm=\"{}\" Tonality=\"{}\" Location=\"{}\">",t.id,escape(&t.title),escape(&t.artist),t.duration_seconds.round() as u64,t.bpm,escape(t.key.as_deref().unwrap_or("")),escape(location.as_str())));
        xml.push_str(&format!(
            "<TEMPO Inizio=\"{}\" Bpm=\"{}\" Metro=\"4/4\" Battito=\"1\"/>",
            t.beatgrid_offset_seconds, t.bpm
        ));
        for (index, cue) in t.hot_cues.iter().enumerate() {
            if let Some(seconds) = cue {
                xml.push_str(&format!(
                    "<POSITION_MARK Name=\"Hot Cue {}\" Type=\"0\" Start=\"{}\" Num=\"{}\"/>",
                    index + 1,
                    seconds,
                    index
                ));
            }
        }
        for cue in t.memory_cues.iter().flatten() {
            xml.push_str(&format!(
                "<POSITION_MARK Name=\"Memory Cue\" Type=\"0\" Start=\"{}\" Num=\"-1\"/>",
                cue
            ));
        }
        xml.push_str("</TRACK>\n");
    }
    xml.push_str(&format!(
        "</COLLECTION><PLAYLISTS><NODE Type=\"0\" Name=\"ROOT\" Count=\"{}\">",
        playlists.len()
    ));
    for p in playlists {
        xml.push_str(&format!(
            "<NODE Type=\"1\" Name=\"{}\" KeyType=\"0\" Entries=\"{}\">",
            escape(&p.name),
            p.track_ids.len()
        ));
        for id in &p.track_ids {
            xml.push_str(&format!("<TRACK Key=\"{id}\"/>"));
        }
        xml.push_str("</NODE>");
    }
    xml.push_str("</NODE></PLAYLISTS></DJ_PLAYLISTS>\n");
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> TrackRecord {
        TrackRecord {
            id: 1,
            path: "/tmp/Music & beats/song.wav".into(),
            title: "A & B <mix>".into(),
            artist: "DJ".into(),
            duration_seconds: 90.,
            bpm: 120.,
            key: Some("Am".into()),
            sample_rate: 48000,
            waveform: vec![0., 0.5],
            added_at: String::new(),
            hot_cues: vec![Some(1.5), None, None, None, None, None, None, None],
            memory_cues: vec![None; 10],
            cue_seconds: 0.,
            camelot: Some("8A".into()),
            key_confidence: Some(0.5),
            beatgrid_offset_seconds: 0.2,
            analysis: AnalysisMetadata::default(),
        }
    }
    #[test]
    fn beat_model_stage_persists_and_only_missing_stages_are_queued() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut done = track();
        done.analysis.beat_model = Some(BeatModelStage {
            version: audio_analysis::BEAT_MODEL_VERSION,
            beats_seconds: vec![0.25, 0.75],
            downbeats_seconds: vec![0.25],
            grid: None,
        });
        let mut stale = track();
        stale.path = "/tmp/old.wav".into();
        stale.analysis.beat_model = Some(BeatModelStage {
            version: 0,
            ..done.analysis.beat_model.clone().unwrap()
        });
        let mut missing = track();
        missing.path = "/tmp/missing.wav".into();
        let saved = lib.upsert(&done).unwrap();
        lib.upsert(&stale).unwrap();
        lib.upsert(&missing).unwrap();
        assert_eq!(saved.analysis.beat_model, done.analysis.beat_model);
        let mut queued = lib.paths_needing_beat_model().unwrap();
        queued.sort();
        assert_eq!(queued, ["/tmp/missing.wav", "/tmp/old.wav"]);
    }
    #[test]
    fn persistence_and_playlist_cascade() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        let p = lib.create_playlist("Set").unwrap();
        lib.add_to_playlist(p.id, saved.id).unwrap();
        lib.add_to_playlist(p.id, saved.id).unwrap();
        assert_eq!(lib.playlists().unwrap()[0].track_ids, vec![saved.id]);
        let cues = vec![Some(2.), None, None, None, None, None, None, None];
        lib.metadata(&saved.path, Some(125.), Some(cues.clone()), Some(0.1))
            .unwrap();
        lib.upsert(&track()).unwrap();
        assert_eq!(lib.tracks().unwrap()[0].hot_cues, cues);
        assert_eq!(lib.tracks().unwrap()[0].bpm, 125.);
        lib.delete_track(saved.id).unwrap();
        assert!(lib.playlists().unwrap()[0].track_ids.is_empty());
    }
    #[test]
    fn analysis_metadata_and_bands_survive_database_roundtrip() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut record = track();
        record.analysis.analysis_version = 2;
        record.analysis.bpm_confidence = 0.8;
        record.analysis.bpm_source = Some("analysis".into());
        record.analysis.bpm_ambiguous = true;
        record.analysis.waveform_bands = Some(WaveformBands {
            low: vec![0.1, 0.2],
            mid: vec![0.2, 0.1],
            high: vec![0.03, 0.04],
        });
        let saved = lib.upsert(&record).unwrap();
        assert_eq!(saved.analysis.analysis_version, 2);
        assert!(saved.analysis.bpm_ambiguous);
        assert_eq!(saved.analysis.waveform_bands.unwrap().low, vec![0.1, 0.2]);
        let serialized =
            serde_json::to_value(lib.track_by_path(&record.path).unwrap().unwrap()).unwrap();
        assert_eq!(serialized["analysis_version"], 2);
        assert_eq!(serialized["waveform_bands"]["mid"][0], 0.2_f32);
    }
    #[test]
    fn xml_roundtrip_cues_and_escaped_names() {
        let t = track();
        let xml = export_xml(
            std::slice::from_ref(&t),
            &[Playlist {
                id: 1,
                name: "Set & one".into(),
                track_ids: vec![1],
            }],
        )
        .unwrap();
        let read: RekordboxXml = quick_xml::de::from_str(&xml).unwrap();
        assert_eq!(read.collection.tracks[0].name, t.title);
        assert_eq!(read.collection.tracks[0].marks[0].start, 1.5);
        assert_eq!(
            xml_path(&read.collection.tracks[0].location)
                .unwrap()
                .to_str()
                .unwrap(),
            t.path
        );
        assert_eq!(read.playlists.nodes[0].children[0].tracks[0].key, "1");
        assert!(xml_path("https://example.com/music.mp3").is_err());
        assert!(xml_path("file://server/private/song.mp3").is_err());
    }
    #[test]
    fn spotify_cache_survives_reopen_and_rejects_old_or_incomplete_analysis() {
        let directory = std::env::temp_dir().join(format!(
            "pulsedeck-analysis-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("cache.sqlite");
        let uri = "spotify:track:0000000000000000000000";
        let samples = vec![[0.0; 2]; 4000];
        let mut analysis = deck_engine::analyze_decoded_audio(
            &samples,
            8000,
            &deck_engine::AudioSourceInfo {
                id: uri.into(),
                location: uri.into(),
                title: "Fixture".into(),
                artist: None,
                extension: None,
            },
            false,
        )
        .unwrap();
        analysis.bpm = Some(127.8);
        analysis.key = Some("B minor".into());
        analysis.camelot = Some("10A".into());
        {
            let library = Library::open(&path).unwrap();
            library.cache_spotify_analysis(&analysis).unwrap();
            assert!(library.tracks().unwrap().is_empty());
        }
        {
            let library = Library::open(&path).unwrap();
            let restored = library.spotify_analysis(uri).unwrap().unwrap();
            assert_eq!(restored.bpm, Some(127.8));
            assert_eq!(restored.camelot.as_deref(), Some("10A"));
            assert_eq!(restored.waveform_bands.unwrap().low.len(), 4096);
            analysis.analysis_version = 0;
            library.cache_spotify_analysis(&analysis).unwrap();
            assert!(library.spotify_analysis(uri).unwrap().is_none());
            analysis.analysis_version = audio_analysis::ANALYSIS_VERSION;
            analysis.waveform_bands = None;
            library.cache_spotify_analysis(&analysis).unwrap();
            assert!(library.spotify_analysis(uri).unwrap().is_none());
            analysis.recording_allowed = true;
            assert!(library.cache_spotify_analysis(&analysis).is_err());
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn bands(level: f32) -> WaveformBands {
        WaveformBands {
            low: vec![level; 4096],
            mid: vec![level * 0.5; 4096],
            high: vec![level * 0.2; 4096],
        }
    }

    fn cached_features(lib: &Library) -> HashMap<String, TrackFeatures> {
        lib.with_features(|pool| pool.clone()).unwrap()
    }

    #[test]
    fn suggestion_features_follow_every_library_write() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        // Load the in-memory copy first so each write must update it.
        assert!(cached_features(&lib).is_empty());
        let mut record = track();
        record.analysis.bpm_ambiguous = true;
        record.analysis.waveform_bands = Some(bands(0.4));
        let saved = lib.upsert(&record).unwrap();
        let stored = &cached_features(&lib)[&saved.path];
        assert_eq!(stored.track_id, Some(saved.id));
        assert_eq!(stored.camelot.as_deref(), Some("8A"));
        assert!(stored.bpm_ambiguous && stored.energy.is_some());
        lib.metadata(&saved.path, Some(124.0), None, None).unwrap();
        lib.tags(&saved.path, "Renamed", "", None).unwrap();
        let edited = cached_features(&lib)[&saved.path].clone();
        assert_eq!((edited.bpm, edited.bpm_ambiguous), (Some(124.0), false));
        assert_eq!(
            (edited.title.as_str(), edited.artist.as_str()),
            ("Renamed", "DJ")
        );
        // The database agrees with the in-memory copy.
        *lib.features.borrow_mut() = None;
        assert_eq!(cached_features(&lib)[&saved.path], edited);
        lib.delete_track(saved.id).unwrap();
        assert!(cached_features(&lib).is_empty());
        *lib.features.borrow_mut() = None;
        assert!(cached_features(&lib).is_empty());
        assert_eq!(lib.features_pending().unwrap(), 0);
    }

    #[test]
    fn records_from_before_suggestions_get_features_in_the_background_pass() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        // A local track and Spotify entries written without features, as
        // older versions of the app stored them.
        let mut analysis = track().analysis;
        analysis.bpm_ambiguous = true;
        analysis.waveform_bands = Some(bands(0.3));
        lib.db
            .execute(
                "INSERT INTO tracks(path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,camelot,key_confidence,analysis_json)
                 VALUES('/old.wav','Old','DJ',200,122,'Am',44100,'[]','0','8A',0.7,?1)",
                [serde_json::to_string(&analysis).unwrap()],
            )
            .unwrap();
        let uri = "spotify:track:1111111111111111111111";
        let samples = vec![[0.1; 2]; 8000];
        let mut spotify = deck_engine::analyze_decoded_audio(
            &samples,
            8000,
            &deck_engine::AudioSourceInfo {
                id: uri.into(),
                location: uri.into(),
                title: "Cached".into(),
                artist: Some("Artist".into()),
                extension: None,
            },
            false,
        )
        .unwrap();
        spotify.bpm = Some(124.0);
        spotify.camelot = Some("9A".into());
        lib.db
            .execute(
                "INSERT INTO spotify_analysis(uri,version,metadata,updated_at) VALUES(?1,?2,?3,0)",
                params![
                    uri,
                    spotify.analysis_version,
                    serde_json::to_string(&spotify).unwrap()
                ],
            )
            .unwrap();
        lib.db
            .execute(
                "INSERT INTO spotify_analysis(uri,version,metadata,updated_at) VALUES('spotify:track:2222222222222222222222',3,'{}',0)",
                [],
            )
            .unwrap();
        assert_eq!(lib.features_pending().unwrap(), 3);
        let pending = lib.pending_features(16).unwrap();
        assert_eq!(pending.len(), 3);
        let computed: Vec<TrackFeatures> =
            pending.into_iter().map(PendingFeatures::compute).collect();
        lib.store_features(&computed).unwrap();
        assert_eq!(lib.features_pending().unwrap(), 0);
        assert!(lib.pending_features(16).unwrap().is_empty());
        let pool = cached_features(&lib);
        let old = &pool["/old.wav"];
        assert_eq!((old.bpm, old.camelot.as_deref()), (Some(122.0), Some("8A")));
        assert!(old.bpm_ambiguous && old.energy.is_some() && old.track_id.is_some());
        let cached = &pool[uri];
        assert_eq!(
            (cached.title.as_str(), cached.artist.as_str()),
            ("Cached", "Artist")
        );
        assert_eq!(
            (cached.bpm, cached.camelot.as_deref()),
            (Some(124.0), Some("9A"))
        );
        // An unreadable entry is stored as unusable so it is not retried.
        assert_eq!(pool["spotify:track:2222222222222222222222"].bpm, None);
    }

    /// Acceptance: suggestions update within about 50 ms of a track change on
    /// a 10,000-track library. Track loads keep the features current, so a
    /// request is the in-memory ranking; reading them from SQLite happens once.
    #[test]
    fn suggestions_over_ten_thousand_stored_tracks_are_fast() {
        use crate::suggest::{rank, EnergyDirection, Reference, LIMIT};
        use std::collections::HashSet;
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let keys = ["1A", "2A", "8A", "9A", "8B", "12B", "5A", "6B", "10A", "3B"];
        let rows: Vec<TrackFeatures> = (0..10_000)
            .map(|i| TrackFeatures {
                source: if i % 4 == 0 {
                    format!("spotify:track:{i:022}")
                } else {
                    format!("/Users/dj/Music/Library/Artist {i}/Track {i:05}.mp3")
                },
                track_id: None,
                title: format!("Track {i:05}"),
                artist: format!("Artist {i}"),
                duration_seconds: 180.0 + (i % 300) as f64,
                bpm: Some(70.0 + (i % 100) as f64 * 0.9),
                bpm_ambiguous: i % 9 == 0,
                camelot: Some(keys[i % keys.len()].into()),
                key_confidence: Some(0.25 + (i % 6) as f32 * 0.1),
                energy: Some((i % 97) as f32 / 97.0),
            })
            .collect();
        lib.store_features(&rows).unwrap();
        *lib.features.borrow_mut() = None;
        let started = std::time::Instant::now();
        assert_eq!(cached_features(&lib).len(), 10_000);
        let cold_ms = started.elapsed().as_secs_f64() * 1000.0;
        let reference = Reference {
            source: rows[1].source.clone(),
            title: "Playing".into(),
            bpm: Some(124.0),
            bpm_ambiguous: false,
            camelot: Some("8A".into()),
            key_confidence: Some(0.7),
            energy: Some(0.5),
            energy_level: Some(6),
        };
        let excluded = HashSet::from([rows[1].source.clone(), rows[2].source.clone()]);
        let played: HashSet<String> = rows.iter().take(200).map(|r| r.source.clone()).collect();
        let started = std::time::Instant::now();
        let runs = 5;
        for _ in 0..runs {
            let ranking = lib
                .with_features(|pool| {
                    rank(
                        pool.values(),
                        &reference,
                        EnergyDirection::Keep,
                        10.0,
                        &excluded,
                        &played,
                        LIMIT,
                    )
                })
                .unwrap();
            assert_eq!(ranking.suggestions.len(), LIMIT);
            assert!(ranking.suggestions.iter().take(10).all(|s| !s.key_clash));
        }
        let warm_ms = started.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        eprintln!("10,000 tracks: first load {cold_ms:.1} ms, request {warm_ms:.2} ms");
        assert!(warm_ms < 50.0, "{warm_ms:.2} ms per request");
        assert!(cold_ms < 2000.0, "{cold_ms:.0} ms to load features");
    }

    /// Prints the top suggestions for every track of a real library. It
    /// writes features into the database, so point it at a copy:
    /// `PULSEDECK_LIBRARY=/tmp/copy.sqlite3 cargo test -p pulsedeck -- --ignored --nocapture real_library`
    #[test]
    #[ignore = "needs PULSEDECK_LIBRARY set to a copy of a library database"]
    fn real_library_suggestions() {
        use crate::suggest::{energy_level, rank, EnergyDirection, Reference};
        use std::collections::HashSet;
        let Ok(path) = std::env::var("PULSEDECK_LIBRARY") else {
            return;
        };
        let lib = Library::open(Path::new(&path)).unwrap();
        let started = std::time::Instant::now();
        let prepared = lib.features_pending().unwrap();
        loop {
            let batch = lib.pending_features(64).unwrap();
            if batch.is_empty() {
                break;
            }
            let features: Vec<TrackFeatures> =
                batch.into_iter().map(PendingFeatures::compute).collect();
            lib.store_features(&features).unwrap();
        }
        eprintln!(
            "prepared {prepared} tracks in {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let pool = cached_features(&lib);
        let mut playing: Vec<&TrackFeatures> = pool.values().filter(|f| f.bpm.is_some()).collect();
        playing.sort_by(|a, b| a.title.cmp(&b.title));
        for track in playing {
            let reference = Reference {
                source: track.source.clone(),
                title: track.title.clone(),
                bpm: track.bpm,
                bpm_ambiguous: track.bpm_ambiguous,
                camelot: track.camelot.clone(),
                key_confidence: track.key_confidence,
                energy: track.energy,
                energy_level: track.energy.map(energy_level),
            };
            for (direction, range) in [(EnergyDirection::Keep, 6.0), (EnergyDirection::Raise, 16.0)]
            {
                let ranking = rank(
                    pool.values(),
                    &reference,
                    direction,
                    range,
                    &HashSet::from([track.source.clone()]),
                    &HashSet::new(),
                    4,
                );
                eprintln!(
                    "{} ({:.1} bpm, {}, energy {}) {direction:?} ±{range}%: {} in reach, {} out of range",
                    track.title,
                    track.bpm.unwrap_or_default(),
                    track.camelot.as_deref().unwrap_or("?"),
                    reference.energy_level.unwrap_or_default(),
                    ranking.considered,
                    ranking.out_of_range
                );
                for s in ranking.suggestions {
                    eprintln!("    {:.2}  {}  [{}]", s.score, s.title, s.reason);
                }
            }
        }
    }
}
