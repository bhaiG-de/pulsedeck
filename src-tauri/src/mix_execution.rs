// The high-level recipe service shares Control's native executor and receipts.
#[derive(Clone)]
struct PreparedMix {
    id: String,
    option: crate::next_move::MoveOption,
    state: String,
    snapshot: Option<deck_engine::EngineSnapshot>,
    job_id: Option<String>,
    error: Option<ControlError>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    preview_cue: Option<(bool, u64)>,
    preview_deadline: Option<Instant>,
    automatic: bool,
    fallback_used: bool,
    preparation_deadline: Option<Instant>,
    restored: bool,
    arm_confirmation_deadline: Option<Instant>,
    recovery_attempts: u8,
    takeover_revision: u64,
}

fn mix_error(code: &str, message: &str) -> ControlError {
    ControlError::new(code, message)
}
fn mix_source_matches(
    app: &tauri::AppHandle,
    track: &deck_engine::TrackAnalysis,
    source: &str,
) -> bool {
    if track.id == source || track.path == source {
        return true;
    }
    source
        .strip_prefix("local:")
        .and_then(|id| id.parse::<i64>().ok())
        .and_then(|id| {
            app.state::<AppState>()
                .library
                .lock()
                .ok()?
                .path_for_id(id)
                .ok()
        })
        .is_some_and(|path| path == track.path)
}
fn mix_summary(prepared: &PreparedMix) -> Value {
    json!({"prepared_id":prepared.id,"operation_id":prepared.id,"state":prepared.state,
        "option":prepared.option,"job_id":prepared.job_id,"error":prepared.error,
        "automatic":prepared.automatic,"fallback_used":prepared.fallback_used,"restored":prepared.restored,
        "recovery_attempts":prepared.recovery_attempts,
        "terminal":mix_terminal(&prepared.state),"event_sequence":null,
        "ready":prepared.state=="ready","previewing":prepared.state=="previewing","transport_started":prepared.state=="previewing", "preview_remaining_seconds":prepared.preview_deadline.map(|deadline|deadline.saturating_duration_since(Instant::now()).as_secs_f64())})
}
fn mix_terminal(state: &str) -> bool {
    matches!(
        state,
        "completed"
            | "recovered"
            | "failed"
            | "cancelled"
            | "yielded"
            | "interrupted"
            | "invalidated"
    )
}

