// The application observer owns the session clock; pair/source revisions cannot cancel it.
struct MixSession {
    id: String,
    prepared_id: String,
    started: Instant,
    duration: Duration,
    started_unix_ms: u64,
    engine_epoch: u64,
    sample_rate: u32,
    underruns: u64,
    last_rendered_frames: u64,
    last_audio_progress: Instant,
    native_batch_id: Option<u64>,
    state: &'static str,
    reason: Option<String>,
    endpoint_sequence: Option<u64>,
    endpoint_ticket: Option<u64>,
}
impl MixSession {
    fn active(&self) -> bool {
        matches!(self.state, "running" | "finishing")
    }
    fn finish(&mut self, state: &'static str, reason: impl Into<String>) {
        self.state = state;
        self.reason = Some(reason.into());
    }
    fn summary(&self, now: Instant) -> Value {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
        json!({"session_id":self.id,"prepared_id":self.prepared_id,"state":self.state,"reason":self.reason,
            "duration_seconds":self.duration.as_secs_f64(),"elapsed_seconds":elapsed.min(self.duration.as_secs_f64()),
            "remaining_seconds":if self.active(){(self.duration.as_secs_f64()-elapsed).max(0.)}else{0.},
            "started_unix_ms":self.started_unix_ms,"finish_unix_ms":self.started_unix_ms.saturating_add(self.duration.as_millis() as u64),
            "clock":"application_monotonic","endpoint_sequence":self.endpoint_sequence,"endpoint_ticket":self.endpoint_ticket,"resumable_after_restart":false})
    }
}

fn mix_session_duration(seconds: f64) -> Result<Duration> {
    if !seconds.is_finite() || !(1.0..=86_400.0).contains(&seconds) {
        return Err(mix_error(
            "invalid_arguments",
            "Session duration_seconds must be 1..86400",
        ));
    }
    Ok(Duration::from_secs_f64(seconds))
}

fn start_mix_session(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let duration = mix_session_duration(arg(&request.args, "duration_seconds")?)?;
    let prepared_id: String = arg(&request.args, "prepared_id")?;
    let control = app.state::<Control>();
    let takeover_revision = control.mix_takeover_revision.load(Ordering::Acquire);
    let engine_state = app.state::<AppState>();
    let start_fence = engine_state.engine.fence().map_err(failure)?;
    engine_state
        .engine
        .wait_for_fence(
            start_fence,
            Duration::from_millis(request.timeout_ms.min(2_000)),
        )
        .map_err(failure)?;
    let prepared = control
        .mix_prepared
        .lock()
        .map_err(failure)?
        .get(&prepared_id)
        .cloned()
        .ok_or_else(|| {
            mix_error(
                "not_found",
                "Prepare a current suggestion before starting a session",
            )
        })?;
    if prepared.state != "ready" && prepared.state != "armed" {
        return Err(mix_error(
            "not_ready",
            "Session requires a ready or armed prepared transition",
        ));
    }
    check_mix_cancelled(&prepared.cancelled)?;
    let snapshot = app
        .state::<AppState>()
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    if !snapshot.running
        || snapshot.sample_rate == 0
        || !snapshot
            .decks
            .iter()
            .any(|d| d.playing && !d.preparation_pending)
    {
        return Err(mix_error(
            "not_ready",
            "Session needs a running audio engine and a ready playing carrier",
        ));
    }
    if prepared.state == "ready" {
        let expected = prepared
            .snapshot
            .as_ref()
            .ok_or_else(|| mix_error("not_ready", "Prepared snapshot is unavailable"))?;
        if expected.engine_epoch != snapshot.engine_epoch
            || expected.load_revisions != snapshot.load_revisions
            || expected.control_revision != snapshot.control_revision
        {
            return Err(mix_error(
                "stale_preparation",
                "Prepared sources or controls changed; prepare a fresh option",
            ));
        }
        mix_reference_guard(&prepared.option, &snapshot)?;
        validate_mix_readiness(&prepared.option, &snapshot)?;
    }
    let native_batch_id = prepared
        .job_id
        .as_ref()
        .and_then(|id| control.jobs.lock().ok()?.get(id)?.native_id);
    if prepared.state == "armed"
        && (!snapshot.scheduled_batch.state.active()
            || native_batch_id != Some(snapshot.scheduled_batch.id))
    {
        return Err(mix_error(
            "stale_preparation",
            "The selected prepared transition is no longer active",
        ));
    }
    if snapshot.scheduled_batch.state.active()
        && native_batch_id != Some(snapshot.scheduled_batch.id)
    {
        return Err(mix_error(
            "resource_conflict",
            "An unrelated native schedule is active",
        ));
    }
    let now = Instant::now();
    let session = MixSession {
        id: format!(
            "mix-session-{}-{}",
            std::process::id(),
            control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1
        ),
        prepared_id,
        started: now,
        duration,
        started_unix_ms: unix_ms(),
        engine_epoch: snapshot.engine_epoch,
        sample_rate: snapshot.sample_rate,
        underruns: snapshot.underruns,
        last_rendered_frames: snapshot.rendered_frames,
        last_audio_progress: now,
        native_batch_id,
        state: "running",
        reason: None,
        endpoint_sequence: None,
        endpoint_ticket: None,
    };
    let summary = session.summary(now);
    let mut stored = control.mix_session.lock().map_err(failure)?;
    if stored.as_ref().is_some_and(MixSession::active) {
        return Err(mix_error(
            "resource_conflict",
            "Stop the active mix session before starting another",
        ));
    }
    if control.mix_takeover_revision.load(Ordering::Acquire) != takeover_revision {
        return Err(mix_error(
            "manual_takeover",
            "External deck controls changed while starting the session",
        ));
    }
    *stored = Some(session);
    drop(stored);
    event(app, "mix.session.updated", summary.clone());
    control.event_ready.notify_all();
    Ok(("accepted", summary))
}

