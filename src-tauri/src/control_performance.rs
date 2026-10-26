// Reviewed scores stay immutable between validation and commitment. Drafts are
// bounded and ephemeral; committed tickets remain in the existing job journal.
#[derive(Clone)]
struct PerformanceDraft {
    ticket: String,
    plan: control_protocol::performance::Plan,
}
fn performance_commit_plan(
    control: &Control,
    input: control_protocol::performance::CommitRequest,
) -> Result<(control_protocol::performance::Plan, Option<String>)> {
    match (input.plan, input.ticket) {
        (Some(plan), None) => Ok((plan, None)),
        (None, Some(ticket)) if !ticket.is_empty() && ticket.len() <= 128 => {
            if let Some(draft) = control
                .performance_drafts
                .lock()
                .map_err(failure)?
                .iter()
                .find(|draft| draft.ticket == ticket)
                .cloned()
            {
                return Ok((draft.plan, Some(ticket)));
            }
            // An uncertain commit can be retried after restart without replaying
            // its controls. The canonical plan fingerprint still governs retries.
            let jobs = control.jobs.lock().map_err(failure)?;
            if let Some(job) = jobs.values().find(|job| {
                job.status["ticket"] == ticket
                    || job.status["ticket_aliases"]
                        .as_array()
                        .is_some_and(|aliases| aliases.iter().any(|alias| alias == &ticket))
            }) {
                let plan = serde_json::from_value(job.status["plan"].clone()).map_err(failure)?;
                return Ok((plan, Some(ticket)));
            }
            Err(ControlError::new(
                "ticket_not_found",
                "Reviewed plan ticket is unavailable; validate the intended score again",
            )
            .with_detail("ticket", ticket))
        }
        _ => Err(ControlError::new(
            "invalid_arguments",
            "Provide exactly one reviewed ticket or plan",
        )),
    }
}
fn performance_bind_ticket(control: &Control, job_id: &str, ticket: &str) -> Result<()> {
    let mut jobs = control.jobs.lock().map_err(failure)?;
    let job = jobs.get_mut(job_id).ok_or_else(|| {
        ControlError::new("job_not_found", "Committed performance history expired")
    })?;
    if job.status["ticket"] == ticket {
        return Ok(());
    }
    if !job.status["ticket_aliases"].is_array() {
        job.status["ticket_aliases"] = json!([]);
    }
    let aliases = job.status["ticket_aliases"].as_array_mut().unwrap();
    if !aliases.iter().any(|alias| alias == ticket) {
        if aliases.len() >= 64 {
            aliases.remove(0);
        }
        aliases.push(json!(ticket));
    }
    Ok(())
}
fn performance_save_draft(
    control: &Control,
    plan: control_protocol::performance::Plan,
    validation: &mut control_protocol::performance::Validation,
) -> Result<()> {
    let sequence = control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1;
    let ticket = format!("score-{}-{}-{sequence}", std::process::id(), unix_ms());
    // Resolved starts already measure remaining renderer time at validation.
    validation.arm_within_seconds = validation
        .actions
        .iter()
        .map(|action| action.start_seconds)
        .min_by(f64::total_cmp);
    validation.ticket = Some(ticket.clone());
    let mut drafts = control.performance_drafts.lock().map_err(failure)?;
    while drafts.len() >= 64 {
        drafts.pop_front();
    }
    drafts.push_back(PerformanceDraft { ticket, plan });
    Ok(())
}

