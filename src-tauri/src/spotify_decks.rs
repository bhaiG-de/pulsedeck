use crate::{spotify::NativeSpotifyService, AppState};
use deck_engine::{AudioSourceInfo, TrackAnalysis};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::Manager;

#[derive(Clone, Default, Serialize)]
pub struct DeckSourceStatus {
    pub preparing: bool,
    pub uri: Option<String>,
    pub resolved_uri: Option<String>,
    pub error: Option<String>,
}
#[derive(Default)]
struct Job {
    cancel: Option<Arc<AtomicBool>>,
    status: DeckSourceStatus,
}
#[derive(Default)]
pub struct SourceJobs(Mutex<[Job; 2]>, Mutex<()>, Mutex<()>);
impl SourceJobs {
    pub fn lock_preparation(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.1
            .try_lock()
            .map_err(|_| "Spotify preparation is busy".into())
    }

    pub fn snapshot(&self) -> [DeckSourceStatus; 2] {
        self.0
            .lock()
            .map(|jobs| std::array::from_fn(|i| jobs[i].status.clone()))
            .unwrap_or_default()
    }
    pub fn cancel(&self, deck: usize) {
        if let Ok(mut jobs) = self.0.lock() {
            if let Some(job) = jobs.get_mut(deck) {
                if let Some(cancel) = job.cancel.take() {
                    cancel.store(true, Ordering::Release);
                }
                job.status = DeckSourceStatus::default();
            }
        }
    }
    fn start(&self, deck: usize, uri: String) -> Result<Arc<AtomicBool>, String> {
        let mut jobs = self.0.lock().map_err(|_| "Source state unavailable")?;
        let job = &mut jobs[deck];
        if let Some(cancel) = job.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        job.cancel = Some(cancel.clone());
        job.status = DeckSourceStatus {
            preparing: true,
            uri: Some(uri),
            resolved_uri: None,
            error: None,
        };
        Ok(cancel)
    }
    fn resolution(&self, deck: usize, cancel: &Arc<AtomicBool>, uri: Option<String>) {
        if let Ok(mut jobs) = self.0.lock() {
            let job = &mut jobs[deck];
            if job.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, cancel)) {
                job.status.resolved_uri = uri;
            }
        }
    }
    fn finish(&self, deck: usize, cancel: &Arc<AtomicBool>, error: Option<String>) {
        if let Ok(mut jobs) = self.0.lock() {
            let job = &mut jobs[deck];
            if job.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, cancel)) {
                job.status.preparing = false;
                job.status.error = error;
                job.cancel = None;
            }
        }
    }
}

pub fn clear_account_sources(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.spotify_analysis.cancel();
    let _commit = state
        .source_jobs
        .2
        .lock()
        .map_err(|_| "Source commit state unavailable")?;
    let snapshot = state.engine.snapshot();
    for deck in 0..2 {
        state.source_jobs.cancel(deck);
        state.loading[deck].fetch_add(1, Ordering::AcqRel);
        let _ = state.engine.begin_source_load(deck)?;
        if snapshot.decks[deck]
            .track
            .as_ref()
            .is_some_and(|t| t.path.starts_with("spotify:track:"))
        {
            state.engine.unload(deck)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_spotify_load(app: tauri::AppHandle, deck: usize) -> Result<(), String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    let state = app.state::<AppState>();
    state.source_jobs.cancel(deck);
    state.loading[deck].fetch_add(1, Ordering::AcqRel);
    let _ = state.engine.begin_source_load(deck)?;
    Ok(())
}

#[tauri::command]
pub async fn load_spotify_track(
    app: tauri::AppHandle,
    deck: usize,
    uri: String,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    let id = uri
        .strip_prefix("spotify:track:")
        .ok_or("Choose a Spotify track URI")?;
    if id.len() != 22 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Invalid Spotify track URI".into());
    }
    let state = app.state::<AppState>();
    state.spotify_analysis.interrupt_current();
    let request = state.loading[deck].fetch_add(1, Ordering::AcqRel) + 1;
    let ticket = state.engine.begin_source_load(deck)?;
    let cancel = state.source_jobs.start(deck, uri.clone())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = (|| {
            // One normal playback preparation session at a time; already loaded
            // decks retain independent native transport and can mix together.
            let _preparation = loop {
                if cancel.load(Ordering::Acquire) {
                    return Err("Spotify preparation cancelled".into());
                }
                match state.source_jobs.1.try_lock() {
                    Ok(guard) => break guard,
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(std::time::Duration::from_millis(25))
                    }
                    Err(_) => return Err("Spotify preparation state unavailable".into()),
                }
            };
            let service = app.state::<NativeSpotifyService>();
            let (client_id, access_token) = service.0.playback_credentials()?;
            if cancel.load(Ordering::Acquire) {
                return Err("Spotify preparation cancelled".into());
            }
            let executable =
                std::env::current_exe().map_err(|_| "Could not find playback worker executable")?;
            let source = spotify_playback::prepare_in_worker(
                &executable,
                spotify_playback::PrepareRequest {
                    client_id,
                    access_token,
                    track_uri: uri.clone(),
                    expected_duration_ms: None,
                },
                cancel.clone(),
            )?;
            state
                .source_jobs
                .resolution(deck, &cancel, source.metadata.resolved_uri.clone());
            let _commit = state
                .source_jobs
                .2
                .lock()
                .map_err(|_| "Source commit state unavailable")?;
            if cancel.load(Ordering::Acquire)
                || state.loading[deck].load(Ordering::Acquire) != request
            {
                return Err("Spotify preparation was replaced by a newer load".into());
            }
            let mut cached = state
                .library
                .lock()
                .map_err(|_| "Analysis cache unavailable")?
                .spotify_analysis(&uri)?;
            if source.metadata.resolved_uri.is_some() {
                // Recompute timing/envelopes for a relinked release; retain explicit
                // manual metadata via the engine's cache-independent override path.
                if let Some(cached) = cached.as_mut() {
                    cached.analysis_version = 0;
                }
            }
            let analysis = state.engine.load_decoded_source_cached(
                ticket,
                source.sample_rate,
                source.samples,
                AudioSourceInfo {
                    id: uri.clone(),
                    location: uri,
                    title: source.metadata.title,
                    artist: Some(source.metadata.artists.join(", ")),
                    extension: None,
                },
                false,
                cached,
            )?;
            crate::spotify_analysis::store(&app, &analysis)?;
            crate::control::restore_cues(&app, deck, &analysis.path)?;
            Ok(analysis)
        })();
        state
            .source_jobs
            .finish(deck, &cancel, result.as_ref().err().cloned());
        result
    })
    .await
    .map_err(|_| "Spotify preparation worker failed".to_string())?
}
