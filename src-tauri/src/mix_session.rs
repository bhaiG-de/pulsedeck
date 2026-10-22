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
    successor: Option<SessionSuccessor>,
    notice: Option<String>,
}
#[derive(Clone, serde::Serialize)]
struct SessionSuccessor {
    deck: usize,
    carrier: usize,
    load_revisions: [u64; 2],
    position_revision: u64,
    carrier_end_seconds: f64,
}
impl MixSession {
    fn active(&self) -> bool {
        matches!(self.state, "running" | "finishing")
    }
    fn finish(&mut self, state: &'static str, reason: impl Into<String>) {
        self.state = state;
        self.reason = Some(reason.into());
        self.successor = None;
    }
    fn summary(&self, now: Instant) -> Value {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
        json!({"session_id":self.id,"prepared_id":self.prepared_id,"state":self.state,"reason":self.reason,
            "duration_seconds":self.duration.as_secs_f64(),"elapsed_seconds":elapsed.min(self.duration.as_secs_f64()),
            "remaining_seconds":if self.active(){(self.duration.as_secs_f64()-elapsed).max(0.)}else{0.},
            "started_unix_ms":self.started_unix_ms,"finish_unix_ms":self.started_unix_ms.saturating_add(self.duration.as_millis() as u64),
            "clock":"application_monotonic","endpoint_sequence":self.endpoint_sequence,"endpoint_ticket":self.endpoint_ticket,
            "successor":self.successor,"continuity_notice":self.notice,"resumable_after_restart":false})
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

fn session_timing(duration: Option<f64>, finish: Option<u64>, now: u64) -> Result<Duration> {
    match (duration, finish) {
        (Some(seconds), None) => mix_session_duration(seconds),
        (None, Some(finish)) => {
            let millis = finish.saturating_sub(now);
            if !(1..=86_400_000).contains(&millis) {
                return Err(mix_error(
                    "invalid_arguments",
                    "finish_unix_ms must be in the future and within 24 hours",
                ));
            }
            Ok(Duration::from_millis(millis))
        }
        _ => Err(mix_error(
            "invalid_arguments",
            "Supply exactly one of duration_seconds or finish_unix_ms",
        )),
    }
}

fn start_mix_session(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let prepared_id: Option<String> = optional(&request.args, "prepared_id")?;
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
    let prepared = prepared_id
        .as_ref()
        .map(|id| {
            control
                .mix_prepared
                .lock()
                .map_err(failure)?
                .get(id)
                .cloned()
                .ok_or_else(|| mix_error("not_found", "Prepared transition not found"))
        })
        .transpose()?;
    if let Some(prepared) = &prepared {
        if prepared.state != "ready" && prepared.state != "armed" {
            return Err(mix_error(
                "not_ready",
                "Session requires a ready or armed prepared transition",
            ));
        }
        check_mix_cancelled(&prepared.cancelled)?;
    }
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
    if let Some(prepared) = prepared.as_ref().filter(|p| p.state == "ready") {
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
        .as_ref()
        .and_then(|p| p.job_id.as_ref())
        .and_then(|id| control.jobs.lock().ok()?.get(id)?.native_id)
        .or_else(|| prepared.is_none().then_some(snapshot.scheduled_batch.id));
    if prepared.as_ref().is_some_and(|p| p.state == "armed")
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
    let started_unix_ms = unix_ms();
    let duration = session_timing(
        optional(&request.args, "duration_seconds")?,
        optional(&request.args, "finish_unix_ms")?,
        started_unix_ms,
    )?;
    let now = Instant::now();
    let session = MixSession {
        id: format!(
            "mix-session-{}-{}",
            std::process::id(),
            control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1
        ),
        prepared_id: prepared_id.unwrap_or_else(|| "direct-performance".into()),
        started: now,
        duration,
        started_unix_ms,
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
        successor: None,
        notice: None,
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

fn arm_session_successor(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    let target = deck(&request.args)?;
    let control = app.state::<Control>();
    let state = app.state::<AppState>();
    let snapshot = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    let mut stored = control.mix_session.lock().map_err(failure)?;
    let session = stored
        .as_mut()
        .filter(|s| s.state == "running")
        .ok_or_else(|| {
            mix_error(
                "not_ready",
                "Start a timed session before arming a continuity successor",
            )
        })?;
    if optional::<String>(&request.args, "session_id")?.is_some_and(|id| id != session.id) {
        return Err(mix_error("not_found", "Mix session not found"));
    }
    if optional::<bool>(&request.args, "cancel")?.unwrap_or(false) {
        session.successor = None;
    } else {
        validate_mix_target_availability(&snapshot.decks[target])?;
        ready_source(
            &snapshot,
            target,
            request
                .expected_load_revision
                .unwrap_or(snapshot.load_revisions[target]),
        )?;
        if !snapshot.decks[1 - target].playing || snapshot.engine_epoch != session.engine_epoch {
            return Err(mix_error(
                "not_ready",
                "The other deck must be the current playing carrier",
            ));
        }
        session.successor = Some(SessionSuccessor {
            deck: target,
            carrier: 1 - target,
            load_revisions: snapshot.load_revisions,
            position_revision: snapshot.decks[target].position_revision,
            carrier_end_seconds: continuity_end(&snapshot.decks[1 - target]),
        });
    }
    let summary = session.summary(Instant::now());
    drop(stored);
    event(app, "mix.session.updated", summary.clone());
    Ok(("armed", summary))
}

fn carrier_route(snapshot: &deck_engine::EngineSnapshot, deck: usize) -> bool {
    let angle = (snapshot.crossfader + 1.) * std::f32::consts::FRAC_PI_4;
    let weight = if deck == 0 { angle.cos() } else { angle.sin() };
    let d = &snapshot.decks[deck];
    snapshot.master_gain > 0.001
        && d.playing
        && !d.preparation_pending
        && d.channel_fader * d.gain * weight > 0.001
}

fn continuity_end(deck: &deck_engine::DeckSnapshot) -> f64 {
    let measured = crate::next_move::useful_end_seconds(deck);
    if measured > 0. {
        measured
    } else {
        deck.track.as_ref().map_or(0., |t| t.duration_seconds)
    }
}

fn successor_due(successor: &SessionSuccessor, snapshot: &deck_engine::EngineSnapshot) -> bool {
    let carrier = &snapshot.decks[successor.carrier];
    !snapshot.scheduled_batch.state.active()
        && snapshot.load_revisions == successor.load_revisions
        && snapshot.decks[successor.deck].position_revision == successor.position_revision
        && !snapshot.decks[successor.deck].playing
        && !snapshot.decks[successor.deck].preparation_pending
        && carrier.loop_start_seconds.is_none()
        && successor.carrier_end_seconds - carrier.position_seconds
            <= carrier.actual_playback_rate.max(0.1)
}

// Only an explicitly armed source can take this last-second, non-phrase fallback.
fn start_session_successor(
    app: &tauri::AppHandle,
    session: &MixSession,
    successor: &SessionSuccessor,
) -> Result<()> {
    let state = app.state::<AppState>();
    let target = successor.deck;
    let mut commands = vec![
        EngineCommand::SetSync {
            deck: target,
            enabled: false,
        },
        EngineCommand::SetDeckGain {
            deck: target,
            gain: 1.,
        },
        EngineCommand::SetEq {
            deck: target,
            low: 1.,
            mid: 1.,
            high: 1.,
        },
        EngineCommand::SetFilter {
            deck: target,
            amount: 0.,
        },
        EngineCommand::SetFx {
            deck: target,
            echo: 0.,
            reverb: 0.,
        },
        EngineCommand::SetMaster { deck: target },
        EngineCommand::Play { deck: target },
        EngineCommand::SetChannelFader {
            deck: target,
            value: 1.,
        },
    ];
    // Leave the outgoing route alone until the chosen incoming is actually audible.
    commands.push(EngineCommand::Pause {
        deck: successor.carrier,
    });
    commands.push(EngineCommand::SetChannelFader {
        deck: successor.carrier,
        value: 0.,
    });
    let mut revision = state.engine.transport_snapshot().control_revision;
    for command in commands {
        let snapshot = state.engine.transport_snapshot();
        let angle = (snapshot.crossfader + 1.) * std::f32::consts::FRAC_PI_4;
        let weight = if target == 0 {
            angle.cos()
        } else {
            angle.sin()
        };
        if snapshot.engine_epoch != session.engine_epoch
            || snapshot.load_revisions != successor.load_revisions
            || snapshot.control_revision != revision
            || snapshot.scheduled_batch.state.active()
            || snapshot.master_gain <= 0.001
            || weight <= 0.001
        {
            return Err(mix_error(
                "source_changed",
                "Continuity successor ownership or output route changed",
            ));
        }
        if matches!(command, EngineCommand::Pause { .. }) && !carrier_route(&snapshot, target) {
            return Err(mix_error(
                "not_ready",
                "Continuity successor did not establish an audible route",
            ));
        }
        let d = command.deck().unwrap();
        let ticket = state
            .engine
            .command_for_control_revision(
                command,
                Some(snapshot.load_revisions[d]),
                snapshot.control_revision,
            )
            .map_err(failure)?;
        state
            .engine
            .wait_for_control_receipt(ticket, Duration::from_millis(100))
            .map_err(failure)?;
        revision = revision.wrapping_add(1);
    }
    Ok(())
}

fn continuity_notice(
    snapshot: &deck_engine::EngineSnapshot,
    durations: [Option<f64>; 2],
) -> Option<String> {
    if !(0..2).any(|d| carrier_route(snapshot, d)) {
        return Some("no_audible_carrier_route".into());
    }
    if !snapshot.scheduled_batch.state.active()
        && snapshot.decks.iter().enumerate().any(|(i, d)| {
            carrier_route(snapshot, i)
                && d.loop_start_seconds.is_none()
                && durations[i].is_some_and(|duration| {
                    (duration - d.position_seconds) / d.actual_playback_rate.max(0.1) <= 60.
                })
        })
    {
        return Some("successor_due".into());
    }
    None
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
                Err(_)
                    if now.saturating_duration_since(session.started)
                        <= session.duration + Duration::from_secs(2) =>
                {
                    session.state = "running";
                    session.endpoint_ticket = None;
                    session.endpoint_sequence = None;
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
            Err(_)
                if now.saturating_duration_since(session.started)
                    <= session.duration + Duration::from_secs(2) =>
            {
                return Ok(())
            }
            Err(error) => session.finish("interrupted", error.message),
        }
    } else {
        let mut notice = continuity_notice(
            &snapshot,
            [
                state.engine.track_duration(0),
                state.engine.track_duration(1),
            ],
        );
        if let Some(successor) = session.successor.clone() {
            if snapshot.load_revisions != successor.load_revisions
                || snapshot.decks[successor.deck].position_revision != successor.position_revision
                || snapshot.decks[successor.deck].playing
            {
                session.successor = None;
                notice = Some(
                    if snapshot.decks[successor.deck].playing {
                        "continuity_successor_consumed"
                    } else {
                        "continuity_successor_invalidated"
                    }
                    .into(),
                );
            } else if successor_due(&successor, &snapshot) {
                session.successor = None;
                notice = Some(match start_session_successor(app, session, &successor) {
                    Ok(()) => "continuity_successor_started".into(),
                    Err(error) => format!("continuity_successor_failed: {}", error.message),
                });
            }
        }
        if session.notice == notice {
            return Ok(());
        }
        session.notice = notice;
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
        SetMasterGain { gain } if *gain > 0.001
    ) && !matches!(
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

/// Human gestures revoke automation. Performance-client gestures keep the endpoint.
pub(crate) fn manual_mix_session_takeover(app: &tauri::AppHandle, command: &EngineCommand) {
    mix_session_control(app, command, true);
}

pub(crate) fn mix_session_control(
    app: &tauri::AppHandle,
    command: &EngineCommand,
    human_control: bool,
) {
    if !mix_session_takeover_command(command) {
        return;
    }
    let Some(control) = app.try_state::<Control>() else {
        return;
    };
    control.mix_takeover_revision.fetch_add(1, Ordering::AcqRel);
    manual_successor_queue_takeover(app);
    let snapshot = app.state::<AppState>().engine.transport_snapshot();
    if !session_control_revokes(command, human_control, &snapshot) {
        return;
    }
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

fn session_control_revokes(
    command: &EngineCommand,
    human_control: bool,
    snapshot: &deck_engine::EngineSnapshot,
) -> bool {
    if !mix_session_takeover_command(command) {
        return false;
    }
    human_control
        || match command {
            EngineCommand::Pause { deck } => {
                !carrier_route(snapshot, 1 - deck)
            }
            _ => false,
        }
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
            successor: None,
            notice: None,
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
    fn lost_carrier_route_reports_attention_but_keeps_original_endpoint() {
        let (mut session, mut snapshot) = fixture();
        let now = session.started;
        snapshot
            .decks
            .iter_mut()
            .for_each(|deck| deck.playing = false);
        assert_eq!(session_failure(&mut session, &snapshot, now), None);
        assert_eq!(
            continuity_notice(&snapshot, [Some(300.); 2]).as_deref(),
            Some("no_audible_carrier_route")
        );
        assert!(session.active());
        snapshot.decks[0].playing = true;
        snapshot.master_gain = 0.;
        assert_eq!(session_failure(&mut session, &snapshot, now), None);
        assert_eq!(
            continuity_notice(&snapshot, [Some(300.); 2]).as_deref(),
            Some("no_audible_carrier_route")
        );
        assert!(session.active());
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

    #[test]
    fn performance_tempo_and_ramps_keep_timer_but_human_takeovers_and_stops_revoke_it() {
        let (_, mut snapshot) = fixture();
        for command in [
            EngineCommand::SetBpm {
                deck: 0,
                bpm: 122.9,
            },
            EngineCommand::Ramp {
                target: deck_engine::ControlTarget::TempoPercent { deck: 0 },
                value: 1.,
                seconds: 8.,
                clock: Some(0),
                beats: None,
                quantize: None,
            },
        ] {
            assert!(!session_control_revokes(&command, false, &snapshot));
            assert!(session_control_revokes(&command, true, &snapshot));
        }
        assert!(!session_control_revokes(
            &EngineCommand::SetMasterGain { gain: 1.5 },
            true,
            &snapshot
        ));
        assert!(!session_control_revokes(
            &EngineCommand::Pause { deck: 0 },
            false,
            &snapshot
        ));
        snapshot.decks[1].playing = false;
        assert!(session_control_revokes(
            &EngineCommand::Pause { deck: 0 },
            false,
            &snapshot
        ));
    }

    #[test]
    fn armed_successor_fires_before_eof_and_rejects_replaced_or_repositioned_sources() {
        let (_, mut snapshot) = fixture();
        assert_eq!(continuity_end(&snapshot.decks[0]), 300.);
        snapshot.decks[1].playing = false;
        snapshot.decks[1].channel_fader = 0.;
        let successor = SessionSuccessor {
            deck: 1,
            carrier: 0,
            load_revisions: snapshot.load_revisions,
            position_revision: snapshot.decks[1].position_revision,
            carrier_end_seconds: 300.,
        };
        snapshot.decks[0].position_seconds = 298.;
        assert!(!successor_due(&successor, &snapshot));
        snapshot.decks[0].position_seconds = 299.2;
        assert!(successor_due(&successor, &snapshot));
        snapshot.decks[0].playing = false;
        snapshot.decks[0].position_seconds = 300.;
        assert!(successor_due(&successor, &snapshot));
        snapshot.load_revisions[1] += 1;
        assert!(!successor_due(&successor, &snapshot));
        snapshot.load_revisions[1] -= 1;
        snapshot.decks[1].position_revision += 1;
        assert!(!successor_due(&successor, &snapshot));
    }

    #[test]
    fn absolute_endpoint_does_not_drift_on_recovery() {
        let finish = 1_000_000;
        for now in [100, 7_354, 100_000] {
            let duration = session_timing(None, Some(finish), now).unwrap();
            assert_eq!(now + duration.as_millis() as u64, finish);
        }
        for millis in 1..2000 {
            assert_eq!(
                session_timing(None, Some(finish), finish - millis)
                    .unwrap()
                    .as_millis(),
                millis as u128
            );
        }
        assert!(session_timing(None, Some(finish), finish + 1).is_err());
        assert!(session_timing(Some(60.), Some(finish), 100).is_err());
    }
}