fn mix_event_feedback(summary: &Value) -> Value {
    let option = &summary["option"];
    let error = summary["error"].as_object().map(|_| {
        json!({
            "code":summary["error"]["code"],"message":summary["error"]["message"]
        })
    });
    json!({"operation_id":summary["operation_id"],"state":summary["state"],
        "terminal":summary["state"].as_str().is_some_and(mix_terminal),
        "job_id":summary["job_id"],"fallback_used":summary["fallback_used"],
        "error":error,"source":option["source"],"title":option["title"],
        "technique":option["technique"],"launch_seconds":option["launch_seconds"],
        "clear_seconds":option["clear_seconds"],"continuation_seconds":option["continuation_seconds"],
        "next_decision_within_seconds":option["next_decision_within_seconds"],
        "decision_required":summary["state"] == "completed" || summary["state"] == "recovered" || summary["state"] == "failed"})
}
fn check_mix_cancelled(token: &std::sync::atomic::AtomicBool) -> Result<()> {
    if token.load(Ordering::Acquire) {
        Err(mix_error("cancelled", "Mix preparation was cancelled"))
    } else {
        Ok(())
    }
}
fn mix_reference_identity_guard(
    option: &crate::next_move::MoveOption,
    snapshot: &deck_engine::EngineSnapshot,
) -> Result<()> {
    let d = option.reference_deck;
    if d > 1 || option.target_deck != 1 - d {
        return Err(mix_error(
            "invalid_arguments",
            "Transition needs two distinct decks",
        ));
    }
    if snapshot.engine_epoch != option.engine_epoch
        || snapshot.decks[d]
            .track
            .as_ref()
            .is_none_or(|track| track.path != option.reference_source)
        || snapshot.load_revisions[d] != option.reference_load_revision
        || snapshot.decks[d].grid_revision != option.reference_grid_revision
        || snapshot.decks[d].position_revision != option.reference_position_revision
        || snapshot.deck_control_revisions[d] != option.reference_control_revision
    {
        return Err(mix_error(
            "stale_option",
            "Reference source, grid, position or audio engine changed; request fresh suggestions",
        ));
    }
    if !snapshot.running || !snapshot.decks[d].playing || snapshot.decks[d].preparation_pending {
        return Err(mix_error(
            "not_ready",
            "A ready playing reference deck is required",
        ));
    }
    Ok(())
}
fn mix_reference_guard(
    option: &crate::next_move::MoveOption,
    snapshot: &deck_engine::EngineSnapshot,
) -> Result<()> {
    mix_reference_identity_guard(option, snapshot)?;
    let d = option.reference_deck;
    if option.launch_seconds
        <= snapshot.decks[d].position_seconds + 2.0 * snapshot.decks[d].actual_playback_rate
    {
        return Err(mix_error(
            "deadline_expired",
            "Transition launch is too close or has passed; request fresh suggestions",
        ));
    }
    Ok(())
}
fn mix_prepare_controls(option: &crate::next_move::MoveOption) -> Vec<EngineCommand> {
    let deck = option.target_deck;
    vec![
        EngineCommand::SetSync {
            deck,
            enabled: false,
        },
        EngineCommand::SetChannelFader { deck, value: 0. },
        EngineCommand::SetDeckGain { deck, gain: 1. },
        EngineCommand::SetKeylock {
            deck,
            enabled: true,
        },
        EngineCommand::SetPitch {
            deck,
            semitones: 0.,
        },
        EngineCommand::SetFx {
            deck,
            echo: 0.,
            reverb: 0.,
        },
        EngineCommand::SetEq {
            deck,
            low: if option.technique == "eq_blend" {
                0.
            } else {
                1.
            },
            mid: 1.,
            high: 1.,
        },
        EngineCommand::SetFilter {
            deck,
            amount: if option.technique == "filter_blend" {
                0.65
            } else {
                0.
            },
        },
        EngineCommand::Seek {
            deck,
            seconds: option.entry_seconds,
        },
        EngineCommand::SetPlaybackBpm {
            deck,
            bpm: option.target_bpm,
        },
    ]
}
fn validate_mix_target_availability(deck: &deck_engine::DeckSnapshot) -> Result<()> {
    if deck.playing
        || deck.launch_pending
        || deck.jog_touch
        || deck.preparation_pending
        || (deck.track.is_some() && deck.channel_fader > 0.001)
    {
        return Err(mix_error(
            "deck_protected",
            "The target deck must be paused, isolated and free of pending work",
        ));
    }
    Ok(())
}
fn prepare_mix(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    begin_mix(app, request, false)
}
fn execute_mix(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    begin_mix(app, request, true)
}
fn begin_mix(
    app: &tauri::AppHandle,
    request: &Request,
    automatic: bool,
) -> Result<(&'static str, Value)> {
    let option_id: String = arg(&request.args, "option_id")?;
    let option = crate::next_move::resolve_option(app, &option_id)
        .map_err(|e| mix_error("stale_option", &e))?;
    let c = app.state::<Control>();
    let _preparation = c
        .mix_preparing
        .try_lock()
        .map_err(|_| mix_error("busy", "Another mix preparation is in progress"))?;
    if c.mix_prepared
        .lock()
        .map_err(failure)?
        .values()
        .any(|p| p.automatic && !mix_terminal(&p.state))
    {
        return Err(mix_error(
            "busy",
            "An owned move is already preparing or executing",
        ));
    }
    if automatic
        && option.review_required
        && !optional::<bool>(&request.args, "review_confirmed")?.unwrap_or(false)
    {
        return Err(mix_error(
            "review_required",
            "Review the option evidence and accept its uncertain phrases/vocals before executing",
        ));
    }
    let state = app.state::<AppState>();
    let initial = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    mix_reference_guard(&option, &initial)?;
    if initial.scheduled_batch.state.active() || initial.active_ramps.iter().any(|v| *v) {
        return Err(mix_error(
            "resource_conflict",
            "Let the active transition or automation settle before preparing another pair",
        ));
    }
    let target = option.target_deck;
    let deck = &initial.decks[target];
    if c.has_pending_load(target) || state.source_jobs.snapshot()[target].preparing {
        return Err(mix_error(
            "deck_protected",
            "A source load already owns the target deck",
        ));
    }
    validate_mix_target_availability(deck)?;
    let matches = deck
        .track
        .as_ref()
        .is_some_and(|track| mix_source_matches(app, track, &option.source));
    if deck.track.is_some()
        && !matches
        && !optional::<bool>(&request.args, "replace_prepared")?.unwrap_or(false)
    {
        return Err(mix_error("deck_protected", "The target deck contains a prepared source; explicitly confirm replacement or clear it first"));
    }
    let id = format!(
        "mix-{}-{}",
        std::process::id(),
        c.job_sequence.fetch_add(1, Ordering::Relaxed) + 1
    );
    let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
    refresh_scheduled_jobs(app)?;
    let execution_states: HashMap<String, String> = c
        .jobs
        .lock()
        .map_err(failure)?
        .iter()
        .filter_map(|(id, j)| {
            j.status["state"]
                .as_str()
                .map(|state| (id.clone(), state.to_owned()))
        })
        .collect();
    {
        let mut stored = c.mix_prepared.lock().map_err(failure)?;
        for prepared in stored.values_mut() {
            if let Some(state) = prepared
                .job_id
                .as_ref()
                .and_then(|id| execution_states.get(id))
            {
                prepared.state = state.clone();
            }
        }
        // Only the current pair is executable. History is bounded independently of native slots.
        for prepared in stored.values_mut().filter(|p| p.state == "ready") {
            prepared.state = "invalidated".into();
            event(app, "mix.updated", mix_summary(prepared));
        }
        if stored.len() >= 64 {
            let expired = stored
                .iter()
                .find(|(_, p)| p.state != "armed" && p.state != "preparing")
                .map(|(id, _)| id.clone());
            if let Some(expired) = expired {
                stored.remove(&expired);
            } else {
                return Err(mix_error(
                    "busy",
                    "Mix operation history is full of active work",
                ));
            }
        }
        stored.insert(
            id.clone(),
            PreparedMix {
                id: id.clone(),
                option: option.clone(),
                state: "preparing".into(),
                snapshot: None,
                job_id: None,
                error: None,
                cancelled: token.clone(),
                preview_cue: None,
                preview_deadline: None,
                automatic,
                fallback_used: false,
                restored: false,
                arm_confirmation_deadline: None,
                recovery_attempts: 0,
                takeover_revision: c.mix_takeover_revision.load(Ordering::Acquire),
                preparation_deadline: automatic.then(|| {
                    let reference = &initial.decks[option.reference_deck];
                    let remaining = (crate::next_move::useful_end_seconds(reference)
                        - reference.position_seconds)
                        / reference.actual_playback_rate;
                    Instant::now()
                        + Duration::from_secs_f64(
                            (remaining - 20. * 60. / option.target_bpm - 15.).clamp(1., 180.),
                        )
                }),
            },
        );
    }
    event(
        app,
        "mix.updated",
        json!({"operation_id":id,"state":"preparing"}),
    );
    if automatic {
        let runtime = app.clone();
        let mut request = request.clone();
        // Preparation has an app-owned deadline; the caller only waits for acceptance.
        request.timeout_ms = ((option.launch_in_seconds + 30.).clamp(10., 180.) * 1000.) as u64;
        let operation_id = id.clone();
        let mut summary = mix_summary(
            c.mix_prepared
                .lock()
                .map_err(failure)?
                .get(&operation_id)
                .unwrap(),
        );
        summary["event_sequence"] = json!(c.event_sequence.load(Ordering::Acquire));
        let work = MixPreparation {
            id,
            option,
            token,
            initial,
            matches,
            automatic,
        };
        std::thread::spawn(move || {
            let control = runtime.state::<Control>();
            let result = {
                let _preparing = control
                    .mix_preparing
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                finish_mix_preparation(&runtime, &request, work)
            };
            if result.is_ok() {
                let _submission = control
                    .scheduled_submission
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let arm = Request::new(
                    "mix.arm",
                    json!({"prepared_id":operation_id,"review_confirmed":true}),
                );
                if let Err(error) = arm_mix(&runtime, &arm) {
                    fail_mix_operation(&runtime, &operation_id, error);
                }
            }
            if let Err(error) = persist_control(&runtime) {
                eprintln!("Mix receipt: {error}");
            }
        });
        return Ok(("accepted", summary));
    }
    finish_mix_preparation(
        app,
        request,
        MixPreparation {
            id,
            option,
            token,
            initial,
            matches,
            automatic,
        },
    )
}
struct MixPreparation {
    id: String,
    option: crate::next_move::MoveOption,
    token: Arc<std::sync::atomic::AtomicBool>,
    initial: deck_engine::EngineSnapshot,
    matches: bool,
    automatic: bool,
}
fn finish_mix_preparation(
    app: &tauri::AppHandle,
    request: &Request,
    work: MixPreparation,
) -> Result<(&'static str, Value)> {
    let MixPreparation {
        id,
        mut option,
        token,
        initial,
        matches,
        automatic,
    } = work;
    let c = app.state::<Control>();
    let state = app.state::<AppState>();
    let target = option.target_deck;
    let result = (|| -> Result<(crate::next_move::MoveOption, deck_engine::EngineSnapshot)> {
        check_mix_cancelled(&token)?;
        if initial.decks[target].track.is_none() && initial.decks[target].channel_fader > 0.001 {
            // Empty channels may start open; isolate before any provider source can arrive.
            let command = EngineCommand::SetChannelFader {
                deck: target,
                value: 0.,
            };
            let mut isolate = Request::new("engine.command", serialized(&command)?);
            isolate.expected_load_revision = Some(initial.load_revisions[target]);
            isolate.timeout_ms = request.timeout_ms;
            apply_with_control_guard(app, &isolate, command, Some(initial.control_revision))?;
        }
        if !matches {
            let args = if option.spotify {
                json!({"deck":target,"uri":option.source})
            } else if option.source.starts_with("local:") {
                json!({"deck":target,"track":option.source})
            } else {
                json!({"deck":target,"path":option.source})
            };
            let mut load = Request::new("deck.load", args);
            load.request_id = format!("{id}:load");
            load.expected_load_revision = Some(initial.load_revisions[target]);
            load.timeout_ms = request.timeout_ms;
            dispatch(app, &load)?;
        }
        check_mix_cancelled(&token)?;
        let loaded = state
            .engine
            .scheduled_snapshot()
            .map_err(scheduled_failure)?;
        mix_reference_identity_guard(&option, &loaded)?;
        if loaded.deck_control_revisions[option.reference_deck]
            != initial.deck_control_revisions[option.reference_deck]
        {
            return Err(mix_error(
                "manual_takeover",
                "Reference deck controls changed during preparation",
            ));
        }
        if !loaded.decks[target]
            .track
            .as_ref()
            .is_some_and(|track| mix_source_matches(app, track, &option.source))
        {
            return Err(mix_error(
                "source_changed",
                "Loaded target source differs from selected option",
            ));
        }
        if !loaded.decks[target].beatgrid_valid {
            if !optional::<bool>(&request.args, "allow_analysis")?.unwrap_or(false) {
                return Err(mix_error("analysis_required","Selected source needs a usable beatgrid; enable analysis or analyze the track first"));
            }
            let mut analyze = Request::new("deck.analyze", json!({"deck":target}));
            analyze.expected_load_revision = Some(loaded.load_revisions[target]);
            analyze.timeout_ms = request.timeout_ms;
            dispatch(app, &analyze)?;
        }
        let ready = state
            .engine
            .scheduled_snapshot()
            .map_err(scheduled_failure)?;
        mix_reference_identity_guard(&option, &ready)?;
        option = match crate::next_move::revalidate_prepared_window(&ready, &option, false) {
            Ok(refreshed) => refreshed,
            Err(reason)
                if automatic
                    && optional::<bool>(&request.args, "allow_fallback")?.unwrap_or(true) =>
            {
                let fallback = crate::next_move::revalidate_prepared_window(&ready, &option, true)
                    .map_err(|e| mix_error("no_feasible_fallback", &format!("{reason}; {e}")))?;
                let mut stored = c.mix_prepared.lock().map_err(failure)?;
                let p = stored
                    .get_mut(&id)
                    .ok_or_else(|| mix_error("not_found", "Move disappeared"))?;
                p.fallback_used = true;
                p.option = fallback.clone();
                event(app, "mix.updated", mix_summary(p));
                fallback
            }
            Err(reason) => return Err(mix_error("not_ready", &reason)),
        };
        let mut expected_control_revision = ready.control_revision;
        for command in mix_prepare_controls(&option) {
            check_mix_cancelled(&token)?;
            let current = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            mix_reference_guard(&option, &current)?;
            if current.control_revision != expected_control_revision
                || current.deck_control_revisions[option.reference_deck]
                    != initial.deck_control_revisions[option.reference_deck]
                || current.load_revisions[target] != ready.load_revisions[target]
                || current.decks[target].playing
            {
                return Err(mix_error(
                    "manual_takeover",
                    "Deck state changed during preparation",
                ));
            }
            if matches!(command, EngineCommand::SetPlaybackBpm { .. }) {
                let local = current.decks[target].local_bpm.ok_or_else(|| {
                    mix_error("grid_unknown", "Incoming entry tempo is unavailable")
                })?;
                let correction = 100. * (option.target_bpm / local - 1.);
                if !correction.is_finite() || correction.abs() > option.max_stretch_percent + 0.001
                {
                    return Err(mix_error(
                        "tempo_policy",
                        "Incoming entry exceeds the selected recipe's natural-speed limit",
                    ));
                }
            }
            let mut native = Request::new("engine.command", serialized(&command)?);
            native.expected_load_revision = Some(ready.load_revisions[target]);
            native.timeout_ms = request.timeout_ms;
            apply_with_control_guard(app, &native, command, Some(expected_control_revision))?;
            expected_control_revision = expected_control_revision.wrapping_add(1);
            if state.engine.transport_snapshot().control_revision != expected_control_revision {
                return Err(mix_error(
                    "manual_takeover",
                    "Controls changed while preparation was applying",
                ));
            }
        }
        check_mix_cancelled(&token)?;
        let refreshed = crate::next_move::refresh_prepared_option(app, &option)
            .map_err(|e| mix_error("not_ready", &e))?;
        let snapshot = state
            .engine
            .scheduled_snapshot()
            .map_err(scheduled_failure)?;
        if snapshot.control_revision != expected_control_revision {
            return Err(mix_error(
                "manual_takeover",
                "Controls changed before preparation completed",
            ));
        }
        mix_reference_guard(&refreshed, &snapshot)?;
        validate_mix_readiness(&refreshed, &snapshot)?;
        // Validation uses the exact same compiler as arm, without reserving a native queue.
        let compiled = compile_mix_recipe(&id, &refreshed, &snapshot)?;
        let (schedule, commands) = scheduled_commands_at(&compiled, Some(&snapshot))?;
        state
            .engine
            .validate_scheduled_batch(&schedule, commands)
            .map_err(scheduled_failure)?;
        Ok((refreshed, snapshot))
    })();
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    let prepared = stored
        .get_mut(&id)
        .ok_or_else(|| mix_error("not_found", "Mix preparation disappeared"))?;
    if prepared.state == "failed" && token.load(Ordering::Acquire) {
        return Err(prepared
            .error
            .clone()
            .unwrap_or_else(|| mix_error("preparation_deadline", "Preparation deadline passed")));
    }
    let result = if token.load(Ordering::Acquire) {
        Err(mix_error("cancelled", "Move was cancelled"))
    } else {
        result
    };
    match result {
        Ok((option, snapshot)) => {
            prepared.option = option;
            prepared.snapshot = Some(snapshot);
            prepared.state = "ready".into();
            prepared.preparation_deadline = None;
        }
        Err(error) => {
            prepared.state = if error.code == "cancelled" {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            prepared.error = Some(error.clone());
            event(app, "mix.updated", mix_summary(prepared));
            return Err(error.with_detail("operation_id", id));
        }
    }
    let summary = mix_summary(prepared);
    drop(stored);
    event(app, "mix.updated", summary.clone());
    Ok(("ready", summary))
}
fn validate_mix_readiness(
    option: &crate::next_move::MoveOption,
    s: &deck_engine::EngineSnapshot,
) -> Result<()> {
    let outgoing = &s.decks[option.reference_deck];
    let incoming = &s.decks[option.target_deck];
    if incoming.playing
        || incoming.preparation_pending
        || incoming.sync_rate_limited
        || !incoming.beatgrid_valid
        || incoming.channel_fader > 0.001
        || (incoming.position_seconds - option.entry_seconds).abs() > 0.001
        || incoming.loop_start_seconds.is_some()
        || outgoing.loop_start_seconds.is_some()
    {
        return Err(mix_error(
            "not_ready",
            "Prepared source must be paused, isolated, grid-ready and outside a loop",
        ));
    }
    if s.crossfader.abs() > 0.05
        || s.master_gain <= 0.001
        || outgoing.channel_fader < 0.99
        || (outgoing.eq_low - 1.).abs() > 0.01
        || (outgoing.eq_mid - 1.).abs() > 0.01
        || (outgoing.eq_high - 1.).abs() > 0.01
        || outgoing.filter.abs() > 0.01
    {
        return Err(mix_error("routing_review","Center the crossfader and restore the reference channel to full fader/EQ with neutral filter before arming this recipe"));
    }
    if ((outgoing.actual_playback_rate - 1.) * 100.).abs() > option.max_stretch_percent + 0.001
        || incoming
            .local_bpm
            .is_none_or(|bpm| (bpm * incoming.playback_rate - option.target_bpm).abs() > 0.05)
        || incoming.tempo_percent.abs() > option.max_stretch_percent + 0.001
        || !incoming.actual_playback_rate.is_finite()
        || ((incoming.playback_rate - 1.) * 100.).abs() > option.max_stretch_percent + 0.001
    {
        return Err(mix_error(
            "tempo_policy",
            "Prepared tempo exceeds the recipe's rate policy",
        ));
    }
    if !matches!(
        option.technique.as_str(),
        "eq_blend" | "filter_blend" | "short_exchange" | "echo_out" | "reverb_exit"
    ) {
        return Err(mix_error(
            "technique_unavailable",
            "This technique has no validated native recipe",
        ));
    }
    Ok(())
}

fn compile_mix_recipe(
    id: &str,
    o: &crate::next_move::MoveOption,
    s: &deck_engine::EngineSnapshot,
) -> Result<Request> {
    let a = o.reference_deck;
    let b = o.target_deck;
    let launch = o.launch_event;
    let audible = launch + 2;
    let handover = o.handover_event;
    let clear = o.clear_event;
    if handover <= audible + 1 || clear <= handover + 1 {
        return Err(mix_error(
            "invalid_timing",
            "Recipe needs advancement and audible overlap before clearance",
        ));
    }
    // Reserve two beat events for endpoint verification before pausing the old carrier.
    deck_engine::event_seconds(s, a, clear + 2).map_err(scheduled_failure)?;
    let require = json!({"deck":b,"ready":true,"playing":true,"advanced_from_seconds":s.decks[b].position_seconds,"min_advanced_beats":1.,"phase_tolerance_beats":0.10});
    let mut steps = vec![
        json!({"action_id":"clock-master","command":"engine.command","args":{"type":"set_master","deck":a},"expected_load_revision":s.load_revisions[a],"at_event":launch}),
        json!({"action_id":"sync-limits","command":"engine.command","args":{"type":"set_sync_rate_limits","deck":b,"limits":{"min_rate":(1.-o.max_stretch_percent/100.).max(0.1),"max_rate":(1.+o.max_stretch_percent/100.).min(2.),"max_slew_per_second":0.01}},"expected_load_revision":s.load_revisions[b],"at_event":launch}),
        json!({"action_id":"follow-clock","command":"engine.command","args":{"type":"set_sync","deck":b,"enabled":true},"expected_load_revision":s.load_revisions[b],"at_event":launch}),
        json!({"action_id":"launch","command":"engine.command","args":{"type":"play","deck":b},"expected_load_revision":s.load_revisions[b],"at_event":launch}),
    ];
    let mut ramp = |label: &str, at: i64, end: i64, deck: usize, target: &str, value: f64| {
        steps.push(json!({"action_id":label,"command":"automation.ramp","args":{"target":{"type":target,"deck":deck},"value":value,"clock":a,"beats":end-at},"expected_load_revision":s.load_revisions[deck],"at_event":at,"require":require}));
    };
    if o.technique == "eq_blend" {
        ramp("entrance", audible, handover, b, "channel_fader", 0.7);
        ramp("bass-out", handover, clear, a, "eq_low", 0.);
        ramp("bass-in", handover, clear, b, "eq_low", 1.);
        ramp("carrier-open", handover, clear, b, "channel_fader", 1.);
        ramp("outgoing-clear", handover, clear, a, "channel_fader", 0.);
    } else if o.technique == "filter_blend" {
        ramp("entrance", audible, handover, b, "channel_fader", 0.7);
        ramp("incoming-filter", audible, handover, b, "filter", 0.);
        ramp("outgoing-filter", handover, clear, a, "filter", 0.65);
        ramp("carrier-open", handover, clear, b, "channel_fader", 1.);
        ramp("outgoing-clear", handover, clear, a, "channel_fader", 0.);
    } else if o.technique == "echo_out" || o.technique == "reverb_exit" {
        let effect = if o.technique == "echo_out" {
            "echo"
        } else {
            "reverb"
        };
        ramp("tail-send", audible, handover, a, effect, 0.55);
        ramp("entrance", audible, handover, b, "channel_fader", 1.);
        ramp("outgoing-clear", handover, clear, a, "channel_fader", 0.);
        ramp("close-send", handover, clear, a, effect, 0.);
    } else {
        ramp("entrance", audible, handover, b, "channel_fader", 1.);
        ramp("outgoing-clear", handover, clear, a, "channel_fader", 0.);
    }
    steps.push(json!({"action_id":"restore-tone","command":"engine.command","args":{"type":"set_eq","deck":b,"low":1.,"mid":1.,"high":1.},"expected_load_revision":s.load_revisions[b],"at_event":clear,"require":require}));
    steps.push(json!({"action_id":"restore-filter","command":"engine.command","args":{"type":"set_filter","deck":b,"amount":0.},"expected_load_revision":s.load_revisions[b],"at_event":clear,"require":require}));
    steps.push(json!({"action_id":"restore-fx","command":"engine.command","args":{"type":"set_fx","deck":b,"echo":0.,"reverb":0.},"expected_load_revision":s.load_revisions[b],"at_event":clear,"require":require}));
    steps.sort_by_key(|step| step["at_event"].as_i64().unwrap_or(0));
    Ok(Request::new(
        "batch.run",
        json!({"schedule":{
        "operation_id":id,"clock":a,"completion_carrier":b,"expected_load_revisions":s.load_revisions,
        "expected_position_revisions":s.decks.each_ref().map(|d|d.position_revision),
        "expected_grid_revisions":s.decks.each_ref().map(|d|d.grid_revision),"expected_engine_epoch":s.engine_epoch,
        "expected_control_revision":s.control_revision,"arm_before_event":launch,"completion_event":clear+2,"pause_outgoing":true,
        "intent":{"planner_version":crate::next_move::PLANNER_VERSION,"option_id":o.id,"technique":o.technique,"target_bpm":o.target_bpm,"max_stretch_percent":o.max_stretch_percent},
        "recovery":{"carrier":a,"seconds":0.3,"controls":[
            {"type":"set_channel_fader","deck":a,"value":1.},{"type":"set_channel_fader","deck":b,"value":0.},
            {"type":"set_eq","deck":a,"low":1.,"mid":1.,"high":1.},{"type":"set_filter","deck":a,"amount":0.},{"type":"set_fx","deck":a,"echo":0.,"reverb":0.}]}
    },"steps":steps}),
    ))
}
fn uncertain_mix_job(error: &ControlError) -> Option<&str> {
    (error.code == "timeout")
        .then(|| error.details["job_id"].as_str())
        .flatten()
}
fn mix_native_operation_id(prepared: &PreparedMix) -> String {
    if prepared.recovery_attempts == 0 {
        prepared.id.clone()
    } else {
        format!("{}:retry-{}", prepared.id, prepared.recovery_attempts)
    }
}
fn arm_mix(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let id: String = arg(&request.args, "prepared_id")?;
    let c = app.state::<Control>();
    let _preparation = c
        .mix_preparing
        .try_lock()
        .map_err(|_| mix_error("busy", "Mix preparation is still in progress"))?;
    let prepared = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .get(&id)
        .cloned()
        .ok_or_else(|| mix_error("not_found", "Prepared mix not found"))?;
    check_mix_cancelled(&prepared.cancelled)?;
    if let Some(job_id) = prepared.job_id {
        return Ok((
            "accepted",
            json!({"prepared_id":id,"operation_id":id,"job_id":job_id,"state":prepared.state,"replayed":false}),
        ));
    }
    if prepared.state != "ready" {
        return Err(mix_error("not_ready", "Mix is not ready to arm"));
    }
    check_mix_cancelled(&prepared.cancelled)?;
    if prepared.option.review_required
        && !optional::<bool>(&request.args, "review_confirmed")?.unwrap_or(false)
    {
        return Err(mix_error("review_required","Review or audition the prepared source, then confirm uncertain passage and vocal evidence before arming"));
    }
    let engine = &app.state::<AppState>().engine;
    let snapshot = engine.scheduled_snapshot().map_err(scheduled_failure)?;
    let expected = prepared
        .snapshot
        .as_ref()
        .ok_or_else(|| mix_error("not_ready", "Prepared state is missing"))?;
    let result = (|| -> Result<(&'static str, Value)> {
        if mix_ownership_changed(expected, &snapshot) {
            return Err(mix_error("stale_preparation","Sources, grids, positions or controls changed after preparation; prepare a fresh option"));
        }
        mix_reference_guard(&prepared.option, &snapshot)?;
        validate_mix_readiness(&prepared.option, &snapshot)?;
        crate::next_move::revalidate_prepared_window(&snapshot, &prepared.option, false)
            .map_err(|e| mix_error("stale_preparation", &e))?;
        // A fresh recovery plan needs a fresh native idempotency key, while
        // callers continue following the original high-level mix operation.
        let mut batch = compile_mix_recipe(
            &mix_native_operation_id(&prepared),
            &prepared.option,
            &snapshot,
        )?;
        batch.timeout_ms = request.timeout_ms;
        {
            let mut stored = c.mix_prepared.lock().map_err(failure)?;
            let current = stored
                .get_mut(&id)
                .ok_or_else(|| mix_error("not_found", "Prepared mix not found"))?;
            check_mix_cancelled(&current.cancelled)?;
            if current.state != "ready" {
                return Err(mix_error("not_ready", "Preparation changed before arming"));
            }
            // Reserve ownership before native submission can advance the decks.
            current.state = "arming".into();
            event(app, "mix.updated", mix_summary(current));
        }
        start_scheduled_batch(app, &batch)
    })();
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    let p = stored
        .get_mut(&id)
        .ok_or_else(|| mix_error("not_found", "Prepared mix not found"))?;
    match result {
        Ok((stage, mut receipt)) => {
            p.state = "armed".into();
            p.job_id = receipt["job_id"].as_str().map(str::to_owned);
            receipt["prepared_id"] = json!(id);
            receipt["operation_id"] = json!(id);
            receipt["state"] = json!("armed");
            event(app, "mix.updated", mix_summary(p));
            let job_id = p.job_id.clone();
            drop(stored);
            if let Some(job_id) = job_id {
                register_mix_session_job(app, &job_id)?;
            }
            Ok((stage, receipt))
        }
        Err(e) => {
            if let Some(job_id) = uncertain_mix_job(&e) {
                if c.jobs
                    .lock()
                    .map_err(failure)?
                    .get(job_id)
                    .is_some_and(|j| j.native_id.is_some())
                {
                    p.job_id = Some(job_id.to_owned());
                    p.state = "arming".into();
                    p.error = Some(e);
                    p.arm_confirmation_deadline = Some(Instant::now() + Duration::from_secs(5));
                    let summary = mix_summary(p);
                    event(app, "mix.updated", summary.clone());
                    let native_job = p.job_id.clone().unwrap();
                    drop(stored);
                    register_mix_session_job(app, &native_job)?;
                    return Ok(("accepted", summary));
                }
            }
            p.state = "invalidated".into();
            p.error = Some(e.clone());
            event(app, "mix.updated", mix_summary(p));
            Err(e)
        }
    }
}
fn mix_status(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let id: String = arg(&request.args, "operation_id")?;
    let c = app.state::<Control>();
    refresh_scheduled_jobs(app)?;
    poll_mix_operations(app)?;
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    if !stored.contains_key(&id) {
        drop(stored);
        let jobs = c.jobs.lock().map_err(failure)?;
        let (job_id, job) = jobs
            .iter()
            .find(|(_, job)| job.status["operation_id"] == id)
            .ok_or_else(|| {
                mix_error(
                    "not_found",
                    "Mix operation not found; unarmed preparation expires after app restart",
                )
            })?;
        return Ok((
            "applied",
            json!({"operation_id":id,"prepared_id":id,"job_id":job_id,"state":job.status["state"],"ready":false,"restored":true,"execution":job_status(&job.status,true)}),
        ));
    }
    let p = stored
        .get_mut(&id)
        .ok_or_else(|| mix_error("not_found", "Mix operation not found"))?;
    let mut summary = mix_summary(p);
    if let Some(job_id) = &p.job_id {
        if let Some(job) = c.jobs.lock().map_err(failure)?.get(job_id) {
            summary["execution"] = job_status(&job.status, true);
            summary["state"] = json!(p.state);
        }
    } else if p.state == "ready" {
        let current = app.state::<AppState>().engine.transport_snapshot();
        if mix_transport_invalidates(p, &current) {
            p.state = "invalidated".into();
            summary["state"] = json!(p.state);
            summary["ready"] = json!(false);
        }
    }
    summary["event_sequence"] = json!(c.event_sequence.load(Ordering::Acquire));
    Ok(("applied", summary))
}
fn cancel_mix(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let id: String = arg(&request.args, "operation_id")?;
    let c = app.state::<Control>();
    let previewing = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .get(&id)
        .is_some_and(|p| p.state == "previewing");
    if previewing {
        let mut stop = Request::new("mix.preview", json!({"prepared_id":id,"enabled":false}));
        stop.timeout_ms = request.timeout_ms;
        if let Err(error) = preview_mix(app, &stop) {
            if error.code == "busy" {
                return Err(error);
            }
        }
    }
    let _submission = c.scheduled_submission.lock().map_err(failure)?;
    let job_id = {
        let mut stored = c.mix_prepared.lock().map_err(failure)?;
        let p = stored
            .get_mut(&id)
            .ok_or_else(|| mix_error("not_found", "Mix operation not found"))?;
        if mix_terminal(&p.state) {
            return Ok(("applied", mix_summary(p)));
        }
        p.cancelled.store(true, Ordering::Release);
        p.state = "cancelled".into();
        p.job_id.clone()
    };
    for deck in 0..2 {
        app.state::<AppState>()
            .source_jobs
            .cancel_owner(deck, &format!("{id}:load"));
    }
    if let Some(job_id) = job_id {
        dispatch(app, &Request::new("jobs.cancel", json!({"id":job_id})))?;
    }
    event(
        app,
        "mix.updated",
        json!({"operation_id":id,"state":"cancelled"}),
    );
    Ok((
        "applied",
        json!({"operation_id":id,"state":"cancelled","playback_paused":false}),
    ))
}

fn restored_mix_operation(record: &Value, native_state: Option<&str>) -> Result<PreparedMix> {
    let previous = record["state"].as_str().unwrap_or("interrupted");
    let state = if mix_terminal(previous) {
        previous
    } else {
        native_state
            .filter(|state| mix_terminal(state))
            .unwrap_or("interrupted")
    };
    let error = if state == "interrupted" {
        Some(mix_error(
            "interrupted",
            "Application restarted; move was not replayed",
        ))
    } else {
        serde_json::from_value(record["error"].clone()).map_err(failure)?
    };
    Ok(PreparedMix {
        id: arg(record, "operation_id")?,
        option: arg(record, "option")?,
        state: state.into(),
        snapshot: None,
        job_id: optional(record, "job_id")?,
        error,
        cancelled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        preview_cue: None,
        preview_deadline: None,
        automatic: record["automatic"] == true,
        fallback_used: record["fallback_used"] == true,
        preparation_deadline: None,
        restored: true,
        arm_confirmation_deadline: None,
        recovery_attempts: 0,
        takeover_revision: 0,
    })
}
fn restore_mix_operations(app: &tauri::AppHandle, records: &Value) -> Result<()> {
    let c = app.state::<Control>();
    let states: HashMap<String, String> = c
        .jobs
        .lock()
        .map_err(failure)?
        .iter()
        .filter_map(|(id, j)| j.status["state"].as_str().map(|s| (id.clone(), s.into())))
        .collect();
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    for record in records.as_array().into_iter().flatten().take(64) {
        let native_state = record["job_id"]
            .as_str()
            .and_then(|id| states.get(id))
            .map(String::as_str);
        let prepared = restored_mix_operation(record, native_state)?;
        event(app, "mix.updated", mix_summary(&prepared));
        stored.insert(prepared.id.clone(), prepared);
    }
    Ok(())
}

fn fail_mix_operation(app: &tauri::AppHandle, id: &str, error: ControlError) {
    let mut retry = false;
    if let Ok(mut stored) = app.state::<Control>().mix_prepared.lock() {
        if let Some(p) = stored.get_mut(id) {
            if can_retry_mix(
                p,
                &app.state::<AppState>().engine.transport_snapshot(),
                app.state::<Control>()
                    .mix_takeover_revision
                    .load(Ordering::Acquire),
                error.details["native"]["reason"]
                    .as_str()
                    .unwrap_or(&error.code),
            ) {
                p.recovery_attempts += 1;
                p.fallback_used = true;
                p.state = "recovering".into();
                p.job_id = None;
                retry = true;
            } else if p.cancelled.load(Ordering::Acquire) {
                p.state = "cancelled".into();
            } else {
                p.state = "failed".into();
                p.error = Some(error);
            }
            event(app, "mix.updated", mix_summary(p));
        }
    }
    if retry {
        let runtime = app.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            if let Err(error) = retry_mix_operation(&runtime, &id) {
                fail_mix_operation(&runtime, &id, error);
            }
        });
    }
}

