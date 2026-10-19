#[path = "library_tagging.rs"]
mod tagging;
pub use tagging::{DetailsPatch, LibraryFilters};

use crate::suggest::{PendingFeatures, TrackFeatures, FEATURES_VERSION};
use audio_analysis::{BeatModelStage, KeyCandidate, TempoCandidate, WaveformBands};
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
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub file_metadata: Vec<deck_engine::EmbeddedTag>,
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
    pub audio_measurements: Option<audio_analysis::AudioMeasurements>,
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
    pub cue_points: Vec<audio_analysis::CuePoint>,
    pub source_fingerprint: Option<String>,
    pub source_release: Option<String>,
    pub analysis_stages: audio_analysis::AnalysisStages,
    pub neural_rhythm: Option<audio_analysis::NeuralRhythm>,
    pub rhythm_error: Option<String>,
    pub manual_grid_anchor_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_ids: Vec<i64>,
}

pub use audio_analysis::CueCollection as SourceCues;

#[derive(Debug, Clone, Serialize)]
pub struct PlannerMember {
    pub source: String,
    pub title: String,
    pub artist: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Serialize)]
pub struct PlannerCoverage {
    pub scope: Option<String>,
    pub total_members: Option<usize>,
    pub indexed_members: usize,
    pub analysed_members: usize,
    pub needs_analysis_members: usize,
    pub complete: bool,
    pub needs_analysis: Vec<PlannerMember>,
    pub needs_analysis_truncated: bool,
}

pub struct Library {
    db: Connection,
    /// Suggestion features by source, loaded on first use and kept in step
    /// with every write below.
    features: RefCell<Option<HashMap<String, TrackFeatures>>>,
    /// Set once no stored track lacks current features; writes keep it so.
    features_complete: Cell<bool>,
    browser_stats: RefCell<HashMap<String, (usize, f64)>>,
    browser_epoch: Cell<(u64, i64)>,
    browser_matches: RefCell<Option<String>>,
    browser_filtered: RefCell<Option<String>>,
    browser_ordered: RefCell<Option<String>>,
}

/// Whether a cached Spotify analysis is complete enough to display and reuse.
pub fn usable_spotify_analysis(uri: &str, a: &deck_engine::TrackAnalysis) -> bool {
    a.path == uri
        && (2..=audio_analysis::ANALYSIS_VERSION).contains(&a.analysis_version)
        && !a.recording_allowed
        && a.waveform.len() == 4096
        && a.waveform_bands
            .as_ref()
            .is_some_and(|b| b.low.len() == 4096 && b.mid.len() == 4096 && b.high.len() == 4096)
}

const FEATURE_COLUMNS: &str =
    "source,track_id,title,artist,duration_seconds,bpm,bpm_ambiguous,camelot,key_confidence,energy,meter";

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
        meter: row.get(10)?,
    })
}