fn status_mix_session(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let control = app.state::<Control>();
    let stored = control.mix_session.lock().map_err(failure)?;
    let session = stored.as_ref().ok_or_else(|| {
        mix_error(
            "not_found",
            "No mix session in this application instance; sessions never resume after restart",
        )
    })?;
    if optional::<String>(&request.args, "session_id")?.is_some_and(|id| id != session.id) {
        return Err(mix_error("not_found", "Mix session not found"));
    }
    Ok(("applied", session.summary(Instant::now())))
}

fn cancel_mix_session_batch(app: &tauri::AppHandle, session: &MixSession) -> Result<()> {
    let state = app.state::<AppState>();
    let snapshot = state.engine.transport_snapshot();
    if snapshot.engine_epoch == session.engine_epoch
        && snapshot.scheduled_batch.state.active()
        && session.native_batch_id == Some(snapshot.scheduled_batch.id)
    {
        state
            .engine
            .cancel_scheduled_batch(snapshot.scheduled_batch.id, false)
            .map_err(failure)?;
    }
    Ok(())
}

fn stop_mix_session(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let control = app.state::<Control>();
    let mut stored = control.mix_session.lock().map_err(failure)?;
    let session = stored
        .as_mut()
        .ok_or_else(|| mix_error("not_found", "No mix session is active"))?;
    if optional::<String>(&request.args, "session_id")?.is_some_and(|id| id != session.id) {
        return Err(mix_error("not_found", "Mix session not found"));
    }
    if session.active() {
        cancel_mix_session_batch(app, session)?;
        session.finish("stopped", "explicit_stop");
    }
    let summary = session.summary(Instant::now());
    drop(stored);
    event(app, "mix.session.updated", summary.clone());
    Ok(("applied", summary))
}