// Typed artist scores share the native scheduler and the existing bounded journal.
fn performance_observation(
    snapshot: &deck_engine::EngineSnapshot,
    query: control_protocol::performance::ObserveRequest,
) -> Result<control_protocol::performance::Observation> {
    use control_protocol::performance::*;
    if query.deck.is_some_and(|d| d > 1) || query.count.is_some_and(|n| n > 64) {
        return Err(ControlError::new(
            "invalid_arguments",
            "Use deck 0/A or 1/B and count 0..64",
        ));
    }
    let decks = std::array::from_fn(|i| {
        let d = &snapshot.decks[i];
        DeckObservation {
            deck: i,
            source: d.track.as_ref().map(|t| t.path.clone()),
            duration_seconds: d.track.as_ref().map(|t| t.duration_seconds),
            playing: d.playing,
            launch_pending: d.launch_pending,
            launch_clock: d.launch_clock,
            launch_at_seconds: d.launch_at_seconds,
            ready: d.track.is_some() && !d.preparation_pending,
            position_seconds: d.position_seconds,
            effective_beat: d.beat_position,
            raw_event_position: d.beat_position.map(|b| {
                b + if d.beatgrid_uniform {
                    0.
                } else {
                    d.beatgrid_anchor_index.unwrap_or(0) as f64
                }
            }),
            bar_origin_known: d.beatgrid_downbeat,
            controls: DeckControls {
                channel_fader: d.channel_fader,
                channel_fader_actual: d.channel_fader_actual,
                channel_fader_ramp_active: snapshot.active_ramps
                    [deck_engine::ControlTarget::ChannelFader { deck: i }.index()],
                gain: d.gain,
                eq: [d.eq_low, d.eq_mid, d.eq_high],
                filter: d.filter,
                tempo_percent: d.tempo_percent,
                playback_rate: d.playback_rate,
                actual_playback_rate: d.actual_playback_rate,
                pitch_semitones: d.pitch_semitones,
                keylock: d.keylock,
                sync_enabled: d.sync_enabled,
                is_master: d.is_master,
                echo: d.echo,
                reverb: d.reverb,
                loop_start_seconds: d.loop_start_seconds,
                loop_end_seconds: d.loop_end_seconds,
            },
        }
    });
    let diff = snapshot.decks[0]
        .beat_position
        .zip(snapshot.decks[1].beat_position)
        .map(|(a, b)| a - b);
    let phase = diff.map(|n| {
        let p = n.rem_euclid(1.);
        p.min(1. - p)
    });
    let meters = snapshot.decks.each_ref().map(|d| {
        d.track
            .as_ref()
            .and_then(|t| t.neural_rhythm.as_ref())
            .and_then(|r| r.beats_per_bar.map(|m| m as u8))
    });
    let meter = if meters[0].is_some() && meters[0] == meters[1] {
        meters[0]
    } else {
        None
    };
    let bar = if snapshot.decks.iter().all(|d| d.beatgrid_downbeat) {
        meter.and_then(|m| diff.map(|n| n.rem_euclid(m as f64)))
    } else {
        None
    };
    let deck = query.deck.unwrap_or(0);
    let d = &snapshot.decks[deck];
    let start = query.start_event.unwrap_or_else(|| {
        d.beat_position.unwrap_or(0.).floor() as i64
            + 1
            + if d.beatgrid_uniform {
                0
            } else {
                d.beatgrid_anchor_index.unwrap_or(0) as i64
            }
    });
    let events = (0..query.count.unwrap_or(16))
        .map(|n| deck_engine::performance::event_handle(snapshot, deck, start + n as i64))
        .take_while(Result::is_ok)
        .collect::<Result<Vec<_>>>()?;
    Ok(Observation {
        generation: deck_engine::performance::generation(snapshot),
        running: snapshot.running,
        decks,
        fractional_phase_error_beats: phase,
        bar_offset_beats: bar,
        bar_beats: meter,
        events,
        native_owner: (snapshot.scheduled_batch.state.active())
            .then_some(snapshot.scheduled_batch.id),
        session: None,
    })
}
fn performance_receipts(
    app: &tauri::AppHandle,
    id: &str,
) -> Result<control_protocol::performance::Receipts> {
    use control_protocol::performance::*;
    refresh_scheduled_jobs(app)?;
    let c = app.state::<Control>();
    let jobs = c.jobs.lock().map_err(failure)?;
    let job = jobs
        .get(id)
        .ok_or_else(|| ControlError::new("job_not_found", "Performance job not found"))?;
    if job.status["mode"] != "performance" {
        return Err(ControlError::new(
            "invalid_arguments",
            "This job is not a typed performance",
        ));
    }
    let validation: Validation =
        serde_json::from_value(job.status["validation"].clone()).map_err(failure)?;
    let mut members = Vec::new();
    for action in validation.actions {
        for native_action_id in action.native_action_ids {
            let receipt = job.status["native"]["events"]
                .as_array()
                .and_then(|a| a.iter().find(|r| r["action_id"] == native_action_id));
            let state = receipt.and_then(|r| r["state"].as_u64()).unwrap_or(0);
            members.push(MemberReceipt {
                action_id: action.id.clone(),
                native_action_id,
                state: match state {
                    1 | 2 => ReceiptState::Applied,
                    3 => ReceiptState::Cancelled,
                    4 => ReceiptState::Failed,
                    _ => ReceiptState::Pending,
                },
                verified: state == 2,
                curve_boundary_verified: receipt
                    .is_some_and(|r| r["curve_boundary_verified"] == true),
                frame: receipt.and_then(|r| r["frame"].as_u64()).unwrap_or(0),
                completed_frame: receipt
                    .and_then(|r| r["completed_frame"].as_u64())
                    .unwrap_or(0),
                failure: receipt
                    .and_then(|r| r["failure"]["cause"].as_str())
                    .map(str::to_owned),
            });
        }
    }
    let recovery_applied = job.status["native"]["recovery_applied"] == true;
    let lifecycle = job.status["state"].as_str().unwrap_or("pending").to_owned();
    let operation_id = job.status["operation_id"].as_str().unwrap_or("").to_owned();
    let reason = job.status["reason"].as_str().unwrap_or("none").to_owned();
    drop(jobs);
    let snapshot = app
        .state::<AppState>()
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    Ok(Receipts {
        id: id.into(),
        operation_id,
        lifecycle: lifecycle.clone(),
        reason,
        recovery_applied,
        members,
        observation: performance_observation(
            &snapshot,
            ObserveRequest {
                count: Some(0),
                ..Default::default()
            },
        )?,
    })
}
fn performance_dispatch(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    use control_protocol::performance::*;
    let state = app.state::<AppState>();
    let engine = &state.engine;
    match request.command.as_str() {
        "performance.observe" => {
            let query: ObserveRequest =
                serde_json::from_value(request.args.clone()).map_err(failure)?;
            let snapshot = engine.scheduled_snapshot().map_err(scheduled_failure)?;
            let mut observation = performance_observation(&snapshot, query)?;
            performance_session_endpoint(app, &mut observation)?;
            Ok(("applied", serialized(observation)?))
        }
        "performance.validate" => {
            let input: PlanRequest =
                serde_json::from_value(request.args.clone()).map_err(failure)?;
            performance_session_guard(app, &input.plan)?;
            let snapshot = engine.scheduled_snapshot().map_err(scheduled_failure)?;
            let mut validation = deck_engine::performance::validate(&input.plan, &snapshot)?;
            performance_save_draft(&app.state::<Control>(), input.plan, &mut validation)?;
            Ok(("applied", serialized(validation)?))
        }
        "performance.commit" => {
            let input: CommitRequest =
                serde_json::from_value(request.args.clone()).map_err(failure)?;
            let c = app.state::<Control>();
            let (plan, ticket) = performance_commit_plan(&c, input)?;
            let input = PlanRequest { plan };
            let fingerprint = queue_fingerprint(&json!({"plan": input.plan}));
            if let Some(receipt) =
                performance_duplicate_receipt(&c, &input.plan.operation_id, &fingerprint)?
            {
                if let Some(ticket) = &ticket {
                    performance_bind_ticket(&c, &receipt.job_id, ticket)?;
                    persist_control(app)?;
                }
                return Ok(("accepted", serialized(receipt)?));
            }
            performance_session_guard(app, &input.plan)?;
            let snapshot = engine.scheduled_snapshot().map_err(scheduled_failure)?;
            let mut validation = deck_engine::performance::validate(&input.plan, &snapshot)?;
            validation.ticket = ticket.clone();
            let native_id = c.job_sequence.fetch_add(1, Ordering::Relaxed) + 1;
            let id = format!("job-{}-{native_id}", std::process::id());
            let actions:Vec<Value>=validation.actions.iter().flat_map(|a|a.native_action_ids.iter().map(move |native|json!({"action_id":a.id,"native_action_id":native,"state":"pending"}))).collect();
            insert_job_bounded(
                &mut *c.jobs.lock().map_err(failure)?,
                id.clone(),
                Job {
                    cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    native_id: Some(native_id),
                    fingerprint: Some(fingerprint),
                    status: json!({"mode":"performance","state":"armed","lifecycle":"armed","operation_id":input.plan.operation_id,"validation":validation,"plan":input.plan,"ticket":ticket,"actions":actions,"patches":[],"revision":1,"created_unix_ms":unix_ms(),"created_sequence":native_id}),
                },
            )?;
            persist_control(app)?;
            // Recompile from a fresh coherent snapshot; ArmBatch checks every
            // generation again in the callback before any control is applied.
            let mut submitted = false;
            let armed = engine
                .arm_performance(native_id, &input.plan)
                .and_then(|mut v| {
                    v.ticket = ticket.clone();
                    submitted = true;
                    if let Some(job) = c.jobs.lock().map_err(failure)?.get_mut(&id) {
                        job.status["validation"] = serialized(&v)?;
                    }
                    persist_control(app)?;
                    let fence = engine.fence().map_err(failure)?;
                    engine
                        .wait_for_fence(fence, std::time::Duration::from_millis(request.timeout_ms))
                        .map_err(failure)?;
                    let actual = engine.transport_snapshot().scheduled_batch;
                    if actual.id != native_id || actual.state == deck_engine::BatchLifecycle::Failed
                    {
                        return Err(ControlError::new(
                            "arm_rejected",
                            format!(
                                "Native ownership/generation arm rejected: {:?}",
                                actual.reason
                            ),
                        ));
                    }
                    Ok(v)
                });
            match armed {
                Ok(validation) => {
                    register_mix_session_job(app, &id)?;
                    refresh_scheduled_jobs(app)?;
                    Ok((
                        "accepted",
                        serialized(CommitReceipt {
                            job_id: id,
                            operation_id: input.plan.operation_id,
                            lifecycle: serialized(
                                engine.transport_snapshot().scheduled_batch.state,
                            )?
                            .as_str()
                            .unwrap_or("pending")
                            .into(),
                            validation,
                            duplicate: false,
                            replayed: false,
                        })?,
                    ))
                }
                Err(error) => {
                    if !submitted {
                        if let Some(job) = c.jobs.lock().map_err(failure)?.get_mut(&id) {
                            job.status["state"] = json!("failed");
                            job.status["error"] = serialized(&error)?;
                            job.native_id = None;
                        }
                        persist_control(app)?;
                    }
                    Err(error.with_detail("job_id", id))
                }
            }
        }
        "performance.receipts" => {
            let input: ReceiptRequest =
                serde_json::from_value(request.args.clone()).map_err(failure)?;
            Ok((
                "applied",
                serialized(performance_receipts(app, &input.id)?)?,
            ))
        }
        "performance.cancel" => {
            let input: CancelRequest =
                serde_json::from_value(request.args.clone()).map_err(failure)?;
            let c = app.state::<Control>();
            let native_id = {
                let jobs = c.jobs.lock().map_err(failure)?;
                let job = jobs
                    .get(&input.id)
                    .ok_or_else(|| ControlError::new("job_not_found", "Performance not found"))?;
                if job.status["mode"] != "performance" {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "This job is not a typed performance",
                    ));
                }
                job.native_id
            };
            if let Some(id) = native_id {
                engine
                    .cancel_scheduled_batch(id, input.stabilize)
                    .map_err(failure)?;
                let fence = engine.fence().map_err(failure)?;
                engine
                    .wait_for_fence(fence, std::time::Duration::from_millis(request.timeout_ms))
                    .map_err(failure)?;
            }
            Ok((
                "applied",
                serialized(performance_receipts(app, &input.id)?)?,
            ))
        }
        _ => unreachable!(),
    }
}

