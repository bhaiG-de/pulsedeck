//! One-time removal of retired generated markers from existing libraries.
//! No audio analysis or transport changes occur here.
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;

fn clean_points(value: &mut Value) {
    if value.is_null() {
        *value = json!([]);
    }
    let Some(points) = value.as_array_mut() else {
        return;
    };
    points.retain_mut(|p| {
        if p["origin"] == "analysis" {
            if p["modified"] != true && p["confirmed"] != true {
                return false;
            }
            p["origin"] = json!("manual");
        }
        if let Some(obj) = p.as_object_mut() {
            obj.retain(|k, _| {
                matches!(
                    k.as_str(),
                    "id" | "start_seconds"
                        | "end_seconds"
                        | "name"
                        | "role"
                        | "origin"
                        | "modified"
                        | "confirmed"
                )
            });
        }
        true
    });
}
fn clean_metadata(value: &mut Value) {
    if let Some(points) = value.get_mut("cue_points") {
        clean_points(points);
    }
    if let Some(obj) = value.as_object_mut() {
        obj.remove("sections");
        obj.remove("auto_cues");
    }
    if let Some(stages) = value
        .get_mut("analysis_stages")
        .and_then(Value::as_object_mut)
    {
        stages.remove("spectral");
        stages.remove("sections");
    }
}
fn clean_cues(mut value: Value, legacy: &Value) -> Result<audio_analysis::CueCollection, String> {
    // Before canonical markers existed, ownership was recorded as slot/time pairs.
    if value["version"].as_u64().unwrap_or(0) == 0 {
        for kind in ["hot", "memory"] {
            let key = format!("{kind}_cues");
            let len = if kind == "hot" { 8 } else { 10 };
            for i in 0..len {
                let t = value[&key][i].as_f64();
                if t.is_none() || value["annotations"][format!("{kind}:{i}")].is_string() {
                    continue;
                }
                let generated = legacy[kind].as_array().is_some_and(|xs| {
                    xs.iter().any(|x| {
                        let old = if kind == "hot" {
                            if x[0].as_u64() != Some(i as u64) {
                                return false;
                            }
                            x[1].as_f64()
                        } else {
                            x.as_f64()
                        };
                        old.zip(t).is_some_and(|(a, b)| (a - b).abs() < 0.002)
                    })
                });
                if generated {
                    value[&key][i] = Value::Null;
                }
            }
        }
    }
    if let Some(points) = value.get_mut("points") {
        clean_points(points);
    }
    let ids: Vec<String> = value["points"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["id"].as_str().map(str::to_owned))
        .collect();
    if value["version"].as_u64().unwrap_or(0) > 0 {
        if let Some(bindings) = value["bindings"].as_object_mut() {
            for (kind, binding) in bindings {
                let keep = |v: &Value| v.as_str().is_some_and(|id| ids.iter().any(|x| x == id));
                if kind == "main" {
                    if !keep(binding) {
                        *binding = Value::Null;
                    }
                } else if let Some(slots) = binding.as_array_mut() {
                    for v in slots {
                        if !keep(v) {
                            *v = Value::Null;
                        }
                    }
                }
            }
        }
    }
    if let Some(obj) = value.as_object_mut() {
        obj.retain(|k, _| {
            matches!(
                k.as_str(),
                "version"
                    | "next_id"
                    | "points"
                    | "bindings"
                    | "cue_seconds"
                    | "hot_cues"
                    | "memory_cues"
                    | "annotations"
            )
        });
    }
    let mut cues: audio_analysis::CueCollection =
        serde_json::from_value(value).map_err(|e| e.to_string())?;
    cues.migrate();
    cues.version = 2;
    cues.project();
    Ok(cues)
}