/// Observe native state changes; never place fader timing on this observer thread.
fn can_retry_mix(
    p: &PreparedMix,
    s: &deck_engine::EngineSnapshot,
    takeover: u64,
    reason: &str,
) -> bool {
    let a = p.option.reference_deck;
    p.automatic
        && p.recovery_attempts == 0
        && !p.cancelled.load(Ordering::Acquire)
        && takeover == p.takeover_revision
        && ([
            "grid_changed",
            "deadline_expired",
            "prerequisite_failed",
            "endpoint_failed",
        ]
        .contains(&reason)
            || (matches!(reason, "stale_preparation" | "stale_option")
                && p.snapshot.as_ref().is_some_and(|old| {
                    old.decks
                        .iter()
                        .zip(&s.decks)
                        .any(|(before, after)| before.grid_revision != after.grid_revision)
                })))
        && s.running
        && s.engine_epoch == p.option.engine_epoch
        && s.decks[a].playing
        && p.snapshot.as_ref().is_some_and(|old| {
            old.load_revisions == s.load_revisions
                && old.deck_control_revisions == s.deck_control_revisions
        })
        && s.decks[a].position_revision == p.option.reference_position_revision
        && p.snapshot.as_ref().is_some_and(|old| {
            (crate::next_move::useful_end_seconds(&old.decks[a]) - s.decks[a].position_seconds)
                / s.decks[a].actual_playback_rate
                > 20.
        })
}