fn register_mix_session_job(app: &tauri::AppHandle, job_id: &str) -> Result<()> {
    let control = app.state::<Control>();
    let native_id = control
        .jobs
        .lock()
        .map_err(failure)?
        .get(job_id)
        .and_then(|job| job.native_id)
        .ok_or_else(|| mix_error("not_found", "Session transition native job was not found"))?;
    if let Some(session) = control
        .mix_session
        .lock()
        .map_err(failure)?
        .as_mut()
        .filter(|s| s.active())
    {
        session.native_batch_id = Some(native_id);
    }
    Ok(())
}

fn session_failure(
    session: &mut MixSession,
    snapshot: &deck_engine::EngineSnapshot,
    now: Instant,
) -> Option<&'static str> {
    if !session.active() {
        return None;
    }
    if snapshot.engine_epoch != session.engine_epoch
        || snapshot.sample_rate != session.sample_rate
        || !snapshot.running
    {
        return Some("audio_engine_changed_or_stopped");
    }
    if snapshot.underruns != session.underruns {
        return Some("audio_device_error");
    }
    if snapshot.rendered_frames > session.last_rendered_frames {
        session.last_rendered_frames = snapshot.rendered_frames;
        session.last_audio_progress = now;
    } else if now.saturating_duration_since(session.last_audio_progress) > Duration::from_secs(2) {
        return Some("audio_clock_stalled");
    }
    let angle = (snapshot.crossfader + 1.) * std::f32::consts::FRAC_PI_4;
    let gains = [angle.cos(), angle.sin()];
    if snapshot.master_gain <= 0.001
        || !snapshot.decks.iter().enumerate().any(|(i, deck)| {
            deck.playing
                && !deck.preparation_pending
                && deck.channel_fader * deck.gain * gains[i] > 0.001
        })
    {
        return Some("no_audible_carrier_route");
    }
    let batch = &snapshot.scheduled_batch;
    if batch.state.active() && session.native_batch_id != Some(batch.id) {
        return Some("unrelated_schedule_takeover");
    }
    if session.native_batch_id == Some(batch.id)
        && matches!(
            batch.state,
            deck_engine::BatchLifecycle::Failed
                | deck_engine::BatchLifecycle::Interrupted
                | deck_engine::BatchLifecycle::Yielded
                | deck_engine::BatchLifecycle::Cancelled
        )
    {
        return Some("transition_interrupted");
    }
    None
}

/// Called by Control's existing application observer. It never waits for a deck clock.
fn poll_mix_session(app: &tauri::AppHandle) -> Result<()> {
    let control = app.state::<Control>();
    // Arming and registering a pair must appear atomic to the session observer.
    let _submission = match control.scheduled_submission.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(()),
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
    };
    let mut stored = control.mix_session.lock().map_err(failure)?;
    let Some(session) = stored.as_mut().filter(|s| s.active()) else {
        return Ok(());
    };
    let state = app.state::<AppState>();
    let snapshot = state.engine.transport_snapshot();
    let now = Instant::now();
    if session.state == "finishing" {
        if snapshot.engine_epoch != session.engine_epoch || !snapshot.running {
            session.finish("interrupted", "audio_engine_changed_during_endpoint");
        } else if session.endpoint_sequence.is_some_and(|sequence| {
            state
                .engine
                .wait_for_fence(sequence, Duration::ZERO)
                .is_ok()
        }) {
            let receipt = session
                .endpoint_ticket
                .ok_or_else(|| "Session endpoint receipt is missing".to_string())
                .and_then(|ticket| {
                    state
                        .engine
                        .wait_for_control_receipt(ticket, Duration::ZERO)
                });
            match receipt {
                Ok(()) => {
                    let current = state.engine.transport_snapshot();
                    if current.engine_epoch == session.engine_epoch
                        && current
                            .decks
                            .iter()
                            .all(|deck| !deck.playing && !deck.launch_pending)
                    {
                        session.finish("completed", "duration_reached");
                    } else {
                        session.finish("interrupted", "endpoint_pause_not_verified");
                    }
                }
                Err(error) => {
                    session.finish("interrupted", format!("endpoint_guard_rejected: {error}"))
                }
            }
        } else if now.saturating_duration_since(session.started)
            > session.duration + Duration::from_secs(2)
        {
            session.finish("interrupted", "endpoint_acknowledgement_timeout");
        } else {
            return Ok(());
        }
    } else if let Some(reason) = session_failure(session, &snapshot, now) {
        session.finish("interrupted", reason);
    } else if now.saturating_duration_since(session.started) >= session.duration {
        let result = (|| -> Result<(u64, u64)> {
            cancel_mix_session_batch(app, session)?;
            let ticket = state
                .engine
                .pause_transports_for_session(snapshot.engine_epoch, snapshot.control_revision)
                .map_err(failure)?;
            let sequence = state.engine.fence().map_err(failure)?;
            Ok((ticket, sequence))
        })();
        match result {
            Ok((ticket, sequence)) => {
                session.endpoint_ticket = Some(ticket);
                session.endpoint_sequence = Some(sequence);
                session.state = "finishing";
            }
            Err(error) => session.finish("interrupted", error.message),
        }
    } else {
        return Ok(());
    }
    let summary = session.summary(now);
    drop(stored);
    event(app, "mix.session.updated", summary);
    Ok(())
}