#[derive(Default)]
pub struct BrowserQuery {
    pub search: String,
    pub sort: String,
    pub playlist: Option<i64>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    pub offset: usize,
    pub limit: usize,
    pub filters: LibraryFilters,
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
        // Bound page caching and checkpoint smaller write batches to limit save stalls.
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-32768; PRAGMA wal_autocheckpoint=256;
          CREATE TABLE IF NOT EXISTS source_cues(source TEXT PRIMARY KEY, revision INTEGER NOT NULL, cues TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS tracks(
            id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
            artist TEXT NOT NULL DEFAULT '', duration_seconds REAL NOT NULL, bpm REAL NOT NULL,
            key_name TEXT, sample_rate INTEGER NOT NULL, waveform TEXT NOT NULL,
            added_at TEXT NOT NULL, hot_cues TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null,null]',
            beatgrid_offset_seconds REAL NOT NULL DEFAULT 0,
            memory_cues TEXT NOT NULL DEFAULT '[null,null,null,null,null,null,null,null,null,null]',
            cue_seconds REAL NOT NULL DEFAULT 0,camelot TEXT,key_confidence REAL);
          CREATE TABLE IF NOT EXISTS suggestion_memberships(scope TEXT NOT NULL,source TEXT NOT NULL,PRIMARY KEY(scope,source));
          CREATE TABLE IF NOT EXISTS suggestion_scopes(scope TEXT PRIMARY KEY,total INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS suggestion_scope_pages(scope TEXT NOT NULL,offset INTEGER NOT NULL,end_offset INTEGER NOT NULL,terminal INTEGER NOT NULL,PRIMARY KEY(scope,offset));
          CREATE TABLE IF NOT EXISTS suggestion_source_metadata(source TEXT PRIMARY KEY,title TEXT NOT NULL,artist TEXT NOT NULL,duration_seconds REAL NOT NULL);
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
            ("tags", "TEXT NOT NULL DEFAULT '[]'"),
            ("file_metadata", "TEXT NOT NULL DEFAULT '[]'"),
            ("metadata_search", "TEXT NOT NULL DEFAULT ''"),
            ("metadata_stamp", "TEXT NOT NULL DEFAULT ''"),
            ("metadata_error", "TEXT"),
        ] {
            if !columns.contains(name) {
                db.execute(
                    &format!("ALTER TABLE tracks ADD COLUMN {name} {declaration}"),
                    [],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        // Cover sorting and duration totals in the same scan, without track payloads.
        for (name, columns) in [
            ("tracks_browser_title", "lower(title),id,duration_seconds"),
            ("tracks_browser_bpm", "bpm,id,duration_seconds"),
            ("tracks_browser_duration", "duration_seconds,id"),
        ] {
            let sql: Option<String> = db
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
                    [name],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if sql.is_some_and(|sql| !sql.contains("duration_seconds")) {
                db.execute_batch(&format!("DROP INDEX {name}"))
                    .map_err(|e| e.to_string())?;
            }
            db.execute_batch(&format!(
                "CREATE INDEX IF NOT EXISTS {name} ON tracks({columns})"
            ))
            .map_err(|e| e.to_string())?;
        }
        // Keep list fields separate from waveform/neural JSON. The migration
        // touches each existing track once; triggers keep every writer in sync.
        db.execute_batch("CREATE TABLE IF NOT EXISTS browser_metadata(
            track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
            analysis_version INTEGER NOT NULL, bpm_source TEXT, bpm_ambiguous INTEGER NOT NULL, rhythm_error TEXT);
            INSERT INTO browser_metadata SELECT t.id,coalesce(json_extract(CASE WHEN json_valid(t.analysis_json) THEN t.analysis_json ELSE '{}' END,'$.analysis_version'),0),json_extract(CASE WHEN json_valid(t.analysis_json) THEN t.analysis_json ELSE '{}' END,'$.bpm_source'),coalesce(json_extract(CASE WHEN json_valid(t.analysis_json) THEN t.analysis_json ELSE '{}' END,'$.bpm_ambiguous'),0),json_extract(CASE WHEN json_valid(t.analysis_json) THEN t.analysis_json ELSE '{}' END,'$.rhythm_error') FROM tracks t LEFT JOIN browser_metadata b ON b.track_id=t.id WHERE b.track_id IS NULL;
            DROP TRIGGER IF EXISTS browser_metadata_insert;
            DROP TRIGGER IF EXISTS browser_metadata_update;
            CREATE TRIGGER browser_metadata_insert AFTER INSERT ON tracks BEGIN
              INSERT INTO browser_metadata VALUES(new.id,coalesce(json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.analysis_version'),0),json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.bpm_source'),coalesce(json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.bpm_ambiguous'),0),json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.rhythm_error')) ON CONFLICT(track_id) DO UPDATE SET analysis_version=excluded.analysis_version,bpm_source=excluded.bpm_source,bpm_ambiguous=excluded.bpm_ambiguous,rhythm_error=excluded.rhythm_error; END;
            CREATE TRIGGER browser_metadata_update AFTER UPDATE OF analysis_json ON tracks BEGIN
              INSERT INTO browser_metadata VALUES(new.id,coalesce(json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.analysis_version'),0),json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.bpm_source'),coalesce(json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.bpm_ambiguous'),0),json_extract(CASE WHEN json_valid(new.analysis_json) THEN new.analysis_json ELSE '{}' END,'$.rhythm_error')) ON CONFLICT(track_id) DO UPDATE SET analysis_version=excluded.analysis_version,bpm_source=excluded.bpm_source,bpm_ambiguous=excluded.bpm_ambiguous,rhythm_error=excluded.rhythm_error; END;
            ").map_err(|e|e.to_string())?;
        // Index the same lowercased text used by the literal predicate. Exact
        // trigram phrases then avoid rereading every broad-match document.
        let search_schema: Option<String> = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='browser_search'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if search_schema.is_some_and(|sql| !sql.contains("case_sensitive 1")) {
            db.execute_batch("DROP TABLE browser_search;")
                .map_err(|e| e.to_string())?;
        }
        db.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS browser_search USING fts5(text,tokenize='trigram case_sensitive 1');
          DROP TRIGGER IF EXISTS browser_search_insert;
          DROP TRIGGER IF EXISTS browser_search_update;
          INSERT INTO browser_search(rowid,text) SELECT t.id,lower(t.title||' '||t.artist||' '||t.bpm||' '||coalesce(t.key_name,'')||' '||t.tags||' '||t.metadata_search) FROM tracks t LEFT JOIN browser_search s ON s.rowid=t.id WHERE s.rowid IS NULL;
          CREATE TRIGGER IF NOT EXISTS browser_search_insert AFTER INSERT ON tracks BEGIN
            INSERT INTO browser_search(rowid,text) VALUES(new.id,lower(new.title||' '||new.artist||' '||new.bpm||' '||coalesce(new.key_name,'')||' '||new.tags||' '||new.metadata_search)); END;
          CREATE TRIGGER IF NOT EXISTS browser_search_update AFTER UPDATE OF title,artist,bpm,key_name,tags,metadata_search ON tracks WHEN old.title IS NOT new.title OR old.artist IS NOT new.artist OR old.bpm IS NOT new.bpm OR old.key_name IS NOT new.key_name OR old.tags IS NOT new.tags OR old.metadata_search IS NOT new.metadata_search BEGIN
            DELETE FROM browser_search WHERE rowid=old.id;
            INSERT INTO browser_search(rowid,text) VALUES(new.id,lower(new.title||' '||new.artist||' '||new.bpm||' '||coalesce(new.key_name,'')||' '||new.tags||' '||new.metadata_search)); END;
          CREATE TRIGGER IF NOT EXISTS browser_search_delete AFTER DELETE ON tracks BEGIN DELETE FROM browser_search WHERE rowid=old.id; END;")
          .map_err(|e|e.to_string())?;
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
        if db
            .prepare("SELECT name FROM pragma_table_info('suggest_features') WHERE name='meter'")
            .map_err(|e| e.to_string())?
            .query_row([], |r| r.get::<_, String>(0))
            .optional()
            .map_err(|e| e.to_string())?
            .is_none()
        {
            db.execute("ALTER TABLE suggest_features ADD COLUMN meter INTEGER", [])
                .map_err(|e| e.to_string())?;
        }
        crate::manual_cue_migration::run(&db)?;
        let library = Self::connected(db);
        library.initialize_tagging()?;
        Ok(library)
    }

    #[cfg(test)]
    pub fn remember_membership(
        &self,
        scope: &str,
        offset: u64,
        total: u64,
        sources: &[String],
    ) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        if offset == 0 {
            tx.execute("DELETE FROM suggestion_scope_pages WHERE scope=?1", [scope])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM suggestion_memberships WHERE scope=?1", [scope])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT INTO suggestion_scopes(scope,total) VALUES(?1,?2) ON CONFLICT(scope) DO UPDATE SET total=excluded.total",params![scope,total]).map_err(|e|e.to_string())?;
        for source in sources {
            tx.execute(
                "INSERT OR IGNORE INTO suggestion_memberships(scope,source) VALUES(?1,?2)",
                params![scope, source],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    /// Persist collection membership even when the provider has no deck-playability flag.
    /// Scanned positions distinguish filtered/duplicate provider entries from missing pages.
    pub fn remember_spotify_page(
        &self,
        scope: &str,
        page: &crate::spotify::SpotifyPage<crate::spotify::SpotifyTrack>,
    ) -> Result<(), String> {
        self.store_spotify_page(scope, page, false)
    }

    pub fn refresh_spotify_scope(
        &self,
        scope: &str,
        page: &crate::spotify::SpotifyPage<crate::spotify::SpotifyTrack>,
    ) -> Result<(), String> {
        if page.offset != 0 {
            return Err("A scope refresh must start at offset zero".into());
        }
        self.store_spotify_page(scope, page, true)
    }

    fn store_spotify_page(
        &self,
        scope: &str,
        page: &crate::spotify::SpotifyPage<crate::spotify::SpotifyTrack>,
        refresh: bool,
    ) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        // Browsing the first page must not erase an explicitly indexed full collection.
        if refresh {
            tx.execute("DELETE FROM suggestion_memberships WHERE scope=?1", [scope])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM suggestion_scope_pages WHERE scope=?1", [scope])
                .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT INTO suggestion_scopes(scope,total) VALUES(?1,?2) ON CONFLICT(scope) DO UPDATE SET total=excluded.total",params![scope,page.total]).map_err(|e|e.to_string())?;
        for track in &page.items {
            tx.execute(
                "INSERT OR IGNORE INTO suggestion_memberships(scope,source) VALUES(?1,?2)",
                params![scope, track.uri],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("INSERT INTO suggestion_source_metadata(source,title,artist,duration_seconds) VALUES(?1,?2,?3,?4) ON CONFLICT(source) DO UPDATE SET title=excluded.title,artist=excluded.artist,duration_seconds=excluded.duration_seconds",
                params![track.uri,track.title,track.artists.join(", "),track.duration_ms as f64 / 1000.0]).map_err(|e|e.to_string())?;
        }
        tx.execute("INSERT INTO suggestion_scope_pages(scope,offset,end_offset,terminal) VALUES(?1,?2,?3,?4) ON CONFLICT(scope,offset) DO UPDATE SET end_offset=excluded.end_offset,terminal=excluded.terminal",
            params![scope,page.offset,page.offset.saturating_add(page.fetched_count),page.next_offset.is_none()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    /// Coverage is metadata-only: asking for options never starts playback or analysis.
    pub fn planner_coverage(
        &self,
        scope: Option<&str>,
        sources: Option<&[String]>,
        provider: Option<&str>,
    ) -> Result<PlannerCoverage, String> {
        let (mut members, mut complete) = if let Some(scope) = scope {
            self.suggestion_scope(scope)?
        } else {
            let mut stmt = self.db.prepare("SELECT path FROM tracks UNION SELECT source FROM suggestion_source_metadata UNION SELECT uri FROM spotify_analysis").map_err(|e|e.to_string())?;
            let members = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|e| e.to_string())?;
            (members, true)
        };
        let mut total_members = if let Some(scope) = scope.filter(|s| s.starts_with("spotify:")) {
            self.db
                .query_row(
                    "SELECT total FROM suggestion_scopes WHERE scope=?1",
                    [scope],
                    |row| row.get::<_, usize>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
        } else {
            Some(members.len())
        };
        if let Some(sources) = sources {
            let explicit = self.normalize_suggestion_sources(sources)?;
            if scope.is_some() {
                members.retain(|source| explicit.contains(source));
            } else {
                members = explicit;
            }
            total_members = Some(members.len());
            // An explicit list is fully known even if the provider collection is partial.
            if scope.is_none() {
                complete = true;
            }
        }
        match provider {
            None | Some("all") => {}
            Some("local") => members.retain(|source| !source.starts_with("spotify:")),
            Some("spotify") => members.retain(|source| source.starts_with("spotify:")),
            Some(_) => return Err("Provider must be all, local or spotify".into()),
        }
        if provider.is_some_and(|p| p != "all") && !scope.is_some_and(|s| s.starts_with("spotify:"))
        {
            total_members = Some(members.len());
        }
        // Imported BPM tags alone are not evidence that audio has been analysed.
        let mut ready = self.db.prepare("SELECT t.path FROM tracks t JOIN browser_metadata b ON b.track_id=t.id WHERE b.analysis_version BETWEEN 2 AND ?1 AND t.bpm BETWEEN 30 AND 300 AND t.duration_seconds>0 UNION SELECT s.uri FROM spotify_analysis s JOIN suggest_features f ON f.source=s.uri WHERE s.version BETWEEN 2 AND ?1 AND f.version=?2 AND f.bpm BETWEEN 30 AND 300 AND f.duration_seconds>0").map_err(|e|e.to_string())?;
        let analysed = ready
            .query_map(
                params![audio_analysis::ANALYSIS_VERSION, FEATURES_VERSION],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .map_err(|e| e.to_string())?;
        let analysed = analysed
            .intersection(&members)
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let needs_analysis_members = members.len().saturating_sub(analysed.len());
        let mut unknown = members
            .iter()
            .filter(|s| !analysed.contains(*s))
            .collect::<Vec<_>>();
        unknown.sort_unstable();
        let mut needs_analysis = Vec::new();
        let mut metadata = self.db.prepare("SELECT title,artist,duration_seconds FROM tracks WHERE path=?1 UNION ALL SELECT title,artist,duration_seconds FROM suggestion_source_metadata WHERE source=?1 LIMIT 1").map_err(|e|e.to_string())?;
        for source in unknown.into_iter().take(100) {
            let member = metadata
                .query_row([source], |row| {
                    Ok(PlannerMember {
                        source: source.clone(),
                        title: row.get(0)?,
                        artist: row.get(1)?,
                        duration_seconds: row.get(2)?,
                    })
                })
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| PlannerMember {
                    source: source.clone(),
                    title: source.clone(),
                    artist: String::new(),
                    duration_seconds: 0.0,
                });
            needs_analysis.push(member);
        }
        Ok(PlannerCoverage {
            scope: scope.map(str::to_owned),
            total_members,
            indexed_members: members.len(),
            analysed_members: analysed.len(),
            needs_analysis_members,
            complete,
            needs_analysis_truncated: needs_analysis_members > needs_analysis.len(),
            needs_analysis,
        })
    }

    /// Resolve source selectors without deserializing track analysis or waveform payloads.
    pub fn normalize_suggestion_sources(
        &self,
        sources: &[String],
    ) -> Result<std::collections::HashSet<String>, String> {
        let mut normalized = std::collections::HashSet::new();
        for source in sources {
            let source = if let Some(id) = source.strip_prefix("local:") {
                let id: i64 = id
                    .parse()
                    .map_err(|_| "Explicit local source must be local:TRACK_ID")?;
                self.db
                    .query_row("SELECT path FROM tracks WHERE id=?1", [id], |row| {
                        row.get::<_, String>(0)
                    })
                    .optional()
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Unknown explicit local source local:{id}"))?
            } else if source.starts_with("spotify:track:") {
                crate::spotify_analysis::validate_uri(source)?;
                source.clone()
            } else if std::path::Path::new(source).is_absolute() {
                source.clone()
            } else {
                return Err("Explicit sources must be local:TRACK_ID, an absolute local path or Spotify track URI".into());
            };
            normalized.insert(source);
        }
        Ok(normalized)
    }
    pub fn suggestion_scope(
        &self,
        scope: &str,
    ) -> Result<(std::collections::HashSet<String>, bool), String> {
        if let Some(id) = scope.strip_prefix("local:") {
            let id: i64 = id.parse().map_err(|_| "Expected local:PLAYLIST_ID")?;
            let playlists = self.playlists()?;
            let playlist = playlists
                .iter()
                .find(|p| p.id == id)
                .ok_or("Unknown local playlist")?;
            let mut stmt=self.db.prepare("SELECT t.path FROM tracks t JOIN playlist_tracks p ON p.track_id=t.id WHERE p.playlist_id=?1 ORDER BY p.position").map_err(|e|e.to_string())?;
            let sources = stmt
                .query_map([playlist.id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            return Ok((sources, true));
        }
        if scope != "spotify:liked" && !scope.starts_with("spotify:playlist:") {
            return Err("Playlist must be local:ID, spotify:liked or spotify:playlist:ID".into());
        }
        let total: Option<usize> = self
            .db
            .query_row(
                "SELECT total FROM suggestion_scopes WHERE scope=?1",
                [scope],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let mut stmt = self
            .db
            .prepare("SELECT source FROM suggestion_memberships WHERE scope=?1")
            .map_err(|e| e.to_string())?;
        let sources: std::collections::HashSet<String> = stmt
            .query_map([scope], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let mut complete = total.is_some_and(|n| sources.len() >= n);
        let mut pages = self.db.prepare("SELECT offset,end_offset,terminal FROM suggestion_scope_pages WHERE scope=?1 ORDER BY offset").map_err(|e|e.to_string())?;
        let page_ranges = pages
            .query_map([scope], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if !page_ranges.is_empty() {
            complete = false;
            let mut cursor = 0;
            for (offset, end, terminal) in page_ranges {
                if offset > cursor {
                    break;
                }
                cursor = cursor.max(end);
                if terminal && total.is_some_and(|n| cursor >= n as u64) {
                    complete = true;
                }
            }
        }
        Ok((sources, complete))
    }
    pub fn export_xml(&self) -> Result<String, String> {
        let mut statement=self.db.prepare("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,'[]',added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,'{}',tags,'[]' FROM tracks ORDER BY id").map_err(|e|e.to_string())?;
        let tracks = statement
            .query_map([], Self::row)
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let cues: HashMap<String, SourceCues> = tracks
            .iter()
            .map(|t| self.source_cues(&t.path).map(|(_, c)| (t.path.clone(), c)))
            .collect::<Result<_, _>>()?;
        let mut attributes = self.all_xml_details()?;
        for track in &tracks {
            if let Some(attrs) = attributes.get_mut(&track.id) {
                attrs.insert(
                    "PulseDeckCues".into(),
                    serde_json::to_string(&cues[&track.path]).map_err(|e| e.to_string())?,
                );
            }
        }
        let xml = export_xml_named(&tracks, &self.playlists()?, &cues, &attributes)?;
        let taxonomy = serde_json::to_string(&self.tag_categories()?).map_err(|e| e.to_string())?;
        Ok(xml.replacen(
            "</COLLECTION>",
            &format!(
                "</COLLECTION><PULSEDECK_TAGS>{}</PULSEDECK_TAGS>",
                quick_xml::escape::escape(&taxonomy)
            ),
            1,
        ))
    }
    fn connected(db: Connection) -> Self {
        Self {
            db,
            features: RefCell::new(None),
            features_complete: Cell::new(false),
            browser_stats: RefCell::new(HashMap::new()),
            browser_epoch: Cell::new((0, 0)),
            browser_matches: RefCell::new(None),
            browser_filtered: RefCell::new(None),
            browser_ordered: RefCell::new(None),
        }
    }
    pub fn database_path(&self) -> Result<String, String> {
        self.db
            .path()
            .map(str::to_owned)
            .ok_or_else(|| "Library database path unavailable".into())
    }
    pub fn export_readonly(path: &Path) -> Result<String, String> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        db.execute_batch("BEGIN").map_err(|e| e.to_string())?;
        Self::connected(db).export_xml()
    }
    pub fn spotify_analysis(
        &self,
        uri: &str,
    ) -> Result<Option<deck_engine::TrackAnalysis>, String> {
        let json: Option<String> = self
            .db
            .query_row(
                "SELECT metadata FROM spotify_analysis WHERE uri=?1 AND version BETWEEN 2 AND ?2",
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
    ) -> Result<bool, String> {
        if !analysis.path.starts_with("spotify:track:") || analysis.recording_allowed {
            return Err("Only Spotify analysis metadata can enter this cache".into());
        }
        let json = serde_json::to_string(analysis).map_err(|e| e.to_string())?;
        let changed=self.db.execute("INSERT INTO spotify_analysis(uri,version,metadata,updated_at) VALUES(?1,?2,?3,unixepoch()) ON CONFLICT(uri) DO UPDATE SET version=excluded.version,metadata=excluded.metadata,updated_at=excluded.updated_at WHERE spotify_analysis.metadata<>excluded.metadata OR spotify_analysis.version<>excluded.version", params![analysis.path, analysis.analysis_version, json]).map_err(|e| e.to_string())?;
        if changed == 0 {
            return Ok(false);
        }
        self.store_features(&[if usable_spotify_analysis(&analysis.path, analysis) {
            TrackFeatures::from_analysis(analysis)
        } else {
            TrackFeatures::unusable(analysis.path.clone())
        }])?;
        Ok(true)
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
                     SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12
                     WHERE ?3 IS NULL OR EXISTS(SELECT 1 FROM tracks WHERE id=?3)
                     ON CONFLICT(source) DO UPDATE SET version=excluded.version,track_id=excluded.track_id,
                       title=excluded.title,artist=excluded.artist,duration_seconds=excluded.duration_seconds,
                       bpm=excluded.bpm,bpm_ambiguous=excluded.bpm_ambiguous,camelot=excluded.camelot,
                       key_confidence=excluded.key_confidence,energy=excluded.energy,meter=excluded.meter"
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
                        f.energy,
                        f.meter,
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
                 FROM (SELECT id FROM tracks WHERE NOT EXISTS(SELECT 1 FROM suggest_features f WHERE f.source=tracks.path AND f.version=?1) LIMIT ?2) pending
                 CROSS JOIN tracks t ON t.id=pending.id",
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

    pub fn track_ids(&self) -> Result<Vec<i64>, String> {
        let mut query = self
            .db
            .prepare("SELECT id FROM tracks ORDER BY added_at DESC,id DESC")
            .map_err(|e| e.to_string())?;
        let result = query
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        result
    }
    pub fn path_for_id(&self, id: i64) -> Result<String, String> {
        self.db
            .query_row("SELECT path FROM tracks WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())
    }

    pub fn metadata_candidates(
        &self,
        after: i64,
    ) -> Result<Vec<crate::metadata_index::Candidate>, String> {
        let mut statement = self
            .db
            .prepare("SELECT id,path,metadata_stamp FROM tracks WHERE id>?1 ORDER BY id LIMIT 128")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([after], |row| {
                Ok(crate::metadata_index::Candidate {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    stamp: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    pub fn metadata_stamp(&self, id: i64) -> Result<String, String> {
        self.db
            .query_row("SELECT metadata_stamp FROM tracks WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())
    }

    /// Update only file metadata; optimistic stamps reject superseded readers.
    pub fn store_file_metadata(
        &self,
        updates: &[crate::metadata_index::Update],
    ) -> Result<usize, String> {
        let transaction = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut changed = 0;
        for update in updates {
            let candidate = &update.candidate;
            let written = match &update.fields {
                Ok(fields) => {
                    let search = crate::metadata_index::search_text(fields);
                    transaction.execute("UPDATE tracks SET file_metadata=?1,metadata_search=?2,metadata_stamp=?3,metadata_error=NULL WHERE id=?4 AND path=?5 AND metadata_stamp=?6",
                        params![serde_json::to_string(fields).map_err(|e| e.to_string())?,search,update.stamp,candidate.id,candidate.path,candidate.stamp])
                }
                Err(error) => transaction.execute("UPDATE tracks SET metadata_stamp=?1,metadata_error=?2 WHERE id=?3 AND path=?4 AND metadata_stamp=?5",
                    params![update.stamp,error,candidate.id,candidate.path,candidate.stamp]),
            }.map_err(|e| e.to_string())?;
            changed += written;
            if written > 0 && update.fields.is_ok() {
                self.refresh_details(candidate.id)?;
            }
        }
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(changed)
    }

    /// Page only list metadata in SQLite. Waveforms, neural arrays and cues are
    /// retrieved through the selected-source APIs, never hydrated for scrolling.
    pub fn browser_page(&self, q: &BrowserQuery) -> Result<serde_json::Value, String> {
        let order = match q.sort.as_str() {
            "title" | "" => "lower(t.title),t.id",
            "bpm" => "t.bpm,t.id",
            "duration" => "t.duration_seconds,t.id",
            _ => return Err("Sort must be title, bpm or duration".into()),
        };
        if let Some(id) = q.playlist {
            let exists: bool = self
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                return Err("Playlist not found".into());
            }
        }
        let epoch = (
            self.db.total_changes(),
            self.db
                .query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0))
                .map_err(|e| e.to_string())?,
        );
        if self.browser_epoch.replace(epoch) != epoch {
            self.browser_stats.borrow_mut().clear();
            self.browser_matches.borrow_mut().take();
            self.browser_filtered.borrow_mut().take();
            self.browser_ordered.borrow_mut().take();
        }
        // Keep one filtered ID set in SQLite; repeated pages must not rebuild
        // broad FTS matches or read large track payloads just to filter them.
        let mut clauses = vec!["1".to_owned()];
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if !q.search.is_empty() {
            let search = q.search.to_lowercase();
            if self.browser_matches.borrow().as_deref() != Some(search.as_str()) {
                self.browser_matches.borrow_mut().take();
                self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS browser_matches(id INTEGER PRIMARY KEY); DELETE FROM browser_matches;").map_err(|e| e.to_string())?;
                if search.chars().count() >= 3 {
                    let phrase = format!("\"{}\"", search.replace('"', "\"\""));
                    self.db.execute("INSERT INTO browser_matches SELECT rowid FROM browser_search WHERE browser_search MATCH ?1", [phrase]).map_err(|e| e.to_string())?;
                } else {
                    self.db.execute("INSERT INTO browser_matches SELECT rowid FROM browser_search WHERE instr(text,?1)>0", [&search]).map_err(|e| e.to_string())?;
                }
                self.db.execute("INSERT OR IGNORE INTO browser_matches SELECT track_id FROM track_my_tags WHERE tag_id IN(SELECT m.id FROM my_tags m JOIN tag_categories c ON c.id=m.category_id WHERE instr(m.search_name,?1)>0 OR instr(c.search_name,?1)>0)", [&search]).map_err(|e|e.to_string())?;
                *self.browser_matches.borrow_mut() = Some(search);
                // Temp-cache writes are not changes to library data.
                self.browser_epoch.set((self.db.total_changes(), epoch.1));
            }
            clauses.push("t.id IN (SELECT id FROM browser_matches)".into());
        }
        if let Some(id) = q.playlist {
            values.push(id.into());
            clauses.push(format!(
                "t.id IN (SELECT track_id FROM playlist_tracks WHERE playlist_id=?{})",
                values.len()
            ));
        }
        for (op, value) in [(">=", q.bpm_min), ("<=", q.bpm_max)] {
            if let Some(value) = value {
                values.push(value.into());
                clauses.push(format!("t.bpm{op}?{}", values.len()));
            }
        }
        let facets = self.filter_sql(&q.filters, &mut values)?;
        let mut filter = clauses.join(" AND ");
        let stats_key = format!(
            "{}:{:?}:{:?}:{:?}:{}",
            q.search,
            q.playlist,
            q.bpm_min,
            q.bpm_max,
            serde_json::to_string(&q.filters).map_err(|e| e.to_string())?
        );
        let filtered = q.playlist.is_some()
            || q.bpm_min.is_some()
            || q.bpm_max.is_some()
            || !q.filters.my_tags.is_empty()
            || !q.filters.colors.is_empty()
            || q.filters.rating_min.is_some()
            || q.filters.rating_max.is_some();
        if filtered {
            if self.browser_filtered.borrow().as_deref() != Some(stats_key.as_str()) {
                self.browser_filtered.borrow_mut().take();
                self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS browser_filtered(id INTEGER PRIMARY KEY); DELETE FROM browser_filtered;").map_err(|e|e.to_string())?;
                let source = match &facets {
                    Some(ids) if q.bpm_min.is_none() && q.bpm_max.is_none() => format!("({ids}) t"),
                    Some(ids) => format!("({ids}) f JOIN tracks t ON t.id=f.id"),
                    None => "tracks t".into(),
                };
                self.db
                    .execute(
                        &format!(
                            "INSERT INTO browser_filtered SELECT t.id FROM {source} WHERE {filter}"
                        ),
                        rusqlite::params_from_iter(values.iter()),
                    )
                    .map_err(|e| e.to_string())?;
                *self.browser_filtered.borrow_mut() = Some(stats_key.clone());
                self.browser_epoch.set((self.db.total_changes(), epoch.1));
            }
            filter = "t.id IN(SELECT id FROM browser_filtered)".into();
            values.clear();
        }
        let cached_stats = self.browser_stats.borrow().get(&stats_key).copied();
        let total = if let Some((total, _)) = cached_stats {
            total
        } else {
            let source = if filtered {
                "browser_filtered"
            } else if !q.search.is_empty() {
                "browser_matches"
            } else {
                "tracks"
            };
            self.db
                .query_row(&format!("SELECT count(*) FROM {source}"), [], |r| {
                    r.get::<_, usize>(0)
                })
                .map_err(|e| e.to_string())?
        };
        let ranked = total > 1000;
        if ranked {
            let key = format!("{stats_key}:{}", q.sort);
            if self.browser_ordered.borrow().as_deref() != Some(key.as_str()) {
                self.browser_ordered.borrow_mut().take();
                self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS browser_ordered(position INTEGER PRIMARY KEY,track_id INTEGER NOT NULL,duration_seconds REAL NOT NULL); DELETE FROM browser_ordered;").map_err(|e|e.to_string())?;
                let index = match q.sort.as_str() {
                    "bpm" => "tracks_browser_bpm",
                    "duration" => "tracks_browser_duration",
                    _ => "tracks_browser_title",
                };
                self.db.execute(&format!("INSERT INTO browser_ordered(track_id,duration_seconds) SELECT t.id,t.duration_seconds FROM tracks t INDEXED BY {index} WHERE {filter} ORDER BY {order}"),rusqlite::params_from_iter(values.iter())).map_err(|e|e.to_string())?;
                *self.browser_ordered.borrow_mut() = Some(key);
                self.browser_epoch.set((self.db.total_changes(), epoch.1));
            }
        }
        let duration = if let Some((_, duration)) = cached_stats {
            duration
        } else {
            // Reuse the compact ordered rows instead of scanning a second track index.
            let duration = if ranked {
                self.db.query_row(
                    "SELECT coalesce(sum(duration_seconds),0) FROM browser_ordered",
                    [],
                    |r| r.get::<_, f64>(0),
                )
            } else {
                self.db.query_row(
                    &format!(
                        "SELECT coalesce(sum(duration_seconds),0) FROM tracks t WHERE {filter}"
                    ),
                    rusqlite::params_from_iter(values.iter()),
                    |r| r.get::<_, f64>(0),
                )
            }
            .map_err(|e| e.to_string())?;
            let mut cache = self.browser_stats.borrow_mut();
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(stats_key.clone(), (total, duration));
            duration
        };
        let limit = q.limit.clamp(1, 1000);
        let mut page_source = "tracks t".to_owned();
        let mut page_order = order;
        let suffix;
        if ranked {
            // Range reads avoid walking every earlier match at deep scroll offsets.
            page_source = "browser_ordered p CROSS JOIN tracks t ON t.id=p.track_id".into();
            page_order = "p.position";
            filter = "p.position>?1".into();
            values = vec![
                (q.offset.min(i64::MAX as usize) as i64).into(),
                (limit as i64).into(),
            ];
            suffix = "LIMIT ?2".into();
        } else {
            values.push((limit as i64).into());
            let limit_parameter = values.len();
            values.push((q.offset.min(i64::MAX as usize) as i64).into());
            let offset_parameter = values.len();
            suffix = format!("LIMIT ?{limit_parameter} OFFSET ?{offset_parameter}");
        }
        let sql = format!("SELECT t.id,t.path,t.title,t.artist,t.duration_seconds,t.bpm,t.key_name,t.sample_rate,t.added_at,t.camelot,t.key_confidence,t.tags,coalesce(b.analysis_version,0),b.bpm_source,coalesce(b.bpm_ambiguous,0),b.rhythm_error,coalesce(i.rating,0),coalesce(i.color,'') FROM {page_source} LEFT JOIN browser_metadata b ON b.track_id=t.id LEFT JOIN track_info i ON i.track_id=t.id WHERE {filter} ORDER BY {page_order} {suffix}");
        let mut statement = self.db.prepare(&sql).map_err(|e| e.to_string())?;
        let tracks = statement.query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok(serde_json::json!({
                "id":r.get::<_,i64>(0)?,"path":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"artist":r.get::<_,String>(3)?,
                "duration_seconds":r.get::<_,f64>(4)?,"bpm":r.get::<_,f64>(5)?,"key":r.get::<_,Option<String>>(6)?,"sample_rate":r.get::<_,u32>(7)?,"added_at":r.get::<_,String>(8)?,
                "camelot":r.get::<_,Option<String>>(9)?,"key_confidence":r.get::<_,Option<f32>>(10)?,"tags":serde_json::from_str::<Vec<String>>(&r.get::<_,String>(11)?).unwrap_or_default(),
                "analysis_version":r.get::<_,u32>(12)?,"bpm_source":r.get::<_,Option<String>>(13)?,"bpm_ambiguous":r.get::<_,bool>(14)?,"rhythm_error":r.get::<_,Option<String>>(15)?,"rating":r.get::<_,u8>(16)?,"color":r.get::<_,String>(17)?
            }))
        }).map_err(|e| e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())?;
        Ok(
            serde_json::json!({"tracks":tracks,"total":total,"duration_seconds":duration,"offset":q.offset,"next_offset":(q.offset.saturating_add(limit)<total).then_some(q.offset.saturating_add(limit))}),
        )
    }

    pub fn tracks(&self) -> Result<Vec<TrackRecord>, String> {
        let mut stmt = self.db.prepare("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,analysis_json,tags,file_metadata FROM tracks ORDER BY added_at DESC,id DESC").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], Self::row).map_err(|e| e.to_string())?;
        rows.map(|row| {
            row.map_err(|e| e.to_string())
                .and_then(|record| self.project_record_cues(record))
        })
        .collect()
    }

    fn project_record_cues(&self, mut record: TrackRecord) -> Result<TrackRecord, String> {
        let json: Option<String> = self
            .db
            .query_row(
                "SELECT cues FROM source_cues WHERE source=?1",
                [&record.path],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let mut cues: SourceCues = if let Some(json) = json {
            serde_json::from_str(&json).map_err(|e| e.to_string())?
        } else {
            SourceCues {
                cue_seconds: record.cue_seconds,
                hot_cues: record.hot_cues.clone().try_into().unwrap_or([None; 8]),
                memory_cues: record.memory_cues.clone().try_into().unwrap_or([None; 10]),
                ..Default::default()
            }
        };
        cues.migrate();
        record.analysis.cue_points = cues.points;
        record.hot_cues = cues.hot_cues.to_vec();
        record.memory_cues = cues.memory_cues.to_vec();
        record.cue_seconds = cues.cue_seconds;
        Ok(record)
    }
    fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRecord> {
        Ok(TrackRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            artist: row.get(3)?,
            tags: serde_json::from_str(&row.get::<_, String>(17)?).unwrap_or_default(),
            file_metadata: serde_json::from_str(&row.get::<_, String>(18)?).unwrap_or_default(),
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

    /// Local tracks missing current rhythm or PCM measurement stages.
    pub fn paths_needing_analysis(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT path FROM tracks WHERE json_extract(analysis_json,'$.source_fingerprint') IS NULL OR coalesce(json_extract(analysis_json,'$.beat_model.version'),0) < ?1 OR coalesce(json_extract(analysis_json,'$.audio_measurements.version'),0) < ?2 OR coalesce(json_extract(analysis_json,'$.audio_measurements.sample_rate'),0) != sample_rate OR coalesce(json_extract(analysis_json,'$.audio_measurements.frames'),-1) != CAST(round(duration_seconds*sample_rate) AS INTEGER) ORDER BY added_at DESC,id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                [
                    audio_analysis::BEAT_MODEL_VERSION,
                    audio_analysis::MEASUREMENTS_VERSION,
                ],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn track_by_path(&self, path: &str) -> Result<Option<TrackRecord>, String> {
        self.db.query_row("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,analysis_json,tags,file_metadata FROM tracks WHERE path=?1",[path],Self::row).optional().map_err(|e|e.to_string())?.map(|record|self.project_record_cues(record)).transpose()
    }

    /// Register a file without replacing its saved analysis, tags or cues.
    pub fn import_metadata(&self, track: &TrackRecord) -> Result<TrackRecord, String> {
        if let Some(existing) = self.track_by_path(&track.path)? {
            return Ok(existing);
        }
        self.upsert(track)
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
                if track.analysis.beatgrid_source.as_deref() != Some("manual") {
                    metadata.manual_grid_anchor_index = previous.analysis.manual_grid_anchor_index;
                }
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
        self.db.execute("INSERT INTO tracks(path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,camelot,key_confidence,analysis_json,beatgrid_offset_seconds,tags)
          VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
          ON CONFLICT(path) DO UPDATE SET title=excluded.title,artist=excluded.artist,duration_seconds=excluded.duration_seconds,
          bpm=excluded.bpm,key_name=excluded.key_name,sample_rate=excluded.sample_rate,waveform=excluded.waveform,camelot=excluded.camelot,key_confidence=excluded.key_confidence,analysis_json=excluded.analysis_json,beatgrid_offset_seconds=excluded.beatgrid_offset_seconds",
          params![track.path,track.title,track.artist,track.duration_seconds,bpm,track.key,track.sample_rate,waveform,now,track.camelot,track.key_confidence,analysis_json,grid_offset,serde_json::to_string(&track.tags).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        let id: i64 = self
            .db
            .query_row("SELECT id FROM tracks WHERE path=?1", [&track.path], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        self.refresh_details(id)?;
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

    pub fn set_tags(&self, id: i64, tags: Vec<String>) -> Result<TrackRecord, String> {
        if tags.len() > 32 {
            return Err("Invalid tags: use at most 32 tags per song".into());
        }
        let mut normalized = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for tag in tags {
            let tag = tag.trim();
            if tag.chars().count() > 128 {
                return Err("Invalid tag: use at most 128 characters".into());
            }
            if !tag.is_empty() && seen.insert(tag.to_lowercase()) {
                normalized.push(tag.to_owned());
            }
        }
        if self
            .db
            .execute(
                "UPDATE tracks SET tags=?1 WHERE id=?2",
                params![
                    serde_json::to_string(&normalized).map_err(|e| e.to_string())?,
                    id
                ],
            )
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("Track not found".into());
        }
        self.project_record_cues(self.db.query_row("SELECT id,path,title,artist,duration_seconds,bpm,key_name,sample_rate,waveform,added_at,hot_cues,beatgrid_offset_seconds,memory_cues,cue_seconds,camelot,key_confidence,analysis_json,tags,file_metadata FROM tracks WHERE id=?1", [id], Self::row).map_err(|e| e.to_string())?)
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

    pub fn manual_grid(
        &self,
        path: &str,
        offset: f64,
        anchor: Option<usize>,
    ) -> Result<(), String> {
        let transaction = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        self.metadata(path, None, None, Some(offset))?;
        if let Some(record) = self.track_by_path(path)? {
            let mut analysis = record.analysis;
            analysis.manual_grid_anchor_index = anchor;
            self.db
                .execute(
                    "UPDATE tracks SET analysis_json=?1 WHERE path=?2",
                    params![
                        serde_json::to_string(&analysis).map_err(|e| e.to_string())?,
                        path
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
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

    pub fn tags(
        &self,
        path: &str,
        title: &str,
        artist: &str,
        key: Option<&str>,
    ) -> Result<(), String> {
        self.db.execute("UPDATE tracks SET title=coalesce((SELECT json_extract(overrides,'$.title') FROM track_info WHERE track_id=tracks.id),CASE WHEN ?1='' THEN title ELSE ?1 END),artist=coalesce((SELECT json_extract(overrides,'$.artist') FROM track_info WHERE track_id=tracks.id),CASE WHEN ?2='' THEN artist ELSE ?2 END),key_name=COALESCE(?3,key_name) WHERE path=?4",params![title,artist,key,path]).map_err(|e|e.to_string())?;
        self.db.execute("UPDATE suggest_features SET title=(SELECT title FROM tracks WHERE path=?1),artist=(SELECT artist FROM tracks WHERE path=?1) WHERE source=?1",[path]).map_err(|e|e.to_string())?;
        self.features.borrow_mut().take();
        Ok(())
    }

    pub fn source_cues(&self, source: &str) -> Result<(u64, SourceCues), String> {
        let row: Option<(u64, String)> = self
            .db
            .query_row(
                "SELECT revision,cues FROM source_cues WHERE source=?1",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((revision, json)) = row {
            let mut cues: SourceCues = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            cues.migrate();
            return Ok((revision, cues));
        }
        let mut cues = SourceCues::default();
        let legacy: Option<(String, String, f64)> = self
            .db
            .query_row(
                "SELECT hot_cues,memory_cues,cue_seconds FROM tracks WHERE path=?1",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((hot, memory, main)) = legacy {
            cues.hot_cues = serde_json::from_str(&hot).map_err(|e| e.to_string())?;
            cues.memory_cues = serde_json::from_str(&memory).map_err(|e| e.to_string())?;
            cues.cue_seconds = main;
        }
        cues.migrate();
        Ok((0, cues))
    }
    pub fn save_source_cues(
        &self,
        source: &str,
        cues: &SourceCues,
        expected: Option<u64>,
    ) -> Result<u64, String> {
        let mut normalized = cues.clone();
        let (_, old) = self.source_cues(source)?;
        // Legacy callers change only projections; canonical callers change point records.
        if old.points == normalized.points && old.bindings == normalized.bindings {
            normalized.reconcile_legacy(
                cues.hot_cues,
                cues.memory_cues,
                cues.cue_seconds,
                &cues.annotations,
            );
        } else {
            normalized.normalize_memory();
        }
        normalized.validate(f64::MAX)?;
        let cues = &normalized;
        if cues
            .annotations
            .iter()
            .any(|(key, value)| key.len() > 80 || value.len() > 256)
        {
            return Err(
                "Cue annotation keys must be at most 80 bytes and labels at most 256 bytes".into(),
            );
        }
        if !cues.cue_seconds.is_finite()
            || cues.cue_seconds < 0.0
            || cues
                .hot_cues
                .iter()
                .chain(cues.memory_cues.iter())
                .flatten()
                .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("Cue times must be finite and nonnegative".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let (revision, old) = self.source_cues(source)?;
        if expected.is_some_and(|r| r != revision) {
            return Err("Cue metadata revision conflict".into());
        }
        if old == *cues {
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(revision);
        }
        let next = revision + 1;
        let mut durable = serde_json::to_value(cues).map_err(|e| e.to_string())?;
        for key in ["hot_cues", "memory_cues", "cue_seconds", "annotations"] {
            durable.as_object_mut().unwrap().remove(key);
        }
        let json = serde_json::to_string(&durable).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO source_cues(source,revision,cues) VALUES(?1,?2,?3) ON CONFLICT(source) DO UPDATE SET revision=excluded.revision,cues=excluded.cues", params![source,next,json]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE tracks SET hot_cues=?1,memory_cues=?2,cue_seconds=?3 WHERE path=?4",
            params![
                serde_json::to_string(&cues.hot_cues).map_err(|e| e.to_string())?,
                serde_json::to_string(&cues.memory_cues).map_err(|e| e.to_string())?,
                cues.cue_seconds,
                source
            ],
        )
        .map_err(|e| e.to_string())?;
        if let Some(record) = self.track_by_path(source)? {
            let mut analysis = record.analysis;
            analysis.cue_points = cues.points.clone();
            tx.execute(
                "UPDATE tracks SET analysis_json=?1 WHERE path=?2",
                params![
                    serde_json::to_string(&analysis).map_err(|e| e.to_string())?,
                    source
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        // Keep the cached projection consistent with the canonical source store.
        // Otherwise unloaded Spotify reads can expose deleted cues until reload.
        tx.execute(
            "UPDATE spotify_analysis SET metadata=json_set(metadata,'$.cue_points',json(?1)) WHERE uri=?2",
            params![serde_json::to_string(&cues.points).map_err(|e| e.to_string())?, source],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(next)
    }
    pub fn playlist_edit(
        &self,
        id: i64,
        action: &str,
        name: Option<&str>,
        tracks: &[i64],
    ) -> Result<(), String> {
        if !self.playlists()?.iter().any(|p| p.id == id) {
            return Err("Playlist not found".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        match action {
            "rename" => {
                let name = name.unwrap_or("").trim();
                if name.is_empty() || name.len() > 200 {
                    return Err("Choose a playlist name between 1 and 200 characters".into());
                }
                tx.execute(
                    "UPDATE playlists SET name=?1 WHERE id=?2",
                    params![name, id],
                )
                .map_err(|e| e.to_string())?;
            }
            "delete" => {
                tx.execute("DELETE FROM playlists WHERE id=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
            "remove" => {
                for track in tracks {
                    tx.execute(
                        "DELETE FROM playlist_tracks WHERE playlist_id=?1 AND track_id=?2",
                        params![id, track],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            "replace" => {
                let mut unique = std::collections::HashSet::new();
                if tracks.iter().any(|t| !unique.insert(*t)) {
                    return Err("A playlist cannot contain duplicate tracks".into());
                }
                tx.execute("DELETE FROM playlist_tracks WHERE playlist_id=?1", [id])
                    .map_err(|e| e.to_string())?;
                for (position, track) in tracks.iter().enumerate() {
                    tx.execute("INSERT INTO playlist_tracks(playlist_id,track_id,position) VALUES(?1,?2,?3)",params![id,track,position as i64]).map_err(|e|e.to_string())?;
                }
            }
            _ => return Err("Unknown playlist action".into()),
        }
        tx.commit().map_err(|e| e.to_string())
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
    #[serde(rename = "PULSEDECK_TAGS", default)]
    pub taxonomy: Option<String>,
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
    #[serde(skip)]
    pub attributes: std::collections::BTreeMap<String, String>,
}
#[derive(Debug, Default, Deserialize)]
pub struct XmlMark {
    #[serde(rename = "@Name", default)]
    pub name: String,
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
    let mut parsed: RekordboxXml =
        quick_xml::de::from_str(&text).map_err(|e| format!("Invalid rekordbox XML: {e}"))?;
    let mut reader = quick_xml::Reader::from_str(&text);
    let mut collection = false;
    let mut index = 0;
    loop {
        use quick_xml::events::Event;
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) if e.name().as_ref() == b"COLLECTION" => collection = true,
            Event::End(e) if e.name().as_ref() == b"COLLECTION" => collection = false,
            Event::Start(e) | Event::Empty(e) if collection && e.name().as_ref() == b"TRACK" => {
                if let Some(track) = parsed.collection.tracks.get_mut(index) {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| e.to_string())?;
                        let key = std::str::from_utf8(attr.key.as_ref())
                            .map_err(|e| e.to_string())?
                            .to_owned();
                        track.attributes.insert(
                            key,
                            attr.decode_and_unescape_value(reader.decoder())
                                .map_err(|e| e.to_string())?
                                .into_owned(),
                        );
                    }
                }
                index += 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(parsed)
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

#[cfg(test)]
pub fn export_xml(tracks: &[TrackRecord], playlists: &[Playlist]) -> Result<String, String> {
    export_xml_named(tracks, playlists, &HashMap::new(), &HashMap::new())
}
fn export_xml_named(
    tracks: &[TrackRecord],
    playlists: &[Playlist],
    cues: &HashMap<String, SourceCues>,
    attributes: &HashMap<i64, std::collections::BTreeMap<String, String>>,
) -> Result<String, String> {
    fn escape(value: &str) -> String {
        quick_xml::escape::escape(
            value
                .chars()
                .filter(|c| !c.is_control() || ['\n', '\r', '\t'].contains(c))
                .collect::<String>(),
        )
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;")
    }
    let mut xml=String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\"><PRODUCT Name=\"PulseDeck\" Version=\"0.1.0\" Company=\"PulseDeck\"/><COLLECTION Entries=\"");
    xml.push_str(&tracks.len().to_string());
    xml.push_str("\">\n");
    for t in tracks {
        let location = url::Url::from_file_path(&t.path)
            .map_err(|_| "Track path must be absolute".to_string())?;
        xml.push_str(&format!("<TRACK TrackID=\"{}\" Name=\"{}\" Artist=\"{}\" TotalTime=\"{}\" AverageBpm=\"{}\" Tonality=\"{}\" Location=\"{}\">",t.id,escape(&t.title),escape(&t.artist),t.duration_seconds.round() as u64,t.bpm,escape(t.key.as_deref().unwrap_or("")),escape(location.as_str())));
        xml.pop();
        if let Some(attrs) = attributes.get(&t.id) {
            for (key, value) in attrs {
                if ![
                    "TrackID",
                    "Name",
                    "Artist",
                    "TotalTime",
                    "AverageBpm",
                    "Tonality",
                    "Location",
                ]
                .contains(&key.as_str())
                {
                    xml.push_str(&format!(" {}=\"{}\"", key, escape(value)));
                }
            }
        }
        xml.push('>');
        xml.push_str(&format!(
            "<TEMPO Inizio=\"{}\" Bpm=\"{}\" Metro=\"4/4\" Battito=\"1\"/>",
            t.beatgrid_offset_seconds, t.bpm
        ));
        for (index, cue) in t.hot_cues.iter().enumerate() {
            if let Some(seconds) = cue {
                xml.push_str(&format!(
                    "<POSITION_MARK Name=\"{}\" Type=\"0\" Start=\"{}\" Num=\"{}\"/>",
                    escape(
                        &cues
                            .get(&t.path)
                            .and_then(|c| c.annotations.get(&format!("hot:{index}")))
                            .cloned()
                            .unwrap_or_else(|| format!("Hot Cue {}", index + 1))
                    ),
                    seconds,
                    index
                ));
            }
        }
        for (index, cue) in t
            .memory_cues
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().map(|t| (i, t)))
        {
            xml.push_str(&format!(
                "<POSITION_MARK Name=\"{}\" Type=\"0\" Start=\"{}\" Num=\"-1\"/>",
                escape(
                    &cues
                        .get(&t.path)
                        .and_then(|c| c.annotations.get(&format!("memory:{index}")))
                        .cloned()
                        .unwrap_or_else(|| "Memory Cue".into())
                ),
                cue
            ));
        }
        if let Some(source) = cues.get(&t.path) {
            for point in &source.points {
                if source
                    .bindings
                    .hot
                    .iter()
                    .chain(source.bindings.memory.iter())
                    .flatten()
                    .any(|id| id == &point.id)
                {
                    continue;
                }
                xml.push_str(&format!(
                    "<POSITION_MARK Name=\"{}\" Type=\"0\" Start=\"{}\" Num=\"-1\"/>",
                    escape(&point.name),
                    point.start_seconds
                ));
            }
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
    #[test]
    fn reanalysis_can_persist_a_remapped_manual_anchor_without_losing_the_origin() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        lib.manual_grid(&saved.path, 1.4, Some(2)).unwrap();
        let mut updated = lib.track_by_path(&saved.path).unwrap().unwrap();
        updated.analysis.manual_grid_anchor_index = Some(3);
        let updated = lib.upsert(&updated).unwrap();
        assert_eq!(updated.beatgrid_offset_seconds, 1.4);
        assert_eq!(updated.analysis.manual_grid_anchor_index, Some(3));
    }
    #[test]
    fn exact_search_index_migrates_and_preserves_literal_semantics() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut seed = track();
        seed.title = "SUN SET 100% a_b \"quoted\" café CAFÉ 🪩🎵🎧".into();
        let saved = lib.upsert(&seed).unwrap();
        // Emulate an existing pre-optimization index, including its old text.
        lib.db.execute_batch("DROP TABLE browser_search; CREATE VIRTUAL TABLE browser_search USING fts5(text,tokenize='trigram'); INSERT INTO browser_search(rowid,text) SELECT id,title||' '||artist||' '||bpm||' '||coalesce(key_name,'')||' '||tags||' '||metadata_search FROM tracks;").unwrap();
        let lib = Library::from_connection(lib.db).unwrap();
        for search in [
            "SU",
            "SUN SET",
            "100%",
            "a_b",
            "\"quoted\"",
            "café",
            "CAFÉ",
            "🪩🎵🎧",
            "sun  set",
            "100_",
            "quoted OR absent",
        ] {
            let expected: usize = lib.db.query_row("SELECT count(*) FROM tracks WHERE instr(lower(title||' '||artist||' '||bpm||' '||coalesce(key_name,'')||' '||tags||' '||metadata_search),?1)>0", [search.to_lowercase()], |r| r.get(0)).unwrap();
            let page = lib
                .browser_page(&BrowserQuery {
                    search: search.into(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(page["total"], expected, "{search}");
        }
        assert_eq!(
            serde_json::to_value(lib.track_by_path(&seed.path).unwrap().unwrap()).unwrap(),
            serde_json::to_value(saved).unwrap()
        );
        lib.db
            .execute("UPDATE tracks SET title='NEW literal' WHERE id=1", [])
            .unwrap();
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "NEW literal".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        lib.db.execute("DELETE FROM tracks WHERE id=1", []).unwrap();
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "NEW literal".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            0
        );
    }
    #[test]
    fn filters_keep_full_tag_limit_and_combine_with_playlist_bpm_and_search() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        lib.db.execute_batch("INSERT INTO tag_categories VALUES(1,'Test','test',0); WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<512) INSERT INTO my_tags SELECT x,1,'Tag '||x,'tag '||x,x FROM n;").unwrap();
        lib.db
            .execute(
                "INSERT INTO track_my_tags SELECT ?1,id FROM my_tags",
                [saved.id],
            )
            .unwrap();
        lib.db
            .execute(
                "UPDATE track_info SET rating=4,color='#FF0000' WHERE track_id=?1",
                [saved.id],
            )
            .unwrap();
        let playlist = lib.create_playlist("Test").unwrap();
        lib.add_to_playlist(playlist.id, saved.id).unwrap();
        for mode in ["and", "or"] {
            let mut q = BrowserQuery {
                playlist: Some(playlist.id),
                bpm_min: Some(saved.bpm),
                bpm_max: Some(saved.bpm),
                search: saved.title.clone(),
                filters: LibraryFilters {
                    my_tags: (1..=512).collect(),
                    tag_mode: mode.into(),
                    rating_min: Some(4),
                    rating_max: Some(4),
                    colors: vec!["#ff0000".into()],
                },
                ..Default::default()
            };
            assert_eq!(lib.browser_page(&q).unwrap()["total"], 1);
            q.filters.rating_max = Some(3);
            assert!(lib.browser_page(&q).is_err());
            q.filters.rating_min = Some(3);
            assert_eq!(lib.browser_page(&q).unwrap()["total"], 0);
        }
    }
    #[test]
    fn ordered_pages_detect_external_database_edits() {
        let path = std::env::temp_dir().join(format!("pd-ordered-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let lib = Library::open(&path).unwrap();
        lib.db.execute_batch("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2001) INSERT INTO tracks(path,title,duration_seconds,bpm,sample_rate,waveform,added_at) SELECT '/music/'||x,printf('Track %06d',x),180,120,44100,'[]','1' FROM n").unwrap();
        let q = BrowserQuery {
            sort: "title".into(),
            limit: 2,
            ..Default::default()
        };
        assert_eq!(lib.browser_page(&q).unwrap()["tracks"][0]["id"], 1);
        let other = Connection::open(&path).unwrap();
        other
            .execute(
                "UPDATE tracks SET title='A changed track' WHERE id=2001",
                [],
            )
            .unwrap();
        assert_eq!(lib.browser_page(&q).unwrap()["tracks"][0]["id"], 2001);
        drop(other);
        drop(lib);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn categorized_tags_filters_and_metadata_edits_preserve_saved_music_data() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut seed = track();
        seed.beatgrid_offset_seconds = 0.;
        let a = lib.upsert(&seed).unwrap();
        let mut second = track();
        second.path = "/tmp/other.wav".into();
        let b = lib.upsert(&second).unwrap();
        lib.set_tags(a.id, vec!["keep this label".into()]).unwrap();
        let original = lib.track_by_path(&a.path).unwrap().unwrap();
        let c = lib
            .edit_taxonomy("category", None, None, "Energy", None, false)
            .unwrap()[0]
            .id;
        let tags = lib
            .edit_taxonomy("tag", None, Some(c), "Warmup", None, false)
            .unwrap();
        let warm = tags[0].tags[0].id;
        let tags = lib
            .edit_taxonomy("tag", None, Some(c), "Peak", None, false)
            .unwrap();
        let peak = tags[0].tags[1].id;
        let patch = |id, ids, rating, color, fields| {
            let d = lib.track_details(id).unwrap();
            DetailsPatch {
                revision: d.revision,
                fields,
                rating,
                color,
                my_tags: ids,
            }
        };
        lib.edit_details(
            a.id,
            patch(
                a.id,
                vec![warm, peak],
                4,
                "#FF0000".into(),
                std::collections::BTreeMap::from([
                    ("title".into(), "My title".into()),
                    ("Genre".into(), "Afro House".into()),
                    ("DJ_ENERGY".into(), "Sunset".into()),
                ]),
            ),
        )
        .unwrap();
        lib.edit_details(
            b.id,
            patch(b.id, vec![peak], 2, "#0000FF".into(), Default::default()),
        )
        .unwrap();
        let filters = |mode: &str| LibraryFilters {
            my_tags: vec![warm, peak],
            tag_mode: mode.into(),
            ..Default::default()
        };
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                filters: filters("and"),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                filters: filters("or"),
                ..Default::default()
            })
            .unwrap()["total"],
            2
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                filters: LibraryFilters {
                    rating_min: Some(3),
                    colors: vec!["#FF0000".into()],
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "warmup".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "Sunset".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        let old = lib.track_details(a.id).unwrap();
        assert!(lib
            .edit_details(
                a.id,
                DetailsPatch {
                    revision: old.revision - 1,
                    fields: Default::default(),
                    rating: 0,
                    color: String::new(),
                    my_tags: vec![]
                }
            )
            .is_err());
        assert!(lib
            .edit_details(
                a.id,
                DetailsPatch {
                    revision: old.revision,
                    fields: Default::default(),
                    rating: 1,
                    color: String::new(),
                    my_tags: vec![999999]
                }
            )
            .is_err());
        assert_eq!(lib.track_details(a.id).unwrap().revision, old.revision);
        let reimported = lib.upsert(&seed).unwrap();
        assert_eq!(reimported.title, "My title");
        assert_eq!(reimported.tags, original.tags);
        assert_eq!(reimported.waveform, original.waveform);
        assert_eq!(reimported.hot_cues, original.hot_cues);
        assert_eq!(
            serde_json::to_value(reimported.analysis).unwrap(),
            serde_json::to_value(original.analysis).unwrap()
        );
        lib.edit_taxonomy("tag", Some(warm), None, "Chill", None, false)
            .unwrap();
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "warmup".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            0
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "chill".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        lib.edit_taxonomy("category", Some(c), None, "", None, true)
            .unwrap();
        assert!(lib.track_details(a.id).unwrap().my_tags.is_empty());
        assert_eq!(
            lib.track_by_path(&a.path).unwrap().unwrap().tags,
            original.tags
        );
    }
    #[test]
    fn xml_metadata_roundtrip_merges_tags_and_retains_colliding_cues() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let a = lib.upsert(&track()).unwrap();
        lib.set_tags(a.id, vec!["old label".into()]).unwrap();
        let (revision, mut saved_cues) = lib.source_cues(&a.path).unwrap();
        let original_id = saved_cues.insert_manual(1.5, "Existing cue".into());
        saved_cues.bindings.hot[0] = Some(original_id);
        saved_cues.project();
        lib.save_source_cues(&a.path, &saved_cues, Some(revision))
            .unwrap();
        let attrs = std::collections::BTreeMap::from([
            ("Genre".into(), "House & café".into()),
            ("Comments".into(), "First line\nSecond line".into()),
            ("Album".into(), "Album".into()),
            ("Label".into(), "Label".into()),
            ("Grouping".into(), "Grouping".into()),
            ("Composer".into(), "Composer".into()),
            ("Rating".into(), "204".into()),
            ("Colour".into(), "0x25FDE9".into()),
            ("PlayCount".into(), "17".into()),
            ("VendorField".into(), "preserve me".into()),
            (
                "PulseDeckTags".into(),
                r#"{"labels":["new label"],"categories":[{"name":"Energy","tags":["Peak"]}]}"#
                    .into(),
            ),
        ]);
        lib.import_xml_details(a.id, &attrs).unwrap();
        let before = lib.source_cues(&a.path).unwrap().1;
        let entry = XmlTrack {
            marks: vec![XmlMark {
                name: "Imported conflict".into(),
                number: 0,
                start: 3.,
                kind: 0,
            }],
            ..Default::default()
        };
        lib.import_xml_cues(&a, &entry).unwrap();
        lib.import_xml_cues(&a, &entry).unwrap();
        let after = lib.source_cues(&a.path).unwrap().1;
        assert!(before.points.iter().all(|p| after.points.contains(p)));
        assert_eq!(after.hot_cues, before.hot_cues);
        assert_eq!(
            after
                .points
                .iter()
                .filter(|p| p.name == "Imported conflict")
                .count(),
            1
        );
        lib.edit_taxonomy("category", None, None, "Unused category", None, false)
            .unwrap();
        let d = lib.track_details(a.id).unwrap();
        lib.edit_details(
            a.id,
            DetailsPatch {
                revision: d.revision,
                fields: std::collections::BTreeMap::from([(
                    "DJ_SLOT".into(),
                    "Warmup\0Closing".into(),
                )]),
                rating: d.rating,
                color: d.color,
                my_tags: d.my_tags,
            },
        )
        .unwrap();
        let mut legacy =
            serde_json::from_str::<serde_json::Value>(&attrs["PulseDeckTags"]).unwrap();
        legacy["fields"] = serde_json::json!({"":"Legacy\u{1}text"});
        lib.import_xml_details(
            a.id,
            &std::collections::BTreeMap::from([("PulseDeckTags".into(), legacy.to_string())]),
        )
        .unwrap();
        let xml = lib.export_xml().unwrap();
        let path =
            std::env::temp_dir().join(format!("pd-tag-roundtrip-{}.xml", std::process::id()));
        std::fs::write(&path, xml).unwrap();
        let parsed = parse_rekordbox_xml(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let imported = &parsed.collection.tracks[0];
        for key in [
            "Genre",
            "Comments",
            "Album",
            "Label",
            "Grouping",
            "Composer",
            "PlayCount",
            "VendorField",
        ] {
            assert_eq!(imported.attributes.get(key), attrs.get(key), "{key}");
        }
        assert_eq!(imported.attributes["Rating"], "204");
        assert_eq!(imported.attributes["Colour"], "#25FDE9");
        assert!(imported.marks.iter().any(|m| m.name == "Imported conflict"));
        let fresh = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let b = fresh.upsert(&track()).unwrap();
        fresh
            .import_xml_details(b.id, &imported.attributes)
            .unwrap();
        fresh
            .import_xml_details(b.id, &imported.attributes)
            .unwrap();
        let d = fresh.track_details(b.id).unwrap();
        assert_eq!(d.rating, 4);
        assert_eq!(d.color, "#25FDE9");
        assert_eq!(d.fields["Comment"], "First line\nSecond line");
        assert_eq!(d.my_tags.len(), 1);
        assert_eq!(d.fields["DJ_SLOT"], "Warmup\0Closing");
        assert_eq!(d.fields[""], "Legacy\u{1}text");
        assert_eq!(
            fresh.track_by_path(&b.path).unwrap().unwrap().tags,
            ["old label", "new label"]
        );
        let analysis = lib.track_by_path(&a.path).unwrap().unwrap().analysis;
        assert_eq!(analysis.analysis_version, a.analysis.analysis_version);
    }
    #[test]
    fn song_tags_survive_reimport_and_restart_without_changing_track_metadata() {
        let path = std::env::temp_dir().join(format!("pd-tags-{}.sqlite", std::process::id()));
        let lib = Library::open(&path).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        assert!(saved.tags.is_empty());
        let tagged = lib
            .set_tags(
                saved.id,
                vec![
                    " Afro house ".into(),
                    "chill vibe".into(),
                    "AFRO HOUSE".into(),
                    " ".into(),
                    "café".into(),
                ],
            )
            .unwrap();
        assert_eq!(tagged.tags, ["Afro house", "chill vibe", "café"]);
        assert_eq!(tagged.bpm, saved.bpm);
        assert_eq!(tagged.hot_cues, saved.hot_cues);
        assert!(lib.set_tags(saved.id, vec!["x".repeat(129)]).is_err());
        assert!(lib.set_tags(-1, vec![]).is_err());
        assert_eq!(lib.upsert(&track()).unwrap().tags, tagged.tags);
        drop(lib);
        let lib = Library::open(&path).unwrap();
        assert_eq!(
            lib.track_by_path(&saved.path).unwrap().unwrap().tags,
            tagged.tags
        );
        assert!(lib.set_tags(saved.id, vec![]).unwrap().tags.is_empty());
        drop(lib);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn source_cues_are_durable_for_local_and_spotify_and_reject_stale_edits() {
        let root = std::env::temp_dir().join(format!("pd-cues-{}.sqlite", std::process::id()));
        let lib = Library::open(&root).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        let mut cues = SourceCues::default();
        cues.hot_cues[7] = Some(40.0);
        cues.memory_cues[0] = Some(2.0);
        cues.cue_seconds = 1.0;
        cues.annotations.insert("hot:7".into(), "Outro".into());
        let revision = lib.save_source_cues(&saved.path, &cues, Some(0)).unwrap();
        assert_eq!(revision, 1);
        assert_eq!(
            lib.save_source_cues(&saved.path, &cues, Some(1)).unwrap(),
            1
        );
        assert!(lib.save_source_cues(&saved.path, &cues, Some(0)).is_err());
        lib.save_source_cues("spotify:track:0000000000000000000000", &cues, Some(0))
            .unwrap();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        cues.migrate();
        assert_eq!(lib.source_cues(&saved.path).unwrap(), (1, cues.clone()));
        assert_eq!(
            lib.source_cues("spotify:track:0000000000000000000000")
                .unwrap()
                .1,
            cues
        );
        assert_eq!(
            lib.track_by_path(&saved.path).unwrap().unwrap().hot_cues[7],
            Some(40.0)
        );
        drop(lib);
        std::fs::remove_file(root).unwrap();
    }
    #[test]
    fn playlist_reordering_is_atomic_and_invalid_replacement_preserves_membership() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let a = lib.upsert(&track()).unwrap();
        let mut b = track();
        b.path = "/tmp/another.wav".into();
        let b = lib.upsert(&b).unwrap();
        let p = lib.create_playlist("Set").unwrap();
        lib.add_to_playlist(p.id, a.id).unwrap();
        lib.add_to_playlist(p.id, b.id).unwrap();
        lib.playlist_edit(p.id, "replace", None, &[b.id, a.id])
            .unwrap();
        assert_eq!(lib.playlists().unwrap()[0].track_ids, vec![b.id, a.id]);
        assert!(lib
            .playlist_edit(p.id, "replace", None, &[a.id, 999999])
            .is_err());
        assert_eq!(lib.playlists().unwrap()[0].track_ids, vec![b.id, a.id]);
        assert!(lib
            .playlist_edit(p.id, "replace", None, &[a.id, a.id])
            .is_err());
        lib.playlist_edit(p.id, "rename", Some("Work"), &[])
            .unwrap();
        assert_eq!(lib.playlists().unwrap()[0].name, "Work");
        lib.playlist_edit(p.id, "remove", None, &[a.id]).unwrap();
        assert_eq!(lib.playlists().unwrap()[0].track_ids, vec![b.id]);
    }
    fn track() -> TrackRecord {
        TrackRecord {
            id: 1,
            path: "/tmp/Music & beats/song.wav".into(),
            title: "A & B <mix>".into(),
            artist: "DJ".into(),
            tags: Vec::new(),
            file_metadata: Vec::new(),
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
    fn embedded_metadata_backfill_is_searchable_and_preserves_analysis_and_edits() {
        use crate::metadata_index::{Candidate, Update};
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let saved = lib.upsert(&track()).unwrap();
        lib.set_tags(saved.id, vec!["personal label".into()])
            .unwrap();
        let before = lib.track_by_path(&saved.path).unwrap().unwrap();
        let fields = vec![
            deck_engine::EmbeddedTag {
                format: "Id3v2".into(),
                key: "TCON".into(),
                standard_key: Some("Genre".into()),
                value: "Afro House".into(),
                description: "".into(),
                language: "".into(),
            },
            deck_engine::EmbeddedTag {
                format: "Id3v2".into(),
                key: "COMM".into(),
                standard_key: Some("Comment".into()),
                value: "Sunset café".into(),
                description: "Mixing notes".into(),
                language: "eng".into(),
            },
            deck_engine::EmbeddedTag {
                format: "Id3v2".into(),
                key: "DJ_ENERGY".into(),
                standard_key: None,
                value: "Peak 9".into(),
                description: "".into(),
                language: "".into(),
            },
        ];
        let update = |previous: &str, stamp: &str, fields| Update {
            candidate: Candidate {
                id: saved.id,
                path: saved.path.clone(),
                stamp: previous.into(),
            },
            stamp: stamp.into(),
            fields,
        };
        assert_eq!(
            lib.store_file_metadata(&[update("", "1:original", Ok(fields.clone()))])
                .unwrap(),
            1
        );
        let mut after = lib.track_by_path(&saved.path).unwrap().unwrap();
        assert_eq!(after.file_metadata, fields);
        after.file_metadata.clear();
        assert_eq!(
            serde_json::to_value(after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        for query in [
            "Afro House",
            "SUNSET CAFÉ",
            "Mixing notes",
            "DJ_ENERGY",
            "Peak 9",
            "9",
            "personal label",
        ] {
            let page = lib
                .browser_page(&BrowserQuery {
                    search: query.into(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(page["total"], 1, "{query}");
            assert!(page["tracks"][0].get("file_metadata").is_none());
        }
        // A failed read retains the last searchable metadata, and a stale
        // background read cannot overwrite a newer import's metadata.
        assert_eq!(
            lib.store_file_metadata(&[update(
                "1:original",
                "1:unavailable",
                Err("Drive offline".into())
            )])
            .unwrap(),
            1
        );
        assert_eq!(
            lib.store_file_metadata(&[update("", "1:stale", Ok(vec![]))])
                .unwrap(),
            0
        );
        assert_eq!(
            lib.track_by_path(&saved.path)
                .unwrap()
                .unwrap()
                .file_metadata,
            fields
        );
        assert_eq!(
            lib.store_file_metadata(&[update("1:unavailable", "1:removed", Ok(vec![]))])
                .unwrap(),
            1
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "Afro House".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            0
        );
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                search: "personal label".into(),
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        lib.delete_track(saved.id).unwrap();
        assert_eq!(
            lib.db
                .query_row("SELECT count(*) FROM browser_search", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn browser_covering_index_migration_preserves_tracks_and_ranked_totals() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        lib.upsert(&track()).unwrap();
        let before = serde_json::to_value(lib.tracks().unwrap()).unwrap();
        lib.db.execute_batch("DROP INDEX tracks_browser_title; DROP INDEX tracks_browser_bpm; CREATE INDEX tracks_browser_title ON tracks(lower(title),id); CREATE INDEX tracks_browser_bpm ON tracks(bpm,id);").unwrap();
        let lib = Library::from_connection(lib.db).unwrap();
        assert_eq!(before, serde_json::to_value(lib.tracks().unwrap()).unwrap());
        lib.db.execute_batch("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1200) INSERT INTO tracks(path,title,duration_seconds,bpm,sample_rate,waveform,added_at) SELECT '/probe/'||x,printf('Probe %04d',x),x,120+x%5,44100,'[]','0' FROM n;").unwrap();
        for sort in ["title", "bpm", "duration"] {
            let mut ids = std::collections::HashSet::new();
            for offset in [0, 600] {
                let page = lib
                    .browser_page(&BrowserQuery {
                        search: "Probe".into(),
                        sort: sort.into(),
                        offset,
                        limit: 600,
                        ..Default::default()
                    })
                    .unwrap();
                assert_eq!(page["total"], 1200);
                assert_eq!(page["duration_seconds"], 720600.0);
                for row in page["tracks"].as_array().unwrap() {
                    assert!(ids.insert(row["id"].as_i64().unwrap()));
                }
            }
            assert_eq!(ids.len(), 1200);
        }
        lib.db
            .execute(
                "UPDATE tracks SET duration_seconds=9999 WHERE path='/probe/1'",
                [],
            )
            .unwrap();
        let page = lib
            .browser_page(&BrowserQuery {
                search: "Probe".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page["duration_seconds"], 730598.0);
    }

    #[test]
    fn browser_pages_filter_and_sort_in_sql_without_reading_large_analysis_or_cues() {
        let path = std::env::temp_dir().join(format!(
            "pd-browser-performance-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let lib = Library::open(&path).unwrap();
        lib.db.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO tracks(path,title,artist,duration_seconds,bpm,sample_rate,waveform,added_at,tags,analysis_json,metadata_search) SELECT '/music/'||x,printf('Track %06d',x),'Artist',180,x%100+80,44100,'not a waveform array',x,'[\"house\"]','{\"analysis_version\":11}','genre afro house comment sunset terrace DJ_ENERGY peak '||x FROM n;").unwrap();
        let heavy = serde_json::json!({"analysis_version":11,"bpm_source":"model","waveform_bands":{"low":vec![0.123456_f32;4096],"mid":vec![0.123456_f32;4096],"high":vec![0.123456_f32;4096]}}).to_string();
        lib.db
            .execute("UPDATE tracks SET analysis_json=?1 WHERE id<=200", [heavy])
            .unwrap();
        let cold_start = std::time::Instant::now();
        let compact = lib
            .browser_page(&super::BrowserQuery {
                limit: 200,
                sort: "title".into(),
                ..Default::default()
            })
            .unwrap();
        let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.;
        eprintln!("100k unfiltered first query {cold_ms:.2}ms");
        assert!(
            cold_ms < 100.,
            "Cold unfiltered query exceeded 100ms: {cold_ms}"
        );
        assert_eq!(compact["tracks"][0]["bpm_source"], "model");
        let started = std::time::Instant::now();
        let page = lib
            .browser_page(&super::BrowserQuery {
                limit: 200,
                offset: 50000,
                sort: "title".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page["total"], 100000);
        assert_eq!(page["tracks"].as_array().unwrap().len(), 200);
        assert_eq!(page["tracks"][0]["title"], "Track 050001");
        assert!(page["tracks"][0].get("waveform").is_none());
        assert!(page["tracks"][0].get("neural_rhythm").is_none());
        assert!(started.elapsed().as_secs_f64() < 2.0);
        let mut timings = Vec::new();
        for index in 0..100 {
            let start = std::time::Instant::now();
            lib.browser_page(&super::BrowserQuery {
                limit: 200,
                offset: index * 990,
                sort: "title".into(),
                ..Default::default()
            })
            .unwrap();
            timings.push(start.elapsed().as_secs_f64() * 1000.);
        }
        timings.sort_by(f64::total_cmp);
        eprintln!(
            "100k browser pages n=100 p90={:.2}ms p95={:.2}ms p99={:.2}ms",
            timings[89], timings[94], timings[98]
        );
        assert!(timings[89] < 25. && timings[94] < 50. && timings[98] < 100.);

        let filtered = lib
            .browser_page(&super::BrowserQuery {
                search: "Track 099999".into(),
                limit: 200,
                ..Default::default()
            })
            .unwrap();
        let mut searches = Vec::new();
        for index in 0..100 {
            let now = std::time::Instant::now();
            let result = lib
                .browser_page(&super::BrowserQuery {
                    search: format!("Track {:06}", 90000 + index),
                    limit: 200,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(result["total"], 1);
            searches.push(now.elapsed().as_secs_f64() * 1000.);
        }
        searches.sort_by(f64::total_cmp);
        eprintln!(
            "100k literal searches n=100 p90={:.2}ms p95={:.2}ms p99={:.2}ms",
            searches[89], searches[94], searches[98]
        );
        assert!(searches[89] < 25. && searches[94] < 50. && searches[98] < 100.);
        let mut metadata_searches = Vec::new();
        for index in 0..100 {
            let now = std::time::Instant::now();
            let result = lib
                .browser_page(&BrowserQuery {
                    search: format!("peak {}", 90000 + index),
                    limit: 200,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(result["total"], 1);
            metadata_searches.push(now.elapsed().as_secs_f64() * 1000.);
        }
        metadata_searches.sort_by(f64::total_cmp);
        eprintln!(
            "100k embedded metadata searches n=100 p90={:.2}ms p95={:.2}ms p99={:.2}ms",
            metadata_searches[89], metadata_searches[94], metadata_searches[98]
        );
        assert!(
            metadata_searches[89] < 25.
                && metadata_searches[94] < 50.
                && metadata_searches[98] < 100.
        );
        let mut genre_pages = Vec::new();
        for index in 0..100 {
            let now = std::time::Instant::now();
            let broad = lib
                .browser_page(&BrowserQuery {
                    search: "Afro House".into(),
                    offset: index * 990,
                    limit: 200,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(broad["total"], 100000);
            genre_pages.push(now.elapsed().as_secs_f64() * 1000.);
        }
        eprintln!("100k broad genre first search {:.2}ms", genre_pages[0]);
        assert!(
            genre_pages[0] < 100.,
            "Cold genre query exceeded 100ms: {}",
            genre_pages[0]
        );
        genre_pages.sort_by(f64::total_cmp);
        eprintln!(
            "100k broad genre pages n=100 p90={:.2}ms p95={:.2}ms p99={:.2}ms",
            genre_pages[89], genre_pages[94], genre_pages[98]
        );
        assert!(genre_pages[89] < 25. && genre_pages[94] < 50. && genre_pages[98] < 100.);
        lib.db.execute_batch("INSERT INTO track_info(track_id,rating,color) SELECT id,4,'#FF0000' FROM tracks; INSERT INTO tag_categories VALUES(1,'Energy','energy',0);INSERT INTO my_tags VALUES(1,1,'Peak','peak',0),(2,1,'Groove','groove',1);INSERT INTO track_my_tags SELECT id,1 FROM tracks;INSERT INTO track_my_tags SELECT id,2 FROM tracks;").unwrap();
        let mut facet_timings = Vec::new();
        for index in 0..100 {
            let start = std::time::Instant::now();
            let page = lib
                .browser_page(&BrowserQuery {
                    filters: LibraryFilters {
                        my_tags: vec![1, 2],
                        tag_mode: "and".into(),
                        rating_min: Some(4),
                        colors: vec!["#FF0000".into()],
                        ..Default::default()
                    },
                    offset: index * 990,
                    limit: 200,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(page["total"], 100000);
            facet_timings.push(start.elapsed().as_secs_f64() * 1000.);
        }
        eprintln!(
            "100k AND/rating/color first query {:.2}ms",
            facet_timings[0]
        );
        assert!(
            facet_timings[0] < 100.,
            "Cold facet query exceeded 100ms: {}",
            facet_timings[0]
        );
        facet_timings.sort_by(f64::total_cmp);
        eprintln!(
            "100k AND/rating/color pages p90={:.2}ms p95={:.2}ms p99={:.2}ms",
            facet_timings[89], facet_timings[94], facet_timings[98]
        );
        assert!(facet_timings[89] < 25. && facet_timings[94] < 50. && facet_timings[98] < 100.);
        let mut cold_after_edits = Vec::new();
        for index in 0..24 {
            let d = lib.track_details(1).unwrap();
            lib.edit_details(
                1,
                DetailsPatch {
                    revision: d.revision,
                    fields: [("Comment".into(), format!("Edit {index}"))].into(),
                    rating: 4,
                    color: "#FF0000".into(),
                    my_tags: vec![1, 2],
                },
            )
            .unwrap();
            let start = std::time::Instant::now();
            let page = lib
                .browser_page(&BrowserQuery {
                    search: "Afro House".into(),
                    sort: ["title", "bpm", "duration"][index % 3].into(),
                    filters: LibraryFilters {
                        my_tags: vec![1, 2],
                        tag_mode: if index % 2 == 0 { "and" } else { "or" }.into(),
                        rating_min: Some(4),
                        rating_max: Some(5),
                        colors: vec!["#FF0000".into()],
                    },
                    offset: 90000,
                    limit: 200,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(page["total"], 99999);
            cold_after_edits.push(start.elapsed().as_secs_f64() * 1000.);
        }
        cold_after_edits.sort_by(f64::total_cmp);
        eprintln!(
            "100k cold combined queries after edits n=24 p90={:.2}ms p95={:.2}ms p99/max={:.2}ms",
            cold_after_edits[21], cold_after_edits[22], cold_after_edits[23]
        );
        assert!(
            cold_after_edits[23] < 100.,
            "Cold combined query exceeded 100ms: {}",
            cold_after_edits[23]
        );
        let bpm_page = lib
            .browser_page(&BrowserQuery {
                sort: "bpm".into(),
                offset: 50000,
                limit: 2,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(bpm_page["tracks"][0]["id"], 50);
        let duration_page = lib
            .browser_page(&BrowserQuery {
                sort: "duration".into(),
                limit: 2,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(duration_page["tracks"][0]["id"], 1);
        lib.browser_page(&BrowserQuery {
            sort: "title".into(),
            ..Default::default()
        })
        .unwrap();
        lib.db
            .execute("UPDATE tracks SET title='A changed title' WHERE id=42", [])
            .unwrap();
        assert_eq!(
            lib.browser_page(&BrowserQuery {
                sort: "title".into(),
                ..Default::default()
            })
            .unwrap()["tracks"][0]["id"],
            42
        );
        assert_eq!(filtered["total"], 1);
        assert!(filtered["next_offset"].is_null());
        let playlist = lib.create_playlist("Subset").unwrap();
        lib.add_to_playlist(playlist.id, 42).unwrap();
        let filtered = lib
            .browser_page(&super::BrowserQuery {
                playlist: Some(playlist.id),
                limit: 200,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["tracks"][0]["id"], 42);
        assert_eq!(lib.path_for_id(42).unwrap(), "/music/42");
        drop(lib);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn metadata_only_reimport_preserves_analysis_and_user_knowledge() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut original = track();
        original.analysis.source_fingerprint = Some("content".into());
        original.analysis.analysis_version = audio_analysis::ANALYSIS_VERSION;
        let saved = lib.upsert(&original).unwrap();
        lib.set_tags(saved.id, vec!["warmup".into()]).unwrap();
        lib.metadata(&saved.path, None, Some(original.hot_cues.clone()), None)
            .unwrap();
        let playlist = lib.create_playlist("Set").unwrap();
        lib.add_to_playlist(playlist.id, saved.id).unwrap();
        let mut metadata = track();
        metadata.title = "filename fallback".into();
        metadata.bpm = 0.;
        metadata.key = None;
        metadata.waveform.clear();
        let reimported = lib.import_metadata(&metadata).unwrap();
        assert_eq!(reimported.id, saved.id);
        assert_eq!(reimported.title, original.title);
        assert_eq!(reimported.waveform, original.waveform);
        assert_eq!(
            reimported.analysis.source_fingerprint,
            original.analysis.source_fingerprint
        );
        assert_eq!(reimported.tags, ["warmup"]);
        assert_eq!(reimported.hot_cues, original.hot_cues);
        assert_eq!(lib.playlists().unwrap()[0].track_ids, [saved.id]);
        metadata.path = "/tmp/new-metadata-only.wav".into();
        metadata.analysis = AnalysisMetadata::default();
        let new = lib.import_metadata(&metadata).unwrap();
        assert!(new.waveform.is_empty());
        assert_eq!(new.analysis.analysis_version, 0);
        assert_eq!(lib.tracks().unwrap().len(), 2);
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
        done.analysis.source_fingerprint = Some("verified fixture".into());
        let mut measurement = audio_analysis::measure_audio(&[], done.sample_rate);
        measurement.frames = (done.duration_seconds * done.sample_rate as f64).round() as u64;
        done.analysis.audio_measurements = Some(measurement);
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
        let mut queued = lib.paths_needing_analysis().unwrap();
        queued.sort();
        assert_eq!(queued, ["/tmp/missing.wav", "/tmp/old.wav"]);
        assert_eq!(
            saved.analysis.audio_measurements.as_ref().unwrap().frames,
            done.analysis.audio_measurements.as_ref().unwrap().frames
        );
        done.analysis.audio_measurements = None;
        lib.upsert(&done).unwrap();
        assert!(lib.paths_needing_analysis().unwrap().contains(&done.path));
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
    fn spotify_and_local_analysis_keep_exact_timing_across_process_restart() {
        const CACHE_PATH: &str = "PULSEDECK_TEST_LIBRARY_RESTART";
        const URI: &str = "spotify:track:0000000000000000000000";
        const LOCAL: &str = "/fixture/restart.wav";
        const BPM: f64 = 120.00100300902709;
        const BEAT: f64 = 0.04245912656188011;
        if let Some(path) = std::env::var_os(CACHE_PATH) {
            let library = Library::open(Path::new(&path)).unwrap();
            let spotify = library.spotify_analysis(URI).unwrap().unwrap();
            let local = crate::record_analysis(library.track_by_path(LOCAL).unwrap().unwrap());
            for analysis in [spotify, local] {
                assert_eq!(analysis.bpm.unwrap().to_bits(), BPM.to_bits());
                assert_eq!(
                    analysis.beatgrid_offset_seconds.unwrap().to_bits(),
                    BEAT.to_bits()
                );
                assert_eq!(
                    analysis.neural_rhythm.unwrap().beats_seconds[0].to_bits(),
                    BEAT.to_bits()
                );
                assert_eq!(
                    analysis.analysis_stages.neural.unwrap().dependency,
                    "durable-source-and-model"
                );
                assert_eq!(analysis.manual_grid_anchor_index, Some(0));
            }
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "pulsedeck-library-restart-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("library.sqlite3");
        let mut analysis = deck_engine::analyze_decoded_audio(
            &vec![[0.0; 2]; 4000],
            8000,
            &deck_engine::AudioSourceInfo {
                id: URI.into(),
                location: URI.into(),
                title: "Restart fixture".into(),
                artist: None,
                extension: None,
                release_identity: Some(URI.into()),
            },
            false,
        )
        .unwrap();
        analysis.bpm = Some(BPM);
        analysis.bpm_source = Some("manual".into());
        analysis.beatgrid_offset_seconds = Some(BEAT);
        analysis.beatgrid_source = Some("manual".into());
        analysis.manual_grid_anchor_index = Some(0);
        analysis.neural_rhythm = Some(audio_analysis::NeuralRhythm {
            beats_seconds: vec![BEAT],
            downbeats_seconds: vec![BEAT],
            bpm: BPM,
            beat_confidence: 0.9,
            downbeat_confidence: 0.9,
            model: "fixture".into(),
            analysis_seconds: 0.,
            bar_evidence: vec![],
            beats_per_bar: Some(4),
        });
        analysis.analysis_stages.neural = Some(audio_analysis::StageStamp::ready(
            1,
            "durable-source-and-model".into(),
        ));
        {
            let library = Library::open(&path).unwrap();
            library.cache_spotify_analysis(&analysis).unwrap();
            analysis.id = LOCAL.into();
            analysis.path = LOCAL.into();
            analysis.recording_allowed = true;
            library.upsert(&crate::analysis_record(analysis)).unwrap();
        }
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "library::tests::spotify_and_local_analysis_keep_exact_timing_across_process_restart", "--nocapture"])
            .env(CACHE_PATH, &path).output().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        assert!(
            result.status.success(),
            "durable cache restart failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
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
                release_identity: None,
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
            let before =
                serde_json::to_value(library.spotify_analysis(uri).unwrap().unwrap()).unwrap();
            let mut cues = SourceCues {
                version: 2,
                ..Default::default()
            };
            cues.points.push(audio_analysis::CuePoint::manual(
                "user:1".into(),
                0.1,
                "Entry".into(),
            ));
            library.save_source_cues(uri, &cues, Some(0)).unwrap();
            assert_eq!(
                library
                    .spotify_analysis(uri)
                    .unwrap()
                    .unwrap()
                    .cue_points
                    .len(),
                1
            );
            cues.points.clear();
            library.save_source_cues(uri, &cues, Some(1)).unwrap();
            let after =
                serde_json::to_value(library.spotify_analysis(uri).unwrap().unwrap()).unwrap();
            assert_eq!(
                before, after,
                "clearing cues must preserve every measured analysis field"
            );
            assert!(library.tracks().unwrap().is_empty());
        }
        {
            let library = Library::open(&path).unwrap();
            let restored = library.spotify_analysis(uri).unwrap().unwrap();
            assert_eq!(restored.bpm, Some(127.8));
            assert_eq!(restored.camelot.as_deref(), Some("10A"));
            assert_eq!(restored.waveform_bands.unwrap().low.len(), 4096);
            // Older usable cache entries still carry the DJ's manual edits.
            analysis.analysis_version = 2;
            analysis.bpm_source = Some("manual".into());
            analysis.beatgrid_source = Some("manual".into());
            analysis.beatgrid_offset_seconds = Some(0.234);
            library.cache_spotify_analysis(&analysis).unwrap();
            let legacy = library.spotify_analysis(uri).unwrap().unwrap();
            assert_eq!(legacy.bpm_source.as_deref(), Some("manual"));
            assert_eq!(legacy.beatgrid_offset_seconds, Some(0.234));
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
                release_identity: None,
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
                meter: Some(4),
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
    #[test]
    fn legacy_memory_annotation_save_preserves_names_by_identity() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let t = lib.upsert(&track()).unwrap();
        let (_, mut cues) = lib.source_cues(&t.path).unwrap();
        cues.memory_cues[0] = Some(10.);
        cues.memory_cues[1] = Some(20.);
        cues.annotations.insert("memory:0".into(), "Entry".into());
        cues.annotations.insert("memory:1".into(), "Exit".into());
        lib.save_source_cues(&t.path, &cues, Some(0)).unwrap();
        let (_, mut cues) = lib.source_cues(&t.path).unwrap();
        let id = cues.bindings.memory[0].clone().unwrap();
        cues.annotations
            .insert("memory:0".into(), "Named arrival".into());
        lib.save_source_cues(&t.path, &cues, Some(1)).unwrap();
        let (_, mut cues) = lib.source_cues(&t.path).unwrap();
        assert_eq!(
            cues.points.iter().find(|p| p.id == id).unwrap().name,
            "Named arrival"
        );
        cues.memory_cues.swap(0, 1);
        lib.save_source_cues(&t.path, &cues, Some(2)).unwrap();
        let (revision, mut cues) = lib.source_cues(&t.path).unwrap();
        assert_eq!(cues.bindings.memory[0].as_deref(), Some(id.as_str()));
        assert_eq!(cues.annotations["memory:0"], "Named arrival");
        cues.memory_cues = [None; 10];
        cues.memory_cues[1] = Some(10.);
        lib.save_source_cues(&t.path, &cues, Some(revision))
            .unwrap();
        let (_, cues) = lib.source_cues(&t.path).unwrap();
        assert_eq!(cues.bindings.memory[0].as_deref(), Some(id.as_str()));
        assert_eq!(cues.annotations["memory:0"], "Named arrival");
    }
    #[test]
    fn named_cues_and_xml_labels_survive_reopen_and_conflicting_edit() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let t = lib.upsert(&track()).unwrap();
        let (_, mut cues) = lib.source_cues(&t.path).unwrap();
        let mut legacy = cues.hot_cues;
        legacy[0] = Some(12.);
        cues.reconcile_legacy(
            legacy,
            cues.memory_cues,
            cues.cue_seconds,
            &cues.annotations.clone(),
        );
        let id = cues.bindings.hot[0].clone().unwrap();
        let p = cues.points.iter_mut().find(|p| p.id == id).unwrap();
        p.name = "Drop & 回憶 <one>".into();
        p.end_seconds = Some(20.);
        p.role = Some(audio_analysis::CueRole::Drop);
        cues.project();
        assert_eq!(lib.save_source_cues(&t.path, &cues, Some(0)).unwrap(), 1);
        let xml = lib.export_xml().unwrap();
        let parsed: RekordboxXml = quick_xml::de::from_str(&xml).unwrap();
        assert_eq!(
            parsed.collection.tracks[0].marks[0].name,
            "Drop & 回憶 <one>"
        );
        let (_, mut renamed) = lib.source_cues(&t.path).unwrap();
        renamed.points.iter_mut().find(|p| p.id == id).unwrap().name = "My name".into();
        renamed.project();
        lib.save_source_cues(&t.path, &renamed, Some(1)).unwrap();
        assert!(lib.save_source_cues(&t.path, &cues, Some(1)).is_err());
        let (_, saved) = lib.source_cues(&t.path).unwrap();
        assert_eq!(
            saved.points.iter().find(|p| p.id == id).unwrap().name,
            "My name"
        );
        assert_eq!(saved.hot_cues[0], Some(12.));
    }
    #[test]
    fn cached_playlist_membership_is_scoped_and_reports_partial_pages() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let a = "spotify:track:0000000000000000000000".to_string();
        let b = "spotify:track:1111111111111111111111".to_string();
        lib.remember_membership("spotify:liked", 0, 2, std::slice::from_ref(&a))
            .unwrap();
        assert_eq!(
            lib.suggestion_scope("spotify:liked").unwrap(),
            (std::collections::HashSet::from([a.clone()]), false)
        );
        lib.remember_membership("spotify:liked", 1, 2, std::slice::from_ref(&b))
            .unwrap();
        assert!(lib.suggestion_scope("spotify:liked").unwrap().1);
        assert_eq!(
            lib.suggestion_scope("spotify:playlist:missing").unwrap(),
            (std::collections::HashSet::new(), false)
        );
        lib.remember_membership("spotify:liked", 0, 1, std::slice::from_ref(&b))
            .unwrap();
        assert_eq!(
            lib.suggestion_scope("spotify:liked").unwrap(),
            (std::collections::HashSet::from([b]), true)
        );
    }
    fn planner_spotify_track(id: &str, title: &str) -> crate::spotify::SpotifyTrack {
        crate::spotify::SpotifyTrack {
            id: id.into(),
            uri: format!("spotify:track:{id}"),
            title: title.into(),
            artists: vec!["Artist".into()],
            album: String::new(),
            album_id: None,
            duration_ms: 180_000,
            artwork_url: None,
            external_url: String::new(),
            is_local: false,
            available: true,
            playable_in_deck: false,
        }
    }

    #[test]
    fn planner_scope_coverage_keeps_unanalysed_metadata_and_counts_raw_pages() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let a = planner_spotify_track("0000000000000000000000", "First page track");
        lib.remember_spotify_page(
            "spotify:liked",
            &crate::spotify::SpotifyPage {
                items: vec![a],
                offset: 0,
                total: 4,
                next_offset: Some(2),
                fetched_count: 2,
            },
        )
        .unwrap();
        let first = lib
            .planner_coverage(Some("spotify:liked"), None, None)
            .unwrap();
        assert_eq!(first.total_members, Some(4));
        assert_eq!(first.indexed_members, 1);
        assert_eq!(first.analysed_members, 0);
        assert_eq!(first.needs_analysis[0].title, "First page track");
        assert!(!first.complete);
        let b = planner_spotify_track("1111111111111111111111", "Older liked track");
        lib.remember_spotify_page(
            "spotify:liked",
            &crate::spotify::SpotifyPage {
                items: vec![b],
                offset: 2,
                total: 4,
                next_offset: None,
                fetched_count: 2,
            },
        )
        .unwrap();
        let full = lib
            .planner_coverage(Some("spotify:liked"), None, None)
            .unwrap();
        assert!(full.complete); // Unsupported provider entries were scanned, not silently lost pages.
        lib.remember_spotify_page(
            "spotify:liked",
            &crate::spotify::SpotifyPage {
                items: vec![],
                offset: 0,
                total: 4,
                next_offset: Some(2),
                fetched_count: 2,
            },
        )
        .unwrap();
        assert!(
            lib.planner_coverage(Some("spotify:liked"), None, None)
                .unwrap()
                .complete
        );
        assert_eq!(full.indexed_members, 2);
        assert_eq!(full.needs_analysis_members, 2);
        assert!(full
            .needs_analysis
            .iter()
            .any(|m| m.title == "Older liked track"));
        assert_eq!(full.needs_analysis[0].duration_seconds, 180.0);
        assert_eq!(lib.features_pending().unwrap(), 0); // Metadata indexing schedules no analysis.
    }

    #[test]
    fn planner_scope_coverage_rejects_missing_pages_and_resets_refresh() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let make_page = |offset, total, next_offset, fetched_count| crate::spotify::SpotifyPage {
            items: vec![planner_spotify_track("0000000000000000000000", "Track")],
            offset,
            total,
            next_offset,
            fetched_count,
        };
        lib.remember_spotify_page("spotify:liked", &make_page(0, 5, Some(1), 1))
            .unwrap();
        lib.remember_spotify_page("spotify:liked", &make_page(3, 5, None, 2))
            .unwrap();
        assert!(
            !lib.planner_coverage(Some("spotify:liked"), None, None)
                .unwrap()
                .complete
        );
        lib.remember_spotify_page("spotify:liked", &make_page(1, 5, Some(3), 2))
            .unwrap();
        assert!(
            lib.planner_coverage(Some("spotify:liked"), None, None)
                .unwrap()
                .complete
        );
        lib.refresh_spotify_scope("spotify:liked", &make_page(0, 8, Some(1), 1))
            .unwrap();
        assert!(
            !lib.planner_coverage(Some("spotify:liked"), None, None)
                .unwrap()
                .complete
        );
        lib.remember_spotify_page("spotify:playlist:short", &make_page(0, 8, None, 1))
            .unwrap();
        assert!(
            !lib.planner_coverage(Some("spotify:playlist:short"), None, None)
                .unwrap()
                .complete
        );
    }

    #[test]
    fn planner_coverage_does_not_treat_imported_bpm_as_audio_analysis() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let imported = lib.upsert(&track()).unwrap();
        let coverage = lib
            .planner_coverage(None, Some(&[format!("local:{}", imported.id)]), None)
            .unwrap();
        assert_eq!(coverage.analysed_members, 0);
        assert_eq!(coverage.needs_analysis_members, 1);
        assert_eq!(coverage.needs_analysis[0].title, imported.title);
    }

    #[test]
    fn planner_explicit_sources_include_unknowns_and_bound_metadata_cards() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let mut analysed_track = track();
        analysed_track.analysis.analysis_version = audio_analysis::ANALYSIS_VERSION;
        let local = lib.upsert(&analysed_track).unwrap();
        let mut sources = (0..101)
            .map(|i| format!("spotify:track:{i:022}"))
            .collect::<Vec<_>>();
        sources.push(format!("local:{}", local.id));
        let coverage = lib.planner_coverage(None, Some(&sources), None).unwrap();
        assert!(coverage.complete);
        assert_eq!(coverage.indexed_members, 102);
        assert_eq!(coverage.analysed_members, 1);
        assert_eq!(coverage.needs_analysis_members, 101);
        assert_eq!(coverage.needs_analysis.len(), 100);
        assert!(coverage.needs_analysis_truncated);
        let only_local = lib
            .planner_coverage(None, Some(&sources), Some("local"))
            .unwrap();
        assert_eq!(only_local.indexed_members, 1);
        assert_eq!(only_local.total_members, Some(1));
    }

    #[test]
    fn canonical_local_source_id_and_path_restrict_the_same_cached_pool() {
        let lib = Library::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let a = lib.upsert(&track()).unwrap();
        let mut other = track();
        other.path = "/tmp/outside-scope.wav".into();
        lib.upsert(&other).unwrap();
        let id = lib
            .normalize_suggestion_sources(&[format!("local:{}", a.id)])
            .unwrap();
        let path = lib
            .normalize_suggestion_sources(std::slice::from_ref(&a.path))
            .unwrap();
        assert_eq!(id, path);
        let by_id = lib
            .with_features(|pool| {
                pool.values()
                    .filter(|f| id.contains(&f.source))
                    .map(|f| f.source.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap();
        let by_path = lib
            .with_features(|pool| {
                pool.values()
                    .filter(|f| path.contains(&f.source))
                    .map(|f| f.source.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(by_id, by_path);
        assert_eq!(by_id.as_slice(), std::slice::from_ref(&a.path));
        assert!(lib
            .normalize_suggestion_sources(&["local:999999".into()])
            .unwrap_err()
            .contains("Unknown explicit local source"));
        assert!(lib
            .normalize_suggestion_sources(&["local:abc".into()])
            .is_err());
        assert!(lib.normalize_suggestion_sources(&[]).unwrap().is_empty());
        // Resolution is independent of the large analysis columns, even if a legacy row is corrupt.
        lib.db
            .execute(
                "UPDATE tracks SET analysis_json='invalid',waveform='invalid' WHERE id=?1",
                [a.id],
            )
            .unwrap();
        assert_eq!(
            lib.normalize_suggestion_sources(&[format!("local:{}", a.id)])
                .unwrap(),
            id
        );
    }
}