fn retry_mix_operation(app: &tauri::AppHandle, id: &str) -> Result<()> {
    let c = app.state::<Control>();
    let state = app.state::<AppState>();
    let result = {
        let _preparing = c.mix_preparing.lock().map_err(failure)?;
        let p = c
            .mix_prepared
            .lock()
            .map_err(failure)?
            .get(id)
            .cloned()
            .ok_or_else(|| mix_error("not_found", "Retry receipt disappeared"))?;
        let _submission = c.scheduled_submission.lock().map_err(failure)?;
        let mut current = state
            .engine
            .scheduled_snapshot()
            .map_err(scheduled_failure)?;
        if p.cancelled.load(Ordering::Acquire)
            || c.mix_takeover_revision.load(Ordering::Acquire) != p.takeover_revision
            || p.snapshot.as_ref().is_none_or(|old| {
                old.load_revisions != current.load_revisions
                    || old.deck_control_revisions != current.deck_control_revisions
            })
            || !current.decks[p.option.reference_deck].playing
        {
            return Err(mix_error(
                "manual_takeover",
                "Recovery yielded to changed sources or controls",
            ));
        }
        let a = p.option.reference_deck;
        let b = p.option.target_deck;
        let sources = current.load_revisions;
        for command in [
            EngineCommand::SetChannelFader { deck: a, value: 1. },
            EngineCommand::SetChannelFader { deck: b, value: 0. },
            EngineCommand::SetEq {
                deck: a,
                low: 1.,
                mid: 1.,
                high: 1.,
            },
            EngineCommand::SetFilter {
                deck: a,
                amount: 0.,
            },
            EngineCommand::SetFx {
                deck: a,
                echo: 0.,
                reverb: 0.,
            },
            EngineCommand::SetSync {
                deck: b,
                enabled: false,
            },
            EngineCommand::Pause { deck: b },
        ] {
            check_mix_cancelled(&p.cancelled)?;
            if current.load_revisions != sources
                || c.mix_takeover_revision.load(Ordering::Acquire) != p.takeover_revision
            {
                return Err(mix_error(
                    "manual_takeover",
                    "Sources changed during recovery",
                ));
            }
            let expected_control_revision = current.control_revision.wrapping_add(1);
            let mut request = Request::new("engine.command", serialized(&command)?);
            request.expected_load_revision = command.deck().map(|deck| sources[deck]);
            apply_with_control_guard(app, &request, command, Some(current.control_revision))?;
            current = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            if current.control_revision != expected_control_revision {
                return Err(mix_error(
                    "manual_takeover",
                    "Controls changed during recovery",
                ));
            }
        }
        drop(_submission);
        let option = crate::next_move::recovery_option(&current, &p.option)
            .map_err(|reason| mix_error("no_feasible_fallback", &reason))?;
        {
            let mut stored = c.mix_prepared.lock().map_err(failure)?;
            let active = stored
                .get_mut(id)
                .ok_or_else(|| mix_error("not_found", "Retry receipt disappeared"))?;
            check_mix_cancelled(&active.cancelled)?;
            active.state = "preparing".into();
            active.option = option.clone();
            active.job_id = None;
            active.error = None;
            active.preparation_deadline = Some(Instant::now() + Duration::from_secs(30));
            event(app, "mix.updated", mix_summary(active));
        }
        let mut request = Request::new("mix.execute", json!({"allow_fallback":true}));
        request.timeout_ms = 30_000;
        finish_mix_preparation(
            app,
            &request,
            MixPreparation {
                id: id.into(),
                option,
                token: p.cancelled,
                initial: current,
                matches: true,
                automatic: true,
            },
        )
    };
    result?;
    let _submission = c.scheduled_submission.lock().map_err(failure)?;
    arm_mix(
        app,
        &Request::new("mix.arm", json!({"prepared_id":id,"review_confirmed":true})),
    )?;
    Ok(())
}