fn mix_session_takeover_command(command: &EngineCommand) -> bool {
    use deck_engine::EngineCommand::*;
    !matches!(
        command,
        SetCueGain { .. }
            | SetCueMix { .. }
            | SetHeadphoneCue { .. }
            | SetCue { .. }
            | SetCuePoint { .. }
            | SetHotCue { .. }
            | SetHotCuePoint { .. }
            | ClearHotCue { .. }
            | RestoreHotCues { .. }
            | MemoryCueSave { .. }
            | MemoryCueDelete { .. }
            | RestoreMemoryCues { .. }
            | Ramp {
                target: deck_engine::ControlTarget::CueGain | deck_engine::ControlTarget::CueMix,
                ..
            }
    )
}

/// All accepted GUI, MIDI and external engine controls pass through this hook.
/// Native recipe execution and isolated high-level preparation bypass it.
pub(crate) fn manual_mix_session_takeover(app: &tauri::AppHandle, command: &EngineCommand) {
    if !mix_session_takeover_command(command) {
        return;
    }
    let Some(control) = app.try_state::<Control>() else {
        return;
    };
    control.mix_takeover_revision.fetch_add(1, Ordering::AcqRel);
    manual_successor_queue_takeover(app);
    let Ok(mut stored) = control.mix_session.lock() else {
        return;
    };
    let Some(session) = stored.as_mut().filter(|s| s.active()) else {
        return;
    };
    session.finish("cancelled", "manual_takeover");
    let summary = session.summary(Instant::now());
    drop(stored);
    event(app, "mix.session.updated", summary);
    control.event_ready.notify_all();
}

