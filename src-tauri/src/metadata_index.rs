use crate::{library::TrackRecord, AppState};
use deck_engine::EmbeddedTag;
use std::{path::Path, time::UNIX_EPOCH};
use tauri::{Emitter, Manager};

pub struct Candidate {
    pub id: i64,
    pub path: String,
    pub stamp: String,
}
pub struct Update {
    pub candidate: Candidate,
    pub stamp: String,
    pub fields: Result<Vec<EmbeddedTag>, String>,
}

fn file_stamp(path: &Path) -> Result<String, String> {
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(format!("2:{}:{modified}", metadata.len()))
}

fn read(candidate: Candidate) -> Option<Update> {
    let path = Path::new(&candidate.path);
    let identity = file_stamp(path);
    let stamp = identity.as_deref().unwrap_or("2:unavailable").to_owned();
    if stamp == candidate.stamp {
        return None;
    }
    let mut fields = identity.and_then(|_| deck_engine::read_file_metadata(path));
    if fields.is_ok() && file_stamp(path).as_deref() != Ok(stamp.as_str()) {
        fields = Err("File changed while reading metadata; retry import".into());
    }
    // Failed reads retry on the next import/start, never in a tight loop.
    let stamp = if fields.is_err() {
        format!("{stamp}:retry")
    } else {
        stamp
    };
    Some(Update {
        candidate,
        stamp,
        fields,
    })
}

pub fn search_text(fields: &[EmbeddedTag]) -> String {
    fields
        .iter()
        .flat_map(|f| {
            [
                f.key.as_str(),
                f.standard_key.as_deref().unwrap_or(""),
                f.description.as_str(),
                f.value.as_str(),
            ]
        })
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn refresh(state: &AppState, record: &mut TrackRecord) -> Result<(), String> {
    let stamp = state
        .library
        .lock()
        .map_err(|_| crate::poisoned())?
        .metadata_stamp(record.id)?;
    if let Some(update) = read(Candidate {
        id: record.id,
        path: record.path.clone(),
        stamp,
    }) {
        let changed = state
            .library
            .lock()
            .map_err(|_| crate::poisoned())?
            .store_file_metadata(std::slice::from_ref(&update))?;
        if changed > 0 {
            if let Ok(fields) = update.fields {
                record.file_metadata = fields;
            }
        }
    }
    Ok(())
}

/// One bounded I/O worker. File reads never hold the library or engine lock.
pub fn start(app: &tauri::AppHandle) -> std::io::Result<()> {
    let app = app.clone();
    std::thread::Builder::new().name("pulsedeck-metadata-index".into()).spawn(move || {
        let state = app.state::<AppState>();
        let result = (|| -> Result<(usize, usize), String> {
            let mut after = 0;
            let mut changed = 0;
            let mut failures = 0;
            loop {
                let candidates = state.library.lock().map_err(|_| crate::poisoned())?.metadata_candidates(after)?;
                if candidates.is_empty() { break; }
                after = candidates.last().unwrap().id;
                for batch in candidates.chunks(32) {
                    // Give foreground imports first use of the database and disk.
                    while state.import_busy.load(std::sync::atomic::Ordering::Acquire) {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    let updates: Vec<_> = batch.iter().filter_map(|c| read(Candidate { id: c.id, path: c.path.clone(), stamp: c.stamp.clone() })).collect();
                    if updates.is_empty() { continue; }
                    failures += updates.iter().filter(|u| u.fields.is_err()).count();
                    changed += state.library.lock().map_err(|_| crate::poisoned())?.store_file_metadata(&updates)?;
                }
            }
            Ok((changed, failures))
        })();
        match result {
            Ok((changed, failures)) => {
                if failures > 0 { state.message(format!("Embedded metadata: {failures} files could not be read; saved analysis and labels were retained")); }
                if changed > 0 { let _ = app.emit("library-updated", serde_json::json!({"kind":"metadata"})); }
                eprintln!("Embedded metadata backfill: {changed} updated, {failures} unreadable");
            }
            Err(error) => state.message(format!("Embedded metadata backfill: {error}")),
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_sources_skip_reads_changed_sources_refresh_and_failures_retry() {
        let path =
            std::env::temp_dir().join(format!("pd-metadata-stamp-{}.mp3", std::process::id()));
        std::fs::write(
            &path,
            include_bytes!("../../crates/deck-engine/tests/fixtures/metadata/tags.mp3"),
        )
        .unwrap();
        let candidate = |stamp: String| Candidate {
            id: 1,
            path: path.to_string_lossy().into(),
            stamp,
        };
        let first = read(candidate(String::new())).unwrap();
        assert!(first.fields.is_ok());
        assert!(read(candidate(first.stamp.clone())).is_none());
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[0])
            .unwrap();
        let changed = read(candidate(first.stamp)).unwrap();
        assert_ne!(changed.stamp, changed.candidate.stamp);
        std::fs::remove_file(&path).unwrap();
        let failed = read(candidate(changed.stamp)).unwrap();
        assert!(failed.fields.is_err());
        assert!(read(candidate(failed.stamp)).is_some());
    }
}