fn poll_mix_operations(app: &tauri::AppHandle) -> Result<()> {
    let c = app.state::<Control>();
    let wanted: Vec<String> = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .values()
        .filter(|p| !mix_terminal(&p.state))
        .filter_map(|p| p.job_id.clone())
        .collect();
    let jobs: HashMap<String, Value> = {
        let jobs = c.jobs.lock().map_err(failure)?;
        wanted
            .iter()
            .filter_map(|id| {
                jobs.get(id).map(|job| {
                    (
                        id.clone(),
                        json!({"state":job.status["state"],"reason":job.status["reason"],"native_id":job.native_id}),
                    )
                })
            })
            .collect()
    };
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    // Read after locking preparations: a newly ready source must not be judged
    // against transport captured before its final controls were applied.
    let current = app.state::<AppState>().engine.transport_snapshot();
    let mut unconfirmed = Vec::new();
    let mut retries = Vec::new();
    for p in stored.values_mut() {
        if mix_terminal(&p.state) {
            continue;
        }
        if p.state == "preparing"
            && p.preparation_deadline
                .is_some_and(|at| Instant::now() >= at)
        {
            p.cancelled.store(true, Ordering::Release);
            p.state = "failed".into();
            p.error = Some(mix_error("preparation_deadline", "Source preparation exceeded the continuity deadline; the healthy carrier is preserved"));
            for deck in 0..2 {
                app.state::<AppState>()
                    .source_jobs
                    .cancel_owner(deck, &format!("{}:load", p.id));
            }
            event(app, "mix.updated", mix_summary(p));
            continue;
        }
        let Some(job) = p.job_id.as_ref().and_then(|id| jobs.get(id)) else {
            if p.state == "ready" && mix_transport_invalidates(p, &current) {
                p.state = "invalidated".into();
                event(app, "mix.updated", mix_summary(p));
            }
            continue;
        };
        if let Some(deadline) = p.arm_confirmation_deadline {
            if job["native_id"].as_u64() == Some(current.scheduled_batch.id) {
                p.arm_confirmation_deadline = None;
                p.error = None;
            } else if Instant::now() >= deadline
                || !current.running
                || current.engine_epoch != p.option.engine_epoch
            {
                if let Some(native) = job["native_id"].as_u64() {
                    // This cancellation is ordered after the uncertain arm and scoped to its ID.
                    app.state::<AppState>()
                        .engine
                        .cancel_scheduled_batch(native, true)
                        .map_err(failure)?;
                }
                p.state = "failed".into();
                p.error = Some(mix_error(
                    "arm_unconfirmed",
                    "Native arm was not acknowledged; owned pending work was cancelled",
                ));
                unconfirmed.push(p.job_id.clone().unwrap());
                event(app, "mix.updated", mix_summary(p));
                continue;
            } else {
                continue;
            }
        }
        let state = match job["state"].as_str().unwrap_or("interrupted") {
            "running" | "executing" => "transitioning",
            state => state,
        };
        if state == p.state {
            continue;
        }
        if matches!(state, "failed" | "recovered" | "yielded")
            && can_retry_mix(
                p,
                &current,
                c.mix_takeover_revision.load(Ordering::Acquire),
                job["reason"].as_str().unwrap_or(""),
            )
        {
            p.recovery_attempts += 1;
            p.fallback_used = true;
            p.state = "recovering".into();
            p.job_id = None;
            retries.push(p.id.clone());
            event(app, "mix.updated", mix_summary(p));
            continue;
        }
        p.state = state.into();
        if matches!(state, "failed" | "recovered" | "interrupted" | "yielded") {
            p.error = Some(mix_error(
                state,
                job["reason"].as_str().unwrap_or("Native move interrupted"),
            ));
        }
        event(app, "mix.updated", mix_summary(p));
    }
    drop(stored);
    for id in retries {
        let runtime = app.clone();
        std::thread::spawn(move || {
            if let Err(error) = retry_mix_operation(&runtime, &id) {
                fail_mix_operation(&runtime, &id, error);
            }
        });
    }
    if !unconfirmed.is_empty() {
        let mut jobs = c.jobs.lock().map_err(failure)?;
        for id in unconfirmed {
            if let Some(job) = jobs.get_mut(&id) {
                job.cancel.store(true, Ordering::Release);
                job.status["state"] = json!("failed");
                job.status["reason"] = json!("arm_unconfirmed");
                job.native_id = None;
            }
        }
        drop(jobs);
        persist_control(app)?;
    }
    Ok(())
}