fn performance_session_endpoint(
    app: &tauri::AppHandle,
    observation: &mut control_protocol::performance::Observation,
) -> Result<()> {
    if let Some(session) = app
        .state::<Control>()
        .mix_session
        .lock()
        .map_err(failure)?
        .as_ref()
        .filter(|s| s.active())
    {
        let remaining =
            (session.duration.as_secs_f64() - session.started.elapsed().as_secs_f64()).max(0.);
        observation.session = Some(control_protocol::performance::SessionEndpoint {
            session_id: session.id.clone(),
            finish_unix_ms: session
                .started_unix_ms
                .saturating_add(session.duration.as_millis() as u64),
            endpoint_seconds: remaining,
        });
    }
    Ok(())
}
fn performance_session_guard(
    app: &tauri::AppHandle,
    plan: &control_protocol::performance::Plan,
) -> Result<()> {
    let Some(closing) = &plan.closing else {
        return Ok(());
    };
    let snapshot = app.state::<AppState>().engine.transport_snapshot();
    let elapsed = snapshot
        .rendered_frames
        .saturating_sub(plan.generation.observed_rendered_frames) as f64
        / snapshot.sample_rate as f64;
    let sessions = app.state::<Control>();
    let stored = sessions.mix_session.lock().map_err(failure)?;
    let session = stored.as_ref().filter(|s| s.active());
    if let Some(id) = &closing.session_id {
        if session.is_none_or(|s| s.id != *id) {
            return Err(ControlError::new(
                "session_changed",
                "Closing references a different or inactive session",
            ));
        }
    }
    if let Some(session) = session {
        let remaining =
            (session.duration.as_secs_f64() - session.started.elapsed().as_secs_f64()).max(0.);
        if closing.endpoint_seconds - elapsed > remaining + 0.05 {
            return Err(ControlError::new(
                "closing_deadline",
                "The closing endpoint extends beyond the active session's fixed endpoint",
            ));
        }
    }
    Ok(())
}

