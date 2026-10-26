use crate::{spotify::NativeSpotifyService, AppState};
use deck_engine::{AudioSourceInfo, TrackAnalysis};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
use tauri::Manager;

#[derive(Clone, Default, Serialize)]
pub struct DeckSourceStatus {
    pub preparing: bool,
    pub uri: Option<String>,
    pub resolved_uri: Option<String>,
    pub error: Option<String>,
    /// Wall timings for this load, retained after completion; no audio or credentials.
    pub diagnostics: Option<SourceDiagnostics>,
}
#[derive(Clone, Default, Serialize)]
pub struct SourceDiagnostics {
    pub preparation_wait_ms: f64,
    pub provider_ms: f64,
    pub engine_prepare_ms: f64,
    pub persistence_ms: f64,
    pub total_ms: f64,
    pub analysis: Option<deck_engine::AnalysisDiagnostics>,
}
#[derive(Default)]
struct Job {
    owner: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    ticket: Option<deck_engine::SourceLoadTicket>,
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
    /// Cancel only the token carrying the exact unified request owner.
    pub fn cancel_owner(&self, deck: usize, owner: &str) -> bool {
        let Ok(mut jobs) = self.0.lock() else {
            return false;
        };
        let Some(job) = jobs.get_mut(deck) else {
            return false;
        };
        if job.owner.as_deref() != Some(owner) {
            return false;
        }
        if let Some(cancel) = job.cancel.take() {
            cancel.store(true, Ordering::Release);
            job.status = DeckSourceStatus::default();
            true
        } else {
            false
        }
    }
    pub(crate) fn start(
        &self,
        deck: usize,
        uri: String,
        owner: Option<String>,
    ) -> Result<Arc<AtomicBool>, String> {
        let mut jobs = self.0.lock().map_err(|_| "Source state unavailable")?;
        let job = &mut jobs[deck];
        if let Some(cancel) = job.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        job.cancel = Some(cancel.clone());
        job.owner = owner;
        job.ticket = None;
        job.status = DeckSourceStatus {
            preparing: true,
            uri: Some(uri),
            resolved_uri: None,
            error: None,
            diagnostics: None,
        };
        Ok(cancel)
    }
    pub(crate) fn attach_ticket(
        &self,
        deck: usize,
        cancel: &Arc<AtomicBool>,
        ticket: deck_engine::SourceLoadTicket,
    ) {
        if let Ok(mut jobs) = self.0.lock() {
            if jobs[deck]
                .cancel
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, cancel))
            {
                jobs[deck].ticket = Some(ticket);
            }
        }
    }
    pub(crate) fn cancel_owner_source(
        &self,
        deck: usize,
        owner: &str,
        engine: &deck_engine::EngineHandle,
    ) -> Result<bool, String> {
        let mut jobs = self.0.lock().map_err(|_| "Source state unavailable")?;
        let job = &mut jobs[deck];
        if job.owner.as_deref() != Some(owner) {
            return Ok(false);
        }
        let Some(cancel) = job.cancel.take() else {
            return Ok(false);
        };
        cancel.store(true, Ordering::Release);
        if let Some(ticket) = job.ticket.take() {
            engine.cancel_source_load(&ticket)?;
        }
        job.status = DeckSourceStatus::default();
        Ok(true)
    }
    fn resolution(&self, deck: usize, cancel: &Arc<AtomicBool>, uri: Option<String>) {
        if let Ok(mut jobs) = self.0.lock() {
            let job = &mut jobs[deck];
            if job.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, cancel)) {
                // A successful worker preparation without relinking resolves to
                // the requested release; null is reserved for not yet resolved.
                job.status.resolved_uri = uri.or_else(|| job.status.uri.clone());
            }
        }
    }
    fn diagnostics(&self, deck: usize, cancel: &Arc<AtomicBool>, diagnostics: &SourceDiagnostics) {
        if let Ok(mut jobs) = self.0.lock() {
            let job = &mut jobs[deck];
            if job.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, cancel)) {
                job.status.diagnostics = Some(diagnostics.clone());
            }
        }
    }
    pub(crate) fn finish(&self, deck: usize, cancel: &Arc<AtomicBool>, error: Option<String>) {
        if let Ok(mut jobs) = self.0.lock() {
            let job = &mut jobs[deck];
            if job.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, cancel)) {
                job.status.preparing = false;
                job.status.error = error;
                job.cancel = None;
                job.ticket = None;
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

pub async fn load_spotify_track(
    app: tauri::AppHandle,
    deck: usize,
    uri: String,
    owner: Option<String>,
) -> Result<TrackAnalysis, String> {
    if deck > 1 {
        return Err("Deck must be 0 or 1".into());
    }
    spotify_playback::validate_track_uri(&uri)?;
    let state = app.state::<AppState>();
    if owner
        .as_ref()
        .is_some_and(|owner| crate::control::request_owner_cancelled(&app, owner))
    {
        return Err("Queued source request was cancelled".into());
    }
    state.spotify_analysis.interrupt_current();
    let request = state.loading[deck].fetch_add(1, Ordering::AcqRel) + 1;
    let ticket = state.engine.begin_source_load(deck)?;
    let cancel = state.source_jobs.start(deck, uri.clone(), owner.clone())?;
    state
        .source_jobs
        .attach_ticket(deck, &cancel, ticket.clone());
    if owner
        .as_ref()
        .is_some_and(|owner| crate::control::request_owner_cancelled(&app, owner))
    {
        cancel.store(true, Ordering::Release);
        state.source_jobs.finish(
            deck,
            &cancel,
            Some("Queued source request was cancelled".into()),
        );
        return Err("Queued source request was cancelled".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let started = Instant::now();
        let mut diagnostics = SourceDiagnostics::default();
        // Discard an earlier request's worker-local timing before starting this job.
        let _ = deck_engine::take_analysis_diagnostics();
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
            diagnostics.preparation_wait_ms = started.elapsed().as_secs_f64() * 1000.;
            let provider_started = Instant::now();
            let service = app.state::<NativeSpotifyService>();
            let (client_id, access_token) = service.0.playback_credentials()?;
            if cancel.load(Ordering::Acquire) {
                return Err("Spotify preparation cancelled".into());
            }
            let executable =
                std::env::current_exe().map_err(|_| "Could not find playback worker executable")?;
            let prepared = spotify_playback::prepare_in_worker(
                &executable,
                spotify_playback::PrepareRequest {
                    client_id,
                    access_token,
                    track_uri: uri.clone(),
                    expected_duration_ms: None,
                },
                cancel.clone(),
            );
            service
                .0
                .playback_preparation_finished(prepared.as_ref().err().map(String::as_str));
            diagnostics.provider_ms = provider_started.elapsed().as_secs_f64() * 1000.;
            let source = prepared?;
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
            let cached = state
                .library
                .lock()
                .map_err(|_| "Analysis cache unavailable")?
                .spotify_analysis(&uri)?;
            let engine_started = Instant::now();
            let prepared = state.engine.load_decoded_source_cached(
                ticket,
                source.sample_rate,
                source.samples,
                AudioSourceInfo {
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
                cached,
            );
            diagnostics.engine_prepare_ms = engine_started.elapsed().as_secs_f64() * 1000.;
            diagnostics.analysis = deck_engine::take_analysis_diagnostics();
            let analysis = prepared?;
            let persistence_started = Instant::now();
            crate::spotify_analysis::store(&app, &analysis)?;
            crate::control::restore_cues(&app, deck, &analysis.path)?;
            diagnostics.persistence_ms = persistence_started.elapsed().as_secs_f64() * 1000.;
            Ok(analysis)
        })();
        diagnostics.total_ms = started.elapsed().as_secs_f64() * 1000.;
        state.source_jobs.diagnostics(deck, &cancel, &diagnostics);
        state
            .source_jobs
            .finish(deck, &cancel, result.as_ref().err().cloned());
        result
    })
    .await
    .map_err(|_| "Spotify preparation worker failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_decode_cancellation_preserves_a_newer_manual_ticket() {
        let engine = deck_engine::EngineHandle::new().unwrap();
        let jobs = SourceJobs::default();
        let old = engine.begin_source_load(0).unwrap();
        let cancel = jobs
            .start(0, "old-source".into(), Some("queue:source-1".into()))
            .unwrap();
        jobs.attach_ticket(0, &cancel, old);
        let newer = engine.begin_source_load(0).unwrap();
        let manual = jobs
            .start(0, "manual-source".into(), Some("manual".into()))
            .unwrap();
        jobs.attach_ticket(0, &manual, newer.clone());
        assert!(!jobs
            .cancel_owner_source(0, "queue:source-1", &engine)
            .unwrap());
        assert!(!manual.load(Ordering::Acquire));
        assert!(engine.cancel_source_load(&newer).unwrap());
        assert!(!engine.transport_snapshot().running);
    }
    #[test]
    fn resolved_release_includes_non_relinked_sources_and_ignores_old_loads() {
        let jobs = SourceJobs::default();
        let first = jobs.start(0, "spotify:track:first".into(), None).unwrap();
        jobs.resolution(0, &first, None);
        jobs.finish(0, &first, None);
        assert_eq!(
            jobs.snapshot()[0].resolved_uri.as_deref(),
            Some("spotify:track:first")
        );
        let second = jobs.start(0, "spotify:track:second".into(), None).unwrap();
        jobs.resolution(0, &first, Some("spotify:track:stale".into()));
        assert!(jobs.snapshot()[0].resolved_uri.is_none());
        jobs.resolution(0, &second, Some("spotify:track:alternate".into()));
        jobs.finish(0, &second, None);
        assert_eq!(
            jobs.snapshot()[0].resolved_uri.as_deref(),
            Some("spotify:track:alternate")
        );
    }
    #[test]
    fn queue_cancel_never_cancels_a_later_manual_source_owner() {
        let jobs = SourceJobs::default();
        let first = jobs
            .start(0, "spotify:track:queued".into(), Some("job:queued".into()))
            .unwrap();
        let manual = jobs
            .start(
                0,
                "spotify:track:manual".into(),
                Some("manual-request".into()),
            )
            .unwrap();
        assert!(first.load(Ordering::Acquire));
        assert!(!jobs.cancel_owner(0, "job:queued"));
        assert!(!manual.load(Ordering::Acquire));
        assert!(jobs.cancel_owner(0, "manual-request"));
        assert!(manual.load(Ordering::Acquire));
    }
    #[test]
    fn source_timings_survive_completion_but_replaced_load_cannot_overwrite_them() {
        let jobs = SourceJobs::default();
        let first = jobs.start(1, "spotify:track:first".into(), None).unwrap();
        let timing = SourceDiagnostics {
            provider_ms: 120.,
            total_ms: 130.,
            ..Default::default()
        };
        jobs.diagnostics(1, &first, &timing);
        jobs.finish(1, &first, None);
        let status = jobs.snapshot();
        assert!(!status[1].preparing);
        assert_eq!(status[1].diagnostics.as_ref().unwrap().provider_ms, 120.);
        let second = jobs.start(1, "spotify:track:second".into(), None).unwrap();
        jobs.diagnostics(1, &first, &timing);
        jobs.finish(1, &first, Some("late failure".into()));
        let status = jobs.snapshot();
        assert!(status[1].preparing);
        assert!(status[1].diagnostics.is_none());
        assert!(status[1].error.is_none());
        jobs.finish(1, &second, None);
    }
}