#[cfg(test)]
mod mix_execution_tests {
    use super::*;
    fn fixture(technique: &str) -> (crate::next_move::MoveOption, deck_engine::EngineSnapshot) {
        let track = deck_engine::TrackAnalysis {
            path: "/test/carrier.wav".into(),
            duration_seconds: 300.,
            bpm: Some(120.),
            bpm_source: Some("manual".into()),
            beatgrid_source: Some("manual".into()),
            waveform_bands: Some(audio_analysis::WaveformBands {
                low: vec![0.1; 16],
                mid: vec![0.1; 16],
                high: vec![0.1; 16],
            }),
            ..Default::default()
        };
        let mut a = crate::next_move::analysis_deck(&track).unwrap();
        a.playing = true;
        a.channel_fader = 1.;
        a.is_master = true;
        a.local_bpm = Some(120.);
        a.playback_bpm = Some(120.);
        a.actual_playback_bpm = Some(120.);
        let mut b = a.clone();
        b.playing = false;
        b.channel_fader = 0.;
        b.is_master = false;
        b.track.as_mut().unwrap().path = "/test/next.wav".into();
        if technique == "eq_blend" {
            b.eq_low = 0.;
        } else if technique == "filter_blend" {
            b.filter = 0.65;
        }
        let s:deck_engine::EngineSnapshot=serde_json::from_value(json!({"decks":[a,b],"load_revisions":[1,2],"sample_rate":48000,"output_channels":2,"buffer_frames":512,"running":true,"rendered_frames":0,"master_peak":[0.,0.],"cue_peak":[0.,0.],"underruns":0,"master_gain":1.,"crossfader":0.,"cue_gain":1.,"cue_mix":1.,"sampler_gain":1.,"sampler_positions_seconds":vec![Value::Null;16],"active_ramps":vec![false;deck_engine::CONTROL_TARGET_COUNT]})).unwrap();
        let p = json!({"start_seconds":0.,"end_seconds":32.,"low":0.1,"mid":0.1,"high":0.1,"level":0.2,"pulse_spread":0.,"local_bpm":120.});
        let o=serde_json::from_value(json!({"id":"fixture","source":"/test/next.wav","title":"Next","artist":"Artist","spotify":false,"reference_deck":0,"target_deck":1,"technique":technique,"overlap_bars":8,"entry_seconds":0.,"launch_seconds":10.,"audible_seconds":11.,"handover_seconds":18.,"clear_seconds":26.,"launch_event":20,"handover_event":36,"clear_event":52,"target_bpm":120.,"native_bpm":120.,"tempo_percent":0.,"max_stretch_percent":2.,"reference_source":"/test/carrier.wav","reference_load_revision":1,"reference_grid_revision":0,"reference_position_revision":0,"reference_control_revision":0,"engine_epoch":0,"review_required":true,"evidence":[],"warnings":[],"score":0.,"score_components":{},"outgoing":p,"incoming":p,"launch_in_seconds":10.,"prepare_within_seconds":4.,"preparation_budget_seconds":6.,"readiness":"ready","incoming_fingerprint":null})).unwrap();
        (o, s)
    }
    #[test]
    fn uncertain_arm_keeps_its_native_identity_without_submitting_another_move() {
        let timeout = mix_error("timeout", "fence pending").with_detail("job_id", "native-job");
        assert_eq!(uncertain_mix_job(&timeout), Some("native-job"));
        assert!(uncertain_mix_job(&mix_error("timeout", "unknown submission")).is_none());
        assert!(uncertain_mix_job(
            &mix_error("arm_failed", "rejected").with_detail("job_id", "rejected")
        )
        .is_none());
    }
    #[test]
    fn automatic_retry_requires_retained_ownership_and_live_runway_and_is_bounded() {
        let (option, s) = fixture("filter_blend");
        let record = json!({"operation_id":"retry-test","option":option,"state":"armed","automatic":true,"job_id":null,"error":null});
        let mut p = restored_mix_operation(&record, None).unwrap();
        p.cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        p.snapshot = Some(s.clone());
        assert_eq!(mix_native_operation_id(&p), "retry-test");
        p.recovery_attempts = 1;
        assert_eq!(mix_native_operation_id(&p), "retry-test:retry-1");
        assert_eq!(p.id, "retry-test");
        p.recovery_attempts = 0;
        let mut current = s;
        current.decks[1].grid_revision += 1;
        for deck in &mut current.decks {
            deck.track = None;
        }
        assert!(can_retry_mix(&p, &current, 0, "grid_changed"));
        assert!(can_retry_mix(&p, &current, 0, "deadline_expired"));
        assert!(can_retry_mix(&p, &current, 0, "stale_preparation"));
        let mut same_grid = current.clone();
        same_grid.decks[1].grid_revision -= 1;
        assert!(!can_retry_mix(&p, &same_grid, 0, "stale_preparation"));
        assert!(!can_retry_mix(&p, &current, 1, "grid_changed"));
        assert!(!can_retry_mix(&p, &current, 0, "manual_takeover"));
        p.recovery_attempts = 1;
        assert!(!can_retry_mix(&p, &current, 0, "grid_changed"));
        p.recovery_attempts = 0;
        current.decks[0].position_seconds = 295.;
        assert!(!can_retry_mix(&p, &current, 0, "grid_changed"));
        current.decks[0].position_seconds = 0.;
        current.load_revisions[1] += 1;
        assert!(!can_retry_mix(&p, &current, 0, "grid_changed"));
    }
    #[test]
    fn restart_retains_operation_identity_and_emits_an_honest_terminal_state() {
        let (o, _) = fixture("eq_blend");
        let record = json!({"operation_id":"move-before-restart","state":"preparing","option":o,
            "automatic":true,"job_id":null,"error":null});
        let restored = restored_mix_operation(&record, None).unwrap();
        assert_eq!(restored.state, "interrupted");
        assert!(restored.cancelled.load(Ordering::Acquire));
        assert!(restored.snapshot.is_none());
        assert_eq!(
            mix_event_feedback(&mix_summary(&restored))["terminal"],
            true
        );
        let completed = restored_mix_operation(&record, Some("completed")).unwrap();
        assert_eq!(completed.state, "completed");
    }
    #[test]
    fn full_rate_range_recipe_stays_inside_engine_sync_bounds() {
        let (mut option, snapshot) = fixture("eq_blend");
        option.max_stretch_percent = 100.;
        let request = compile_mix_recipe("wide-rate", &option, &snapshot).unwrap();
        let (_, commands) = scheduled_commands_at(&request, Some(&snapshot)).unwrap();
        assert!(commands.iter().any(|step| matches!(&step.command,
            EngineCommand::SetSyncRateLimits { limits: Some(limits), .. }
                if limits.min_rate == 0.1 && limits.max_rate == 2.)));
    }
    #[test]
    fn tempo_readiness_uses_entry_rate_and_not_the_outgoing_instantaneous_bpm() {
        let (o, mut s) = fixture("eq_blend");
        s.decks[0].actual_playback_bpm = Some(118.8);
        validate_mix_readiness(&o, &s).unwrap();
        s.decks[1].playback_rate = 1.01;
        assert_eq!(
            validate_mix_readiness(&o, &s).unwrap_err().code,
            "tempo_policy"
        );
    }
    #[test]
    fn event_feedback_is_bounded_and_recovery_is_not_reported_as_success() {
        let feedback = mix_event_feedback(&json!({"operation_id":"move","state":"recovered",
            "option":{"source":"real-source","waveform_bands":vec![0.;4096]},"fallback_used":true,
            "error":{"code":"recovered","message":"Carrier restored","details":{"large_debug_snapshot":vec![0.;4096]}}}));
        assert_eq!(feedback["terminal"], true);
        assert_eq!(feedback["decision_required"], true);
        assert!(feedback.to_string().len() < 700);
        assert!(feedback["option"].is_null());
        assert!(mix_terminal("cancelled"));
        assert!(!mix_terminal("transitioning"));
    }
    #[test]
    fn all_recipes_gate_every_transfer_on_the_incoming_carrier() {
        for technique in [
            "eq_blend",
            "filter_blend",
            "short_exchange",
            "echo_out",
            "reverb_exit",
        ] {
            let (o, s) = fixture(technique);
            validate_mix_readiness(&o, &s).unwrap();
            let request = compile_mix_recipe("test-operation", &o, &s).unwrap();
            let (schedule, commands) = scheduled_commands_at(&request, Some(&s)).unwrap();
            deck_engine::validate_scheduled_plan(&schedule, commands.clone(), &s).unwrap();
            let mut unguarded = commands.clone();
            unguarded
                .iter_mut()
                .find(|step| {
                    matches!(
                        step.command,
                        EngineCommand::Ramp {
                            target: deck_engine::ControlTarget::ChannelFader { .. },
                            ..
                        } | EngineCommand::SetChannelFader { .. }
                    )
                })
                .unwrap()
                .require = None;
            assert!(
                deck_engine::validate_scheduled_plan(&schedule, unguarded, &s).is_err(),
                "The native compiler must reject a missing carrier proof"
            );
            assert_eq!(schedule.recovery.carrier, 0);
            assert_eq!(schedule.completion_carrier, Some(1));
            assert!(schedule.pause_outgoing);
            assert!(commands.len() < 16);
            assert!(matches!(
                commands[3].command,
                EngineCommand::PlayQuantized {
                    deck: 1,
                    clock: 0,
                    ..
                }
            ));
            for command in &commands[4..] {
                command.command.validate().unwrap();
                let proof = command
                    .require
                    .expect("Every mixer phase depends on successful incoming launch");
                assert_eq!(proof.deck, 1);
                assert_eq!(proof.playing, Some(true));
                assert!(proof.ready);
                assert_eq!(proof.advanced_from_seconds, Some(0.));
                assert!(proof.min_advanced_beats >= 1.);
            }
            assert!(schedule
                .recovery
                .controls
                .iter()
                .any(|c| matches!(c,EngineCommand::SetChannelFader{deck:0,value} if *value==1.)));
            assert!(schedule
                .recovery
                .controls
                .iter()
                .any(|c| matches!(c,EngineCommand::SetChannelFader{deck:1,value} if *value==0.)));
        }
    }
    #[test]
    fn reference_guard_allows_normal_advance_but_rejects_seek_and_missed_launch() {
        let (o, mut s) = fixture("eq_blend");
        s.decks[0].position_seconds = 3.;
        mix_reference_guard(&o, &s).unwrap();
        s.decks[0].position_revision += 1;
        assert_eq!(
            mix_reference_guard(&o, &s).unwrap_err().code,
            "stale_option"
        );
        s.decks[0].position_revision -= 1;
        s.decks[0].position_seconds = 9.;
        assert_eq!(
            mix_reference_guard(&o, &s).unwrap_err().code,
            "deadline_expired"
        );
    }
    #[test]
    fn empty_open_channel_can_be_prepared_but_loaded_open_channel_is_protected() {
        let (_, mut s) = fixture("eq_blend");
        s.decks[1].channel_fader = 1.;
        assert_eq!(
            validate_mix_target_availability(&s.decks[1])
                .unwrap_err()
                .code,
            "deck_protected"
        );
        s.decks[1].track = None;
        validate_mix_target_availability(&s.decks[1]).unwrap();
        s.decks[1].playing = true;
        assert_eq!(
            validate_mix_target_availability(&s.decks[1])
                .unwrap_err()
                .code,
            "deck_protected"
        );
    }
    #[test]
    fn readiness_rejects_unprepared_tempo_and_unsafe_routing() {
        let (o, mut s) = fixture("eq_blend");
        s.decks[1].tempo_percent = 10.;
        assert_eq!(
            validate_mix_readiness(&o, &s).unwrap_err().code,
            "tempo_policy"
        );
        s.decks[1].tempo_percent = 0.;
        s.crossfader = 1.;
        assert_eq!(
            validate_mix_readiness(&o, &s).unwrap_err().code,
            "routing_review"
        );
        s.crossfader = 0.;
        s.decks[1].playing = true;
        assert_eq!(
            validate_mix_readiness(&o, &s).unwrap_err().code,
            "not_ready"
        );
    }
    #[test]
    fn recipe_rejects_timing_without_advancement_or_verification_margin() {
        let (mut o, s) = fixture("short_exchange");
        o.handover_event = o.launch_event + 3;
        assert!(compile_mix_recipe("test", &o, &s).is_err());
        o.handover_event = 36;
        o.clear_event = 599;
        assert!(compile_mix_recipe("test", &o, &s).is_err());
    }
    #[test]
    fn preparation_only_changes_the_isolated_target_and_never_plays_it() {
        let (o, _) = fixture("filter_blend");
        let controls = mix_prepare_controls(&o);
        let entry = controls
            .iter()
            .position(|c| matches!(c, EngineCommand::Seek { .. }))
            .unwrap();
        let tempo = controls
            .iter()
            .position(|c| matches!(c, EngineCommand::SetPlaybackBpm { .. }))
            .unwrap();
        assert!(
            entry < tempo,
            "Tempo must be resolved at the actual selected entry"
        );
        assert!(controls
            .iter()
            .any(|c| matches!(c,EngineCommand::SetPitch{semitones,..} if *semitones==0.)));
        assert!(controls
            .iter()
            .any(|c| matches!(c,EngineCommand::SetFx{echo,reverb,..} if *echo==0. && *reverb==0.)));
        for command in controls {
            command.validate().unwrap();
            assert_eq!(command.deck(), Some(1));
            assert!(!matches!(
                command,
                EngineCommand::Play { .. }
                    | EngineCommand::PlayQuantized { .. }
                    | EngineCommand::SetMaster { .. }
            ));
        }
    }
    #[test]
    fn headphone_listening_changes_preserve_prepared_ownership() {
        let (_, old) = fixture("eq_blend");
        let mut current = old.clone();
        current.control_revision += 4;
        current.cue_gain = 0.8;
        current.cue_mix = 0.4;
        current.decks[1].cue_enabled = true;
        for target in [2, 3, deck_engine::CONTROL_TARGET_COUNT + 1] {
            current.target_control_revisions[target] += 1;
        }
        assert!(!mix_ownership_changed(&old, &current));
        current.target_control_revisions[0] += 1;
        assert!(mix_ownership_changed(&old, &current));
        current.target_control_revisions[0] -= 1;
        current.deck_control_revisions[1] += 1;
        assert!(mix_ownership_changed(&old, &current));
    }
    #[test]
    fn fast_transport_without_metadata_preserves_a_ready_move() {
        let (option, old) = fixture("eq_blend");
        assert_transport_preserves_prepared_move(option, old);
    }
    #[test]
    #[ignore = "Requires captured actual local-song snapshot; no audio hardware"]
    fn actual_local_prepared_move_survives_metadata_free_transport_feedback() {
        let path = std::env::var("PULSEDECK_REAL_MOVE_INPUT").unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let option = serde_json::from_value(value["option"].clone()).unwrap();
        let snapshot: deck_engine::EngineSnapshot =
            serde_json::from_value(value["snapshot"].clone()).unwrap();
        for deck in &snapshot.decks {
            assert!(std::path::Path::new(&deck.track.as_ref().unwrap().path).is_file());
        }
        assert_transport_preserves_prepared_move(option, snapshot);
    }
    fn assert_transport_preserves_prepared_move(
        option: crate::next_move::MoveOption,
        old: deck_engine::EngineSnapshot,
    ) {
        let mut current = old.clone();
        for deck in &mut current.decks {
            deck.track = None;
        }
        let prepared = PreparedMix {
            id: "transport-regression".into(),
            option,
            state: "ready".into(),
            snapshot: Some(old),
            job_id: None,
            error: None,
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            preview_cue: None,
            preview_deadline: None,
            automatic: true,
            fallback_used: false,
            preparation_deadline: None,
            restored: false,
            arm_confirmation_deadline: None,
            recovery_attempts: 0,
            takeover_revision: 0,
        };
        assert!(!mix_transport_invalidates(&prepared, &current));
        for deck in 0..2 {
            current.load_revisions[deck] += 1;
            assert!(mix_transport_invalidates(&prepared, &current));
            current.load_revisions[deck] -= 1;
            current.deck_control_revisions[deck] += 1;
            assert!(mix_transport_invalidates(&prepared, &current));
            current.deck_control_revisions[deck] -= 1;
            current.decks[deck].position_revision += 1;
            assert!(mix_transport_invalidates(&prepared, &current));
            current.decks[deck].position_revision -= 1;
            current.decks[deck].grid_revision += 1;
            assert!(mix_transport_invalidates(&prepared, &current));
            current.decks[deck].grid_revision -= 1;
        }
        current.target_control_revisions[0] += 1;
        assert!(mix_transport_invalidates(&prepared, &current));
        current.target_control_revisions[0] -= 1;
        current.engine_epoch += 1;
        assert!(mix_transport_invalidates(&prepared, &current));
        current.engine_epoch -= 1;
        current.decks[prepared.option.reference_deck].position_seconds =
            prepared.option.launch_seconds;
        assert!(mix_transport_invalidates(&prepared, &current));
        current.decks[prepared.option.reference_deck].position_seconds =
            prepared.snapshot.as_ref().unwrap().decks[prepared.option.reference_deck]
                .position_seconds;
        current.running = false;
        assert!(mix_transport_invalidates(&prepared, &current));
    }
    #[test]
    fn mismatched_reference_metadata_is_never_ready() {
        let (o, mut s) = fixture("eq_blend");
        s.decks[0].track.as_mut().unwrap().path = "/test/replaced.wav".into();
        assert_eq!(
            mix_reference_guard(&o, &s).unwrap_err().code,
            "stale_option"
        );
    }
    #[test]
    fn audition_requires_a_distinct_audible_headphone_route() {
        let (_, mut s) = fixture("eq_blend");
        let mut status = deck_engine::AudioStatus {
            running: true,
            device_name: "Test".into(),
            sample_rate: 48000,
            channels: 4,
            buffer_frames: 512,
            routing: deck_engine::AudioRouting::default(),
        };
        assert_eq!(
            validate_mix_preview_route(Some(&status), &s)
                .unwrap_err()
                .code,
            "headphone_route_required"
        );
        status.routing.cue_left = Some(2);
        status.routing.cue_right = Some(3);
        s.output_channels = 4;
        s.cue_mix = 0.;
        validate_mix_preview_route(Some(&status), &s).unwrap();
        status.routing.cue_left = Some(0);
        assert_eq!(
            validate_mix_preview_route(Some(&status), &s)
                .unwrap_err()
                .code,
            "headphone_route_required"
        );
        status.routing.cue_left = Some(2);
        s.cue_mix = 1.;
        assert_eq!(
            validate_mix_preview_route(Some(&status), &s)
                .unwrap_err()
                .code,
            "headphone_controls_required"
        );
    }
    #[test]
    fn audition_controls_never_open_master_or_change_the_outgoing_deck() {
        let (o, _) = fixture("eq_blend");
        for command in mix_preview_controls(&o, true)
            .into_iter()
            .chain(mix_preview_controls(&o, false))
        {
            command.validate().unwrap();
            assert_eq!(command.deck(), Some(o.target_deck));
            assert!(matches!(
                command,
                EngineCommand::SetHeadphoneCue { .. }
                    | EngineCommand::Play { .. }
                    | EngineCommand::Pause { .. }
                    | EngineCommand::Seek { .. }
            ));
            if let EngineCommand::Seek { seconds, .. } = command {
                assert_eq!(seconds, o.entry_seconds);
            }
        }
    }
}