fn performance_duplicate_receipt(
    control: &Control,
    operation: &str,
    fingerprint: &str,
) -> Result<Option<control_protocol::performance::CommitReceipt>> {
    let jobs = control.jobs.lock().map_err(failure)?;
    let Some((id, job)) = jobs
        .iter()
        .find(|(_, job)| job.status["operation_id"] == operation)
    else {
        return Ok(None);
    };
    if job.fingerprint.as_deref() != Some(fingerprint) {
        // Older journals fingerprinted the input JSON before typed defaults and
        // A/B aliases were normalized. Compare the stored typed score as well;
        // this resolves an old identical uncertain commit, never a changed score.
        let canonical = (job.status["mode"] == "performance")
            .then(|| {
                serde_json::from_value::<control_protocol::performance::Plan>(
                    job.status["plan"].clone(),
                )
                .ok()
            })
            .flatten()
            .map(|plan| queue_fingerprint(&json!({"plan":plan})));
        if canonical.as_deref() != Some(fingerprint) {
            return Err(ControlError::new(
                "idempotency_conflict",
                "Operation ID was already used for a different score",
            ));
        }
    }
    Ok(Some(control_protocol::performance::CommitReceipt {
        job_id: id.clone(),
        operation_id: operation.into(),
        lifecycle: job.status["state"].as_str().unwrap_or("pending").into(),
        validation: serde_json::from_value(job.status["validation"].clone()).map_err(failure)?,
        duplicate: true,
        replayed: false,
    }))
}
#[cfg(test)]
mod performance_receipt_tests {
    use super::*;
    fn ticket_plan() -> control_protocol::performance::Plan {
        serde_json::from_value(json!({"operation_id":"ticket-once","generation":{
            "engine_epoch":1,"load_revisions":[1,1],"grid_revisions":[1,1],
            "position_revisions":[1,1],"control_revision":1,"tempo_percent":[0.,0.],
            "observed_rendered_frames":0,"sample_rate":48000},"actions":[],
            "recovery":{"carrier":0,"seconds":1.,"commands":[]}}))
        .unwrap()
    }
    #[test]
    fn ticket_resolution_preserves_exact_score_and_survives_committed_draft_eviction() {
        use control_protocol::performance::CommitRequest;
        let control = Control::default();
        let plan = ticket_plan();
        control
            .performance_drafts
            .lock()
            .unwrap()
            .push_back(PerformanceDraft {
                ticket: "reviewed".into(),
                plan: plan.clone(),
            });
        let (resolved, ticket) = performance_commit_plan(
            &control,
            CommitRequest {
                ticket: Some("reviewed".into()),
                plan: None,
            },
        )
        .unwrap();
        assert_eq!(serialized(&resolved).unwrap(), serialized(&plan).unwrap());
        assert_eq!(ticket.as_deref(), Some("reviewed"));
        control.performance_drafts.lock().unwrap().clear();
        control.jobs.lock().unwrap().insert(
            "job-ticket".into(),
            Job {
                cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                native_id: Some(1),
                fingerprint: None,
                status: json!({"mode":"performance","ticket":"reviewed","plan":plan}),
            },
        );
        let (restored, _) = performance_commit_plan(
            &control,
            CommitRequest {
                ticket: Some("reviewed".into()),
                plan: None,
            },
        )
        .unwrap();
        assert_eq!(serialized(&restored).unwrap(), serialized(&plan).unwrap());
        assert!(control.jobs.try_lock().is_ok());
        let validation = control_protocol::performance::Validation {
            valid: true,
            operation_id: plan.operation_id.clone(),
            actions: vec![],
            native_steps: 0,
            coverage_seconds: None,
            forecast: None,
            ticket: Some("reviewed".into()),
            arm_within_seconds: None,
        };
        {
            let mut jobs = control.jobs.lock().unwrap();
            let job = jobs.get_mut("job-ticket").unwrap();
            job.fingerprint = Some("legacy-unnormalized-body".into());
            job.status["operation_id"] = json!(plan.operation_id);
            job.status["validation"] = json!(validation);
        }
        performance_bind_ticket(&control, "job-ticket", "second-review-ticket").unwrap();
        let (alias_plan, _) = performance_commit_plan(
            &control,
            CommitRequest {
                ticket: Some("second-review-ticket".into()),
                plan: None,
            },
        )
        .unwrap();
        assert_eq!(serialized(&alias_plan).unwrap(), serialized(&plan).unwrap());
        let fingerprint = queue_fingerprint(&json!({"plan":plan}));
        assert!(
            performance_duplicate_receipt(&control, &plan.operation_id, &fingerprint)
                .unwrap()
                .unwrap()
                .duplicate
        );
        assert_eq!(
            performance_duplicate_receipt(&control, &plan.operation_id, "changed-score")
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
        assert!(performance_commit_plan(
            &control,
            CommitRequest {
                ticket: Some("reviewed".into()),
                plan: Some(ticket_plan())
            }
        )
        .is_err());
        assert_eq!(
            performance_commit_plan(
                &control,
                CommitRequest {
                    ticket: Some("expired".into()),
                    plan: None
                }
            )
            .unwrap_err()
            .code,
            "ticket_not_found"
        );
    }
    #[test]
    fn reviewed_draft_deadline_is_remaining_time_and_history_is_bounded() {
        use control_protocol::performance::{CommitRequest, ResolvedAction, Validation};
        let control = Control::default();
        let mut first = None;
        let mut last = None;
        for _ in 0..65 {
            let mut validation = Validation {
                valid: true,
                operation_id: "ticket-once".into(),
                actions: vec![ResolvedAction {
                    id: "future".into(),
                    start_seconds: 3.4,
                    end_seconds: 4.,
                    native_action_ids: vec![1],
                }],
                native_steps: 1,
                coverage_seconds: None,
                forecast: None,
                ticket: None,
                arm_within_seconds: None,
            };
            performance_save_draft(&control, ticket_plan(), &mut validation).unwrap();
            assert_eq!(validation.arm_within_seconds, Some(3.4));
            if first.is_none() {
                first = validation.ticket.clone();
            }
            last = validation.ticket;
        }
        assert_eq!(control.performance_drafts.lock().unwrap().len(), 64);
        assert_eq!(
            performance_commit_plan(
                &control,
                CommitRequest {
                    plan: None,
                    ticket: first
                }
            )
            .unwrap_err()
            .code,
            "ticket_not_found"
        );
        assert!(performance_commit_plan(
            &control,
            CommitRequest {
                plan: None,
                ticket: last
            }
        )
        .is_ok());
    }
    #[test]
    fn performance_duplicate_releases_job_lock_and_returns_same_typed_receipt() {
        let control = Control::default();
        let validation = control_protocol::performance::Validation {
            valid: true,
            operation_id: "once".into(),
            actions: vec![],
            native_steps: 1,
            coverage_seconds: None,
            forecast: None,
            ticket: None,
            arm_within_seconds: None,
        };
        control.jobs.lock().unwrap().insert("job-once".into(),Job{cancel:Arc::new(std::sync::atomic::AtomicBool::new(false)),native_id:Some(1),fingerprint:Some("body".into()),status:json!({"mode":"performance","state":"completed","operation_id":"once","validation":validation})});
        for _ in 0..2 {
            let receipt = performance_duplicate_receipt(&control, "once", "body")
                .unwrap()
                .unwrap();
            assert_eq!(receipt.job_id, "job-once");
            assert!(receipt.duplicate && !receipt.replayed);
            assert!(control.jobs.try_lock().is_ok());
        }
        assert_eq!(
            performance_duplicate_receipt(&control, "once", "different")
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
    }
}
