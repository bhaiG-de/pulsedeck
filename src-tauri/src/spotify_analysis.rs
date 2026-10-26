use crate::{spotify::NativeSpotifyService, AppState};
use deck_engine::{AudioSourceInfo, TrackAnalysis};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize)]
pub struct BatchStatus {
    pub active: bool,
    pub total: usize,
    pub completed: usize,
    pub cached: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub current_uri: Option<String>,
    pub last_error: Option<String>,
}
#[derive(Default)]
struct Batch {
    status: BatchStatus,
    cancel: Option<Arc<AtomicBool>>,
    current: Option<Arc<AtomicBool>>,
}
#[derive(Default)]
pub struct BatchService(Mutex<Batch>);
impl BatchService {
    pub fn snapshot(&self) -> BatchStatus {
        self.0.lock().map(|b| b.status.clone()).unwrap_or_default()
    }
    pub fn cancel(&self) {
        if let Ok(b) = self.0.lock() {
            if let Some(cancel) = &b.cancel {
                cancel.store(true, Ordering::Release);
            }
            if let Some(current) = &b.current {
                current.store(true, Ordering::Release);
            }
        }
    }
    pub fn interrupt_current(&self) {
        if let Ok(b) = self.0.lock() {
            if let Some(current) = &b.current {
                current.store(true, Ordering::Release);
            }
        }
    }
}
#[derive(Clone, Serialize)]
pub struct AnalysisSummary {
    pub uri: String,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub camelot: Option<String>,
    pub bpm_ambiguous: bool,
    pub bpm_confidence: f32,
    pub key_confidence: Option<f32>,
}
impl From<&TrackAnalysis> for AnalysisSummary {
    fn from(a: &TrackAnalysis) -> Self {
        Self {
            uri: a.path.clone(),
            bpm: a.bpm,
            key: a.key.clone(),
            camelot: a.camelot.clone(),
            bpm_ambiguous: a.bpm_ambiguous,
            bpm_confidence: a.bpm_confidence,
            key_confidence: a.key_confidence,
        }
    }
}
pub fn validate_uri(uri: &str) -> Result<(), String> {
    let id = uri
        .strip_prefix("spotify:track:")
        .ok_or("Choose a Spotify track URI")?;
    if id.len() != 22 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Invalid Spotify track URI".into());
    }
    Ok(())
}
pub fn store(app: &tauri::AppHandle, analysis: &TrackAnalysis) -> Result<(), String> {
    let changed = app
        .state::<AppState>()
        .library
        .lock()
        .map_err(|_| "Analysis cache unavailable")?
        .cache_spotify_analysis(analysis)?;
    if changed {
        let _ = app.emit("spotify-analysis-updated", AnalysisSummary::from(analysis));
    }
    Ok(())
}
pub fn spotify_cached_analysis(
    app: tauri::AppHandle,
    uris: Vec<String>,
) -> Result<Vec<AnalysisSummary>, String> {
    if uris.len() > 100 {
        return Err("Request at most 100 cached tracks".into());
    }
    let state = app.state::<AppState>();
    let cache = state
        .library
        .lock()
        .map_err(|_| "Analysis cache unavailable")?;
    let mut result = Vec::new();
    for uri in uris {
        validate_uri(&uri)?;
        if let Some(a) = cache.spotify_analysis(&uri)? {
            result.push(AnalysisSummary::from(&a));
        }
    }
    Ok(result)
}
pub fn spotify_analysis_status(app: tauri::AppHandle) -> BatchStatus {
    app.state::<AppState>().spotify_analysis.snapshot()
}
pub fn cancel_spotify_analysis(app: tauri::AppHandle) {
    app.state::<AppState>().spotify_analysis.cancel()
}