fn mix_deck_ownership_changed(
    old: &deck_engine::EngineSnapshot,
    current: &deck_engine::EngineSnapshot,
    deck: usize,
) -> bool {
    mix_deck_revisions_changed(old, current, deck)
        || old.decks[deck]
            .track
            .as_ref()
            .map(|t| (&t.path, &t.source_fingerprint))
            != current.decks[deck]
                .track
                .as_ref()
                .map(|t| (&t.path, &t.source_fingerprint))
}
fn mix_deck_revisions_changed(
    old: &deck_engine::EngineSnapshot,
    current: &deck_engine::EngineSnapshot,
    deck: usize,
) -> bool {
    old.engine_epoch != current.engine_epoch
        || old.load_revisions[deck] != current.load_revisions[deck]
        || old.deck_control_revisions[deck] != current.deck_control_revisions[deck]
        || old.decks[deck].grid_revision != current.decks[deck].grid_revision
        || old.decks[deck].position_revision != current.decks[deck].position_revision
}
fn mix_transport_invalidates(
    prepared: &PreparedMix,
    current: &deck_engine::EngineSnapshot,
) -> bool {
    // Fast transport deliberately omits metadata; load revisions carry identity.
    !current.running
        || prepared.snapshot.as_ref().is_some_and(|old| {
            mix_deck_revisions_changed(old, current, 0)
                || mix_deck_revisions_changed(old, current, 1)
                || old.target_control_revisions[..2] != current.target_control_revisions[..2]
        })
        || prepared.option.launch_seconds
            <= current.decks[prepared.option.reference_deck].position_seconds
}
fn mix_ownership_changed(
    old: &deck_engine::EngineSnapshot,
    current: &deck_engine::EngineSnapshot,
) -> bool {
    // Native ControlTarget slots 0/1 are crossfader/master gain; cue targets are independent.
    mix_deck_ownership_changed(old, current, 0)
        || mix_deck_ownership_changed(old, current, 1)
        || old.target_control_revisions[..2] != current.target_control_revisions[..2]
}
fn validate_mix_preview_route(
    status: Option<&deck_engine::AudioStatus>,
    snapshot: &deck_engine::EngineSnapshot,
) -> Result<()> {
    let status = status.ok_or_else(|| {
        mix_error(
            "headphone_route_required",
            "Configure a distinct stereo headphone cue output in Audio settings before auditioning",
        )
    })?;
    let route = &status.routing;
    let (left,right)=route.cue_left.zip(route.cue_right)
        .ok_or_else(||mix_error("headphone_route_required","Assign separate Cue Left/Right channels in Audio settings; the default stereo master output has no headphone route"))?;
    if !status.running
        || left == right
        || left >= status.channels
        || right >= status.channels
        || left >= snapshot.output_channels
        || right >= snapshot.output_channels
        || status.sample_rate != snapshot.sample_rate
        || [route.master_left, route.master_right].contains(&left)
        || [route.master_left, route.master_right].contains(&right)
    {
        return Err(mix_error(
            "headphone_route_required",
            "Headphone cue must use two valid output channels distinct from the master channels",
        ));
    }
    if snapshot.cue_gain <= 0.001 || snapshot.cue_mix >= 0.999 {
        return Err(mix_error(
            "headphone_controls_required",
            "Raise headphone Cue Gain and turn Cue/Master Mix toward Cue before auditioning",
        ));
    }
    Ok(())
}
fn mix_preview_controls(
    option: &crate::next_move::MoveOption,
    enabled: bool,
) -> Vec<EngineCommand> {
    let deck = option.target_deck;
    if enabled {
        vec![
            EngineCommand::SetHeadphoneCue {
                deck,
                enabled: true,
            },
            EngineCommand::Play { deck },
        ]
    } else {
        vec![
            EngineCommand::Pause { deck },
            EngineCommand::Seek {
                deck,
                seconds: option.entry_seconds,
            },
        ]
    }
}
fn preview_mix(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let id: String = arg(&request.args, "prepared_id")?;
    let enabled: bool = arg(&request.args, "enabled")?;
    let c = app.state::<Control>();
    let _submission = c.scheduled_submission.lock().map_err(failure)?;
    let _preparation = c.mix_preparing.try_lock().map_err(|_| {
        mix_error(
            "busy",
            "Another preparation or audition change is in progress",
        )
    })?;
    let prepared = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .get(&id)
        .cloned()
        .ok_or_else(|| mix_error("not_found", "Prepared mix not found"))?;
    check_mix_cancelled(&prepared.cancelled)?;
    if prepared.state == "previewing" && enabled || prepared.state == "ready" && !enabled {
        return Ok(("applied", mix_summary(&prepared)));
    }
    if (enabled && prepared.state != "ready")
        || (!enabled && prepared.state != "previewing")
        || prepared.job_id.is_some()
    {
        return Err(mix_error(
            "not_ready",
            "Audition requires an unarmed ready source; finish its audition before arming",
        ));
    }
    let state = app.state::<AppState>();
    let expected = prepared
        .snapshot
        .as_ref()
        .ok_or_else(|| mix_error("not_ready", "Prepared source state is missing"))?;
    let initial = state
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    let target = prepared.option.target_deck;
    if mix_deck_ownership_changed(expected, &initial, target) {
        let mut stored = c.mix_prepared.lock().map_err(failure)?;
        let p = stored
            .get_mut(&id)
            .ok_or_else(|| mix_error("not_found", "Prepared source expired"))?;
        p.state = "invalidated".into();
        p.error = Some(mix_error(
            "manual_takeover",
            "The target deck changed; audition yields without restoring cached controls",
        ));
        return Err(p.error.clone().unwrap());
    }
    if initial.decks[target].channel_fader > 0.001 || initial.scheduled_batch.state.active() {
        return Err(mix_error(
            "deck_protected",
            "Audition requires an isolated target deck and no active native transition",
        ));
    }
    if enabled {
        if mix_ownership_changed(expected, &initial) {
            return Err(mix_error(
                "stale_preparation",
                "Prepared source or master routing changed; prepare a fresh option",
            ));
        }
        mix_reference_guard(&prepared.option, &initial)?;
        validate_mix_readiness(&prepared.option, &initial)?;
        validate_mix_preview_route(
            state.audio_status.lock().map_err(failure)?.as_ref(),
            &initial,
        )?;
    }
    type PreviewState = (
        crate::next_move::MoveOption,
        deck_engine::EngineSnapshot,
        Option<(bool, u64)>,
    );
    let result = (|| -> Result<PreviewState> {
        let mut previous = initial.clone();
        let mut cue = prepared.preview_cue;
        for command in mix_preview_controls(&prepared.option, enabled) {
            let current = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            if mix_deck_ownership_changed(&previous, &current, target) {
                return Err(mix_error(
                    "manual_takeover",
                    "The target changed during audition; its current controls are preserved",
                ));
            }
            let mut native = Request::new("engine.command", serialized(&command)?);
            native.timeout_ms = request.timeout_ms;
            native.expected_load_revision = Some(current.load_revisions[target]);
            apply_with_control_guard(
                app,
                &native,
                command.clone(),
                Some(current.control_revision),
            )?;
            previous = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            let mut expected_after = current.clone();
            if !matches!(command, EngineCommand::SetHeadphoneCue { .. }) {
                expected_after.deck_control_revisions[target] =
                    expected_after.deck_control_revisions[target].wrapping_add(1);
            }
            if matches!(command, EngineCommand::Seek { .. }) {
                expected_after.decks[target].position_revision = expected_after.decks[target]
                    .position_revision
                    .wrapping_add(1);
            }
            if mix_deck_ownership_changed(&expected_after, &previous, target) {
                return Err(mix_error("manual_takeover","The target changed immediately after audition control; its current state is preserved"));
            }
            if matches!(command, EngineCommand::Play { .. }) && !previous.decks[target].playing
                || matches!(command, EngineCommand::Pause { .. }) && previous.decks[target].playing
            {
                return Err(mix_error(
                    "not_ready",
                    "Audition transport did not reach its requested state",
                ));
            }
            if enabled && cue.is_none() {
                cue = Some((
                    initial.decks[target].cue_enabled,
                    current.target_control_revisions[deck_engine::CONTROL_TARGET_COUNT + target]
                        .wrapping_add(1),
                ));
            }
        }
        if !enabled {
            let current = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            if mix_deck_ownership_changed(&previous, &current, target) {
                return Err(mix_error("manual_takeover","The target changed while stopping audition; its current controls are preserved"));
            }
            if let Some((original, revision)) = cue {
                if current.target_control_revisions[deck_engine::CONTROL_TARGET_COUNT + target]
                    == revision
                    && current.decks[target].cue_enabled != original
                {
                    let command = EngineCommand::SetHeadphoneCue {
                        deck: target,
                        enabled: original,
                    };
                    let mut native = Request::new("engine.command", serialized(&command)?);
                    native.timeout_ms = request.timeout_ms;
                    native.expected_load_revision = Some(current.load_revisions[target]);
                    apply_with_control_guard(
                        app,
                        &native,
                        command,
                        Some(current.control_revision),
                    )?;
                }
            }
            let refreshed = crate::next_move::refresh_prepared_option(app, &prepared.option)
                .map_err(|e| mix_error("stale_preparation", &e))?;
            let snapshot = state
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            mix_reference_guard(&refreshed, &snapshot)?;
            validate_mix_readiness(&refreshed, &snapshot)?;
            let batch = compile_mix_recipe(&id, &refreshed, &snapshot)?;
            let (schedule, commands) = scheduled_commands_at(&batch, Some(&snapshot))?;
            state
                .engine
                .validate_scheduled_batch(&schedule, commands)
                .map_err(scheduled_failure)?;
            Ok((refreshed, snapshot, None))
        } else {
            Ok((prepared.option.clone(), previous, cue))
        }
    })();
    let mut stored = c.mix_prepared.lock().map_err(failure)?;
    let p = stored
        .get_mut(&id)
        .ok_or_else(|| mix_error("not_found", "Prepared source expired"))?;
    match result {
        Ok((option, snapshot, cue)) => {
            p.option = option;
            p.snapshot = Some(snapshot);
            p.preview_cue = cue;
            p.preview_deadline = enabled.then(|| Instant::now() + Duration::from_secs(30));
            p.state = if enabled { "previewing" } else { "ready" }.into();
            p.error = None;
        }
        Err(error) => {
            p.state = "invalidated".into();
            p.error = Some(error.clone());
            event(app, "mix.updated", mix_summary(p));
            return Err(error.with_detail("prepared_id", id));
        }
    }
    let summary = mix_summary(p);
    drop(stored);
    event(app, "mix.updated", summary.clone());
    Ok((if enabled { "applied" } else { "ready" }, summary))
}