pub fn run(db: &Connection) -> Result<(), String> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS library_migrations(name TEXT PRIMARY KEY)")
        .map_err(|e| e.to_string())?;
    if db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM library_migrations WHERE name='manual-cues-only')",
            [],
            |r| r.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let local: Vec<(String, String, String, String, f64)> = tx
        .prepare("SELECT path,analysis_json,hot_cues,memory_cues,cue_seconds FROM tracks")
        .map_err(|e| e.to_string())?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let spotify: Vec<(String, String)> = tx
        .prepare("SELECT uri,metadata FROM spotify_analysis")
        .map_err(|e| e.to_string())?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let stored: Vec<(String, u64, String)> = tx
        .prepare("SELECT source,revision,cues FROM source_cues")
        .map_err(|e| e.to_string())?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut metadata = HashMap::new();
    let mut cues = HashMap::new();
    for (source, raw, hot, memory, main) in &local {
        let m: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        cues.insert(source.clone(),json!({"hot_cues":serde_json::from_str::<Value>(hot).map_err(|e|e.to_string())?,"memory_cues":serde_json::from_str::<Value>(memory).map_err(|e|e.to_string())?,"cue_seconds":main}));
        metadata.insert(source.clone(), m);
    }
    for (source, raw) in &spotify {
        metadata.insert(
            source.clone(),
            serde_json::from_str::<Value>(raw).map_err(|e| e.to_string())?,
        );
    }
    let mut revisions = HashMap::new();
    for (source, revision, raw) in &stored {
        cues.insert(
            source.clone(),
            serde_json::from_str(raw).map_err(|e| e.to_string())?,
        );
        revisions.insert(source.clone(), *revision);
    }
    // Preserve unbound human markers in old metadata even when no source-cue row exists.
    for (source, m) in &metadata {
        if !cues.contains_key(source) && m["cue_points"].as_array().is_some_and(|xs| !xs.is_empty())
        {
            cues.insert(
                source.clone(),
                json!({"version":1,"points":m["cue_points"],"bindings":{}}),
            );
        }
    }
    let mut clean = HashMap::new();
    for (source, c) in cues {
        let legacy = &metadata.get(&source).unwrap_or(&Value::Null)["auto_cues"];
        let mut c = clean_cues(c, legacy)?;
        if let Some(m) = metadata.get(&source) {
            let mut points = m["cue_points"].clone();
            clean_points(&mut points);
            if let Some(points) = points.as_array() {
                for point in points {
                    let point: audio_analysis::CuePoint =
                        serde_json::from_value(point.clone()).map_err(|e| e.to_string())?;
                    if !c.points.iter().any(|p| p.id == point.id) {
                        c.points.push(point);
                    }
                }
            }
            c.project();
        }
        let raw = serde_json::to_string(&c).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO source_cues(source,revision,cues) VALUES(?1,?2,?3) ON CONFLICT(source) DO UPDATE SET revision=excluded.revision,cues=excluded.cues",params![source,revisions.get(&source).copied().unwrap_or(0)+1,raw]).map_err(|e|e.to_string())?;
        clean.insert(source, c);
    }
    for (source, mut m) in metadata {
        clean_metadata(&mut m);
        if let Some(c) = clean.get(&source) {
            m["cue_points"] = json!(c.points);
        }
        let raw = serde_json::to_string(&m).map_err(|e| e.to_string())?;
        if let Some(c) = clean.get(&source) {
            tx.execute("UPDATE tracks SET analysis_json=?2,hot_cues=?3,memory_cues=?4,cue_seconds=?5 WHERE path=?1",params![source,raw,serde_json::to_string(&c.hot_cues).unwrap(),serde_json::to_string(&c.memory_cues).unwrap(),c.cue_seconds]).map_err(|e|e.to_string())?;
        } else {
            tx.execute(
                "UPDATE tracks SET analysis_json=?2 WHERE path=?1",
                params![source, raw],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "UPDATE spotify_analysis SET metadata=?2 WHERE uri=?1",
            params![source, raw],
        )
        .map_err(|e| e.to_string())?;
    }
    if tx
        .query_row(
            "SELECT count(*) FROM pragma_table_info('suggest_features') WHERE name='grid_current'",
            [],
            |r| r.get::<_, u64>(0),
        )
        .map_err(|e| e.to_string())?
        > 0
    {
        tx.execute("ALTER TABLE suggest_features DROP COLUMN grid_current", [])
            .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "INSERT INTO library_migrations(name) VALUES('manual-cues-only')",
        [],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(id: &str, origin: &str, modified: bool, confirmed: bool) -> Value {
        json!({"id":id,"start_seconds":12.0,"end_seconds":null,"name":id,"role":"drop","origin":origin,"modified":modified,"confirmed":confirmed,"analysis_version":2,"energy":0.8,"anchor":{"source_seconds":12.0}})
    }
    #[test]
    fn canonical_cleanup_preserves_edits_imports_and_bindings_without_promoting_guesses() {
        let value = json!({"version":1,"next_id":8,"points":[point("auto","analysis",false,false),point("edited","analysis",true,false),point("approved","analysis",false,true),point("import","imported",false,false),point("manual","manual",true,true)],"bindings":{"hot":["auto","edited","import",null,null,null,null,null],"memory":["approved",null,null,null,null,null,null,null,null,null]},"declined_analysis":["old"],"tombstones":[{"id":"old"}]});
        let c = clean_cues(value, &Value::Null).unwrap();
        assert_eq!(c.points.len(), 4);
        assert_eq!(c.bindings.hot[0], None);
        assert_eq!(c.bindings.hot[1].as_deref(), Some("edited"));
        assert_eq!(
            c.points.iter().find(|p| p.id == "import").unwrap().origin,
            audio_analysis::CueOrigin::Imported
        );
        assert_eq!(
            c.points.iter().find(|p| p.id == "edited").unwrap().origin,
            audio_analysis::CueOrigin::Manual
        );
        let text = serde_json::to_string(&c).unwrap();
        for retired in ["analysis_version", "tombstones", "anchor", "energy"] {
            assert!(!text.contains(retired));
        }
        c.validate(60.).unwrap();
    }
    #[test]
    fn legacy_slots_remove_only_known_untouched_generated_positions() {
        let value = json!({"hot_cues":[12.0,24.0,36.0,null,null,null,null,null],"memory_cues":[12.0,24.0,null,null,null,null,null,null,null,null],"annotations":{"hot:1":"My arrival","memory:1":"My ending"}});
        let c = clean_cues(
            value,
            &json!({"hot":[[0,12.0],[1,24.0]],"memory":[12.0,24.0]}),
        )
        .unwrap();
        assert_eq!(c.hot_cues[0], None);
        assert_eq!(c.hot_cues[1], Some(24.));
        assert_eq!(c.hot_cues[2], Some(36.));
        assert_eq!(c.memory_cues[0], Some(24.));
        assert!(c.points.iter().any(|p| p.name == "My arrival"));
    }
    #[test]
    fn database_cleanup_is_atomic_idempotent_and_preserves_cached_timing_bits() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE tracks(path TEXT PRIMARY KEY,analysis_json TEXT,hot_cues TEXT,memory_cues TEXT,cue_seconds REAL); CREATE TABLE source_cues(source TEXT PRIMARY KEY,revision INTEGER,cues TEXT); CREATE TABLE spotify_analysis(uri TEXT PRIMARY KEY,metadata TEXT); CREATE TABLE suggest_features(source TEXT,grid_current INTEGER);").unwrap();
        let bpm = 120.00100300902709f64;
        let metadata = json!({"bpm":bpm,"source_fingerprint":"stable","neural_rhythm":{"beats_seconds":[0.04245912656188011,0.541]},"analysis_stages":{"neural":{"version":1,"dependency":"kept"},"sections":{"version":2}},"sections":{"old":true},"auto_cues":{"hot":[[0,12.0]]},"cue_points":[point("auto","analysis",false,false),point("edit","analysis",true,false)]});
        db.execute(
            "INSERT INTO spotify_analysis VALUES(?1,?2)",
            params!["spotify:track:test", metadata.to_string()],
        )
        .unwrap();
        run(&db).unwrap();
        let first: String = db
            .query_row("SELECT metadata FROM spotify_analysis", [], |r| r.get(0))
            .unwrap();
        let saved: Value = serde_json::from_str(&first).unwrap();
        assert_eq!(saved["bpm"].as_f64().unwrap().to_bits(), bpm.to_bits());
        assert_eq!(saved["analysis_stages"]["neural"]["dependency"], "kept");
        assert!(saved.get("sections").is_none() && saved.get("auto_cues").is_none());
        assert_eq!(saved["cue_points"].as_array().unwrap().len(), 1);
        let revision: u64 = db
            .query_row("SELECT revision FROM source_cues", [], |r| r.get(0))
            .unwrap();
        run(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT metadata FROM spotify_analysis", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            first
        );
        assert_eq!(
            db.query_row("SELECT revision FROM source_cues", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            revision
        );
        assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('suggest_features') WHERE name='grid_current'",[],|r|r.get::<_,u64>(0)).unwrap(),0);
    }
}