#[cfg(test)]
mod mix_session_tests {
    use super::*;
    fn fixture() -> (MixSession, deck_engine::EngineSnapshot) {
        let t = deck_engine::TrackAnalysis {
            path: "/test/session.wav".into(),
            duration_seconds: 300.,
            bpm: Some(120.),
            bpm_source: Some("manual".into()),
            beatgrid_source: Some("manual".into()),
            ..Default::default()
        };
        let mut deck = crate::next_move::analysis_deck(&t).unwrap();
        deck.playing = true;
        deck.channel_fader = 1.;
        let snapshot=serde_json::from_value(json!({"decks":[deck.clone(),deck],"load_revisions":[1,2],"sample_rate":48000,"output_channels":2,"buffer_frames":512,"running":true,"rendered_frames":1,"master_peak":[0.,0.],"cue_peak":[0.,0.],"underruns":0,"master_gain":1.,"crossfader":0.,"cue_gain":1.,"cue_mix":1.,"sampler_gain":1.,"sampler_positions_seconds":vec![Value::Null;16],"active_ramps":vec![false;deck_engine::CONTROL_TARGET_COUNT]})).unwrap();
        let now = Instant::now();
        let session = MixSession {
            id: "session".into(),
            prepared_id: "pair".into(),
            started: now,
            duration: Duration::from_secs(3600),
            started_unix_ms: 0,
            engine_epoch: 0,
            sample_rate: 48000,
            underruns: 0,
            last_rendered_frames: 0,
            last_audio_progress: now,
            native_batch_id: None,
            state: "running",
            reason: None,
            endpoint_sequence: None,
            endpoint_ticket: None,
        };
        (session, snapshot)
    }
    #[test]
    fn one_hour_endpoint_survives_repeated_source_grid_and_position_revisions() {
        let (mut session, mut snapshot) = fixture();
        let start = session.started;
        for handover in 1..30 {
            snapshot.load_revisions = [handover, handover + 1];
            snapshot.decks[0].grid_revision += 1;
            snapshot.decks[1].position_revision += 1;
            snapshot.rendered_frames += 48_000 * 60;
            assert_eq!(
                session_failure(
                    &mut session,
                    &snapshot,
                    start + Duration::from_secs(handover * 60)
                ),
                None
            );
            assert_eq!(session.duration, Duration::from_secs(3600));
        }
        assert!(session.active());
        assert_eq!(
            session.summary(start + Duration::from_secs(3599))["remaining_seconds"],
            1.
        );
    }
    #[test]
    fn manual_takeover_and_explicit_stop_revoke_endpoint() {
        let (mut session, mut snapshot) = fixture();
        assert!(mix_session_takeover_command(&EngineCommand::Seek {
            deck: 0,
            seconds: 3.
        }));
        assert!(!mix_session_takeover_command(
            &EngineCommand::SetHeadphoneCue {
                deck: 1,
                enabled: true
            }
        ));
        session.finish("cancelled", "manual_takeover");
        snapshot.engine_epoch += 1;
        let now = session.started + Duration::from_secs(3601);
        assert_eq!(session_failure(&mut session, &snapshot, now), None);
        assert!(!session.active());
        let (mut stopped, snapshot) = fixture();
        stopped.finish("stopped", "explicit_stop");
        assert_eq!(session_failure(&mut stopped, &snapshot, now), None);
        assert!(!stopped.active());
    }
    #[test]
    fn invalid_route_or_stalled_clock_interrupts_instead_of_claiming_completion() {
        let (mut session, mut snapshot) = fixture();
        snapshot.engine_epoch += 1;
        let now = session.started;
        assert_eq!(
            session_failure(&mut session, &snapshot, now),
            Some("audio_engine_changed_or_stopped")
        );
        snapshot.engine_epoch = session.engine_epoch;
        snapshot.rendered_frames = 0;
        assert_eq!(
            session_failure(&mut session, &snapshot, now + Duration::from_secs(3)),
            Some("audio_clock_stalled")
        );
    }
    #[test]
    fn lost_carrier_route_is_reported_as_interrupted() {
        let (mut session, mut snapshot) = fixture();
        let now = session.started;
        snapshot
            .decks
            .iter_mut()
            .for_each(|deck| deck.playing = false);
        assert_eq!(
            session_failure(&mut session, &snapshot, now),
            Some("no_audible_carrier_route")
        );
        snapshot.decks[0].playing = true;
        snapshot.master_gain = 0.;
        assert_eq!(
            session_failure(&mut session, &snapshot, now),
            Some("no_audible_carrier_route")
        );
    }

    #[test]
    fn session_duration_is_finite_positive_and_bounded() {
        for bad in [0., -1., f64::NAN, f64::INFINITY, 86401.] {
            assert!(mix_session_duration(bad).is_err());
        }
        assert_eq!(
            mix_session_duration(3600.).unwrap(),
            Duration::from_secs(3600)
        );
    }
}