fn poll_mix_previews(app: &tauri::AppHandle) -> Result<()> {
    let c = app.state::<Control>();
    let expired: Vec<String> = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .values()
        .filter(|p| {
            p.state == "previewing"
                && p.preview_deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
        })
        .map(|p| p.id.clone())
        .collect();
    for id in expired {
        let mut request = Request::new("mix.preview", json!({"prepared_id":id,"enabled":false}));
        request.timeout_ms = 2000;
        if let Err(error) = preview_mix(app, &request) {
            if error.code != "busy" {
                event(
                    app,
                    "mix.preview.expired",
                    json!({"prepared_id":id,"error":error}),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
#[ignore = "requires captured actual local-library options and snapshots; writes native render manifests"]
fn export_real_library_recipe_manifests() {
    let input = std::env::var("PULSEDECK_REAL_MIX_INPUT").expect("PULSEDECK_REAL_MIX_INPUT");
    let directory = std::env::var("PULSEDECK_REAL_MIX_OUTPUT").expect("PULSEDECK_REAL_MIX_OUTPUT");
    let value: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let captured: deck_engine::EngineSnapshot =
        serde_json::from_value(value["snapshot"].clone()).unwrap();
    let previous: Vec<crate::next_move::MoveOption> =
        serde_json::from_value(value["options"].clone()).unwrap();
    let reference = previous[0].reference_deck;
    let mut options = crate::next_move::generate(
        &captured,
        reference,
        &captured.decks[1 - reference],
        crate::suggest::EnergyDirection::Keep,
        2.,
        None,
    );
    options.sort_by(|a, b| b.score.total_cmp(&a.score));
    std::fs::create_dir_all(&directory).unwrap();
    for technique in [
        "eq_blend",
        "filter_blend",
        "short_exchange",
        "echo_out",
        "reverb_exit",
    ] {
        let option = options
            .iter()
            .find(|o| o.technique == technique)
            .unwrap_or_else(|| panic!("No real {technique} option"));
        let mut snapshot = captured.clone();
        let a = option.reference_deck;
        let b = option.target_deck;
        assert_eq!(
            snapshot.decks[b].track.as_ref().unwrap().path,
            option.source
        );
        // Offline initial conditions match typed preparation; the live profile stays muted/paused.
        snapshot.master_gain = 1.;
        snapshot.crossfader = 0.;
        snapshot.decks[a].playing = true;
        snapshot.decks[a].channel_fader = 1.;
        snapshot.decks[a].eq_low = 1.;
        snapshot.decks[a].eq_mid = 1.;
        snapshot.decks[a].eq_high = 1.;
        snapshot.decks[a].filter = 0.;
        let d = &mut snapshot.decks[b];
        d.playing = false;
        d.channel_fader = 0.;
        d.gain = 1.;
        d.position_seconds = option.entry_seconds;
        d.sync_enabled = false;
        d.pitch_semitones = 0.;
        d.keylock = true;
        d.echo = 0.;
        d.reverb = 0.;
        d.eq_low = if technique == "eq_blend" { 0. } else { 1. };
        d.eq_mid = 1.;
        d.eq_high = 1.;
        d.filter = if technique == "filter_blend" {
            0.65
        } else {
            0.
        };
        let beat = d.grid_position(option.entry_seconds).unwrap();
        let local = 240. / (d.grid_time(beat + 4.).unwrap() - d.grid_time(beat).unwrap());
        d.local_bpm = Some(local);
        d.playback_rate = option.target_bpm / local;
        d.actual_playback_rate = d.playback_rate;
        d.tempo_percent = (d.playback_rate - 1.) * 100.;
        d.playback_bpm = Some(option.target_bpm);
        d.actual_playback_bpm = d.playback_bpm;
        d.target_playback_bpm = d.playback_bpm;
        validate_mix_readiness(option, &snapshot).unwrap();
        let request = compile_mix_recipe("real-library-offline", option, &snapshot).unwrap();
        let (schedule, commands) = scheduled_commands_at(&request, Some(&snapshot)).unwrap();
        deck_engine::validate_scheduled_plan(&schedule, commands.clone(), &snapshot).unwrap();
        let path = std::path::Path::new(&directory).join(format!("{technique}.json"));
        std::fs::write(&path,serde_json::to_vec(&json!({"snapshot":snapshot,"schedule":schedule,"commands":commands,"option":option,"origin":"actual local-library metadata; planned initial controls; live master/cue remained muted"})).unwrap()).unwrap();
        println!("{}", path.display());
    }
}

#[cfg(test)]
#[test]
#[ignore = "Requires actual local-library captures; emits native normal/fallback/recovery manifests"]
fn export_real_single_call_move_manifests() {
    let input = std::path::PathBuf::from(std::env::var("PULSEDECK_REAL_MOVE_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var("PULSEDECK_REAL_MOVE_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    fn manifest(mut s: deck_engine::EngineSnapshot, o: crate::next_move::MoveOption) -> Value {
        let a = o.reference_deck;
        let b = o.target_deck;
        s.running = true;
        s.master_gain = 1.;
        s.crossfader = 0.;
        s.decks[a].playing = true;
        s.decks[a].channel_fader = 1.;
        let d = &mut s.decks[b];
        d.position_seconds = o.entry_seconds;
        d.playing = false;
        d.channel_fader = 0.;
        d.eq_low = if o.technique == "eq_blend" { 0. } else { 1. };
        d.filter = if o.technique == "filter_blend" {
            0.65
        } else {
            0.
        };
        let beat = d.grid_position(o.entry_seconds).unwrap();
        let local = 240. / (d.grid_time(beat + 4.).unwrap() - d.grid_time(beat).unwrap());
        d.local_bpm = Some(local);
        d.playback_rate = o.target_bpm / local;
        d.actual_playback_rate = d.playback_rate;
        d.tempo_percent = (d.playback_rate - 1.) * 100.;
        d.playback_bpm = Some(o.target_bpm);
        d.actual_playback_bpm = d.playback_bpm;
        validate_mix_readiness(&o, &s).unwrap();
        let request = compile_mix_recipe("real-one-call", &o, &s).unwrap();
        let (schedule, commands) = scheduled_commands_at(&request, Some(&s)).unwrap();
        deck_engine::validate_scheduled_plan(&schedule, commands.clone(), &s).unwrap();
        json!({"snapshot":s,"option":o,"schedule":schedule,"commands":commands,
            "origin":"Unchanged actual local MP3s; prepared offline controls; no hardware output"})
    }
    for name in ["eq_blend", "filter_blend", "short_exchange"] {
        let captured: Value =
            serde_json::from_slice(&std::fs::read(input.join(format!("{name}.json"))).unwrap())
                .unwrap();
        let s: deck_engine::EngineSnapshot =
            serde_json::from_value(captured["snapshot"].clone()).unwrap();
        let old: crate::next_move::MoveOption =
            serde_json::from_value(captured["option"].clone()).unwrap();
        let o = crate::next_move::revalidate_prepared_window(&s, &old, false).unwrap();
        assert!(o.continuation_seconds >= o.minimum_continuation_seconds);
        let value = manifest(s.clone(), o.clone());
        std::fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        if name == "eq_blend" {
            let mut delayed = s;
            delayed.decks[o.reference_deck].position_seconds = o.launch_seconds + 0.1;
            let fallback =
                crate::next_move::revalidate_prepared_window(&delayed, &o, true).unwrap();
            let fallback = manifest(delayed, fallback);
            std::fs::write(
                output.join("fallback.json"),
                serde_json::to_vec(&fallback).unwrap(),
            )
            .unwrap();
            let mut recovery = value;
            recovery["stall_incoming_after_launch"] = json!(true);
            std::fs::write(
                output.join("recovery.json"),
                serde_json::to_vec(&recovery).unwrap(),
            )
            .unwrap();
        }
    }
    println!("{}", output.display());
}
