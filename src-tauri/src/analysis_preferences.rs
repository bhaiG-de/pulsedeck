//! Persisted analysis choices, stored as `analysis.json` in the app data folder.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct AnalysisSettings {
    /// Add sections on Spotify sources as well as local files. Core Beat This!
    /// inference remains part of the existing source preparation pipeline.
    pub beat_model_spotify: bool,
    /// Fill empty hot cue pads from song sections.
    pub auto_hot_cues: bool,
    /// Fill empty memory cues from song sections.
    pub auto_memory_cues: bool,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            beat_model_spotify: true,
            auto_hot_cues: true,
            auto_memory_cues: true,
        }
    }
}

/// Missing or unreadable settings fall back to the defaults.
pub fn load(path: &Path) -> AnalysisSettings {
    std::fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &AnalysisSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "Could not create analysis settings folder")?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(settings).map_err(|_| "Could not encode analysis settings")?,
    )
    .map_err(|_| "Could not save analysis settings")?;
    std::fs::rename(temporary, path).map_err(|_| "Could not replace analysis settings".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_default_to_spotify_enabled() {
        let dir = std::env::temp_dir().join(format!("pulsedeck-analysis-{}", std::process::id()));
        let path = dir.join("analysis.json");
        assert!(load(&path).beat_model_spotify);
        save(
            &path,
            &AnalysisSettings {
                beat_model_spotify: false,
                auto_hot_cues: false,
                auto_memory_cues: true,
            },
        )
        .unwrap();
        let saved = load(&path);
        assert!(!saved.beat_model_spotify && !saved.auto_hot_cues && saved.auto_memory_cues);
        // Settings written before the cue options existed keep the defaults.
        std::fs::write(&path, br#"{"beat_model_spotify":false}"#).unwrap();
        let older = load(&path);
        assert!(!older.beat_model_spotify && older.auto_hot_cues && older.auto_memory_cues);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