pub fn analyze_spotify_page(
    app: tauri::AppHandle,
    uris: Vec<String>,
) -> Result<BatchStatus, String> {
    if uris.is_empty() || uris.len() > 50 {
        return Err("Choose 1–50 tracks on the current page".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut queue = Vec::new();
    for uri in uris {
        validate_uri(&uri)?;
        if seen.insert(uri.clone()) {
            queue.push(uri);
        }
    }
    // Fail promptly if native sign-in is absent, before scheduling any work.
    app.state::<NativeSpotifyService>()
        .0
        .playback_credentials()?;
    let state = app.state::<AppState>();
    let cancel = Arc::new(AtomicBool::new(false));
    let status = BatchStatus {
        active: true,
        total: queue.len(),
        ..Default::default()
    };
    {
        let mut job = state
            .spotify_analysis
            .0
            .lock()
            .map_err(|_| "Analysis state unavailable")?;
        if job.status.active {
            return Err("A page analysis is already running".into());
        }
        *job = Batch {
            status: status.clone(),
            cancel: Some(cancel.clone()),
            current: None,
        };
    }
    tauri::async_runtime::spawn_blocking(move || run_page(&app, queue, cancel));
    Ok(status)
}
fn run_page(app: &tauri::AppHandle, queue: Vec<String>, cancel: Arc<AtomicBool>) {
    let state = app.state::<AppState>();
    'tracks: for uri in queue {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let cached = state
            .library
            .lock()
            .ok()
            .and_then(|cache| cache.spotify_analysis(&uri).ok().flatten());
        // Upgrade only entries missing compatible persisted timing or PCM measurements.
        if cached.as_ref().is_some_and(crate::source_analysis_current) {
            if let Ok(mut b) = state.spotify_analysis.0.lock() {
                b.status.completed += 1;
                b.status.cached += 1;
            }
            continue;
        }
        loop {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            // Already loaded decks continue to play; pending deck loads take priority.
            if state.source_jobs.snapshot().iter().any(|job| job.preparing) {
                std::thread::sleep(std::time::Duration::from_millis(25));
                continue;
            }
            let Ok(guard) = state.source_jobs.lock_preparation() else {
                std::thread::sleep(std::time::Duration::from_millis(25));
                continue;
            };
            let current = Arc::new(AtomicBool::new(false));
            if let Ok(mut b) = state.spotify_analysis.0.lock() {
                b.current = Some(current.clone());
                b.status.current_uri = Some(uri.clone());
            }
            if cancel.load(Ordering::Acquire)
                || state.source_jobs.snapshot().iter().any(|job| job.preparing)
            {
                current.store(true, Ordering::Release);
            }
            let result = (|| {
                let (client_id, access_token) = app
                    .state::<NativeSpotifyService>()
                    .0
                    .playback_credentials()?;
                let executable = std::env::current_exe()
                    .map_err(|_| "Could not find playback worker executable")?;
                let source = spotify_playback::prepare_in_worker(
                    &executable,
                    spotify_playback::PrepareRequest {
                        client_id,
                        access_token,
                        track_uri: uri.clone(),
                        expected_duration_ms: None,
                    },
                    current.clone(),
                );
                app.state::<NativeSpotifyService>()
                    .0
                    .playback_preparation_finished(source.as_ref().err().map(String::as_str));
                let source = source?;
                if current.load(Ordering::Acquire) {
                    return Err("Analysis interrupted".into());
                }
                let mut analysis = deck_engine::analyze_decoded_audio_cached(
                    &source.samples,
                    source.sample_rate,
                    &AudioSourceInfo {
                        id: uri.clone(),
                        location: uri.clone(),
                        title: source.metadata.title,
                        artist: Some(source.metadata.artists.join(", ")),
                        extension: None,
                        release_identity: Some(
                            source
                                .metadata
                                .resolved_uri
                                .clone()
                                .unwrap_or_else(|| uri.clone()),
                        ),
                    },
                    false,
                    cached.clone(),
                );
                let interrupted = || current.load(Ordering::Acquire);
                let mut model_error = None;
                {
                    match deck_engine::analyze_beats(
                        &source.samples,
                        source.sample_rate,
                        &mut analysis,
                        &interrupted,
                    ) {
                        Ok(()) => {}
                        Err(_) if interrupted() => return Err("Analysis interrupted".into()),
                        Err(e) => model_error = Some(e),
                    }
                }
                if interrupted() {
                    return Err("Analysis interrupted".into());
                }
                store(app, &analysis)?;
                model_error.map_or(Ok(()), |e| Err(format!("Beat analysis failed: {e}")))
            })();
            drop(guard);
            if let Ok(mut b) = state.spotify_analysis.0.lock() {
                b.current = None;
            }
            if cancel.load(Ordering::Acquire) {
                break;
            }
            if current.load(Ordering::Acquire) {
                continue;
            }
            let provider_blocked = result.as_ref().err().is_some_and(|error| {
                spotify_playback::is_playback_key_failure(error)
                    || error.starts_with("Spotify native playback is cooling down")
            });
            if let Ok(mut b) = state.spotify_analysis.0.lock() {
                b.status.completed += 1;
                if let Err(error) = result {
                    b.status.failed += 1;
                    b.status.last_error = Some(error);
                }
            }
            if provider_blocked {
                break 'tracks;
            }
            break;
        }
    }
    if let Ok(mut b) = state.spotify_analysis.0.lock() {
        b.status.active = false;
        b.status.cancelled = cancel.load(Ordering::Acquire);
        b.status.current_uri = None;
        b.current = None;
        b.cancel = None;
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deck_priority_interrupts_only_current_work_and_user_cancel_stops_the_page() {
        let service = BatchService::default();
        let page = Arc::new(AtomicBool::new(false));
        let current = Arc::new(AtomicBool::new(false));
        {
            let mut b = service.0.lock().unwrap();
            b.cancel = Some(page.clone());
            b.current = Some(current.clone());
        }
        service.interrupt_current();
        assert!(current.load(Ordering::Acquire));
        assert!(!page.load(Ordering::Acquire));
        service.cancel();
        assert!(page.load(Ordering::Acquire));
        assert!(validate_uri("spotify:track:0000000000000000000000").is_ok());
        assert!(validate_uri("spotify:track:../../token").is_err());
    }
}
