// Scheduling/journaling extension of Control.jobs, included into control.rs.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScheduledRequest {
    #[serde(default)]
    action_id: Option<String>,
    command: String,
    #[serde(default = "empty_args")]
    args: Value,
    expected_load_revision: Option<u64>,
    at_event: i64,
    #[serde(default)]
    deadline_event: Option<i64>,
    #[serde(default)]
    require: Option<deck_engine::BatchPrerequisite>,
}
fn empty_args() -> Value {
    json!({})
}
#[cfg(test)]
fn scheduled_commands(
    request: &Request,
) -> Result<(
    deck_engine::BatchSchedule,
    Vec<deck_engine::ScheduledCommand>,
)> {
    scheduled_commands_at(request, None)
}
fn scheduled_commands_at(
    request: &Request,
    snapshot: Option<&deck_engine::EngineSnapshot>,
) -> Result<(
    deck_engine::BatchSchedule,
    Vec<deck_engine::ScheduledCommand>,
)> {
    let args = capture_schedule_guards(&request.args, snapshot)?;
    let schedule: deck_engine::BatchSchedule = arg(&args, "schedule")?;
    let steps: Vec<ScheduledRequest> = arg(&args, "steps")?;
    let mut commands = Vec::with_capacity(steps.len());
    for (index, step) in steps.into_iter().enumerate() {
        if step
            .action_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
        {
            return Err(ControlError::new(
                "invalid_arguments",
                "action_id must be 1..128 bytes",
            ));
        }
        let mut native = Request::new(&step.command, step.args.clone());
        native.expected_load_revision = step.expected_load_revision;
        native.validate()?;
        control_protocol::validate_arguments(&native)?;
        let mut command = queued_engine_command(&native)?.ok_or_else(|| {
            ControlError::new(
                "command_unsupported",
                "Scheduled steps support engine.command and automation.ramp",
            )
            .with_detail("step_index", index)
        })?;
        if let (EngineCommand::Play { deck }, Some(snapshot)) = (&command, snapshot) {
            command = EngineCommand::PlayQuantized {
                deck: *deck,
                clock: schedule.clock,
                clock_revision: schedule.expected_load_revisions[schedule.clock],
                at_seconds: Some(
                    deck_engine::event_seconds(snapshot, schedule.clock, step.at_event)
                        .map_err(scheduled_failure)?,
                ),
                quantize: 1.,
            };
        }
        let require = step.require.or_else(|| {
            snapshot.and_then(|snapshot| {
                if matches!(
                    &command,
                    EngineCommand::SetChannelFader { .. }
                        | EngineCommand::Ramp {
                            target: deck_engine::ControlTarget::ChannelFader { .. },
                            ..
                        }
                ) {
                    let carrier = schedule
                        .completion_carrier
                        .unwrap_or(schedule.recovery.carrier);
                    snapshot
                        .decks
                        .get(carrier)
                        .map(|deck| deck_engine::BatchPrerequisite {
                            deck: carrier,
                            playing: Some(true),
                            ready: true,
                            advanced_from_seconds: Some(deck.position_seconds),
                            min_advanced_beats: 1.,
                            ..Default::default()
                        })
                } else {
                    None
                }
            })
        });
        if let Some(d) = command.deck() {
            if d > 1
                || native
                    .expected_load_revision
                    .is_some_and(|r| r != schedule.expected_load_revisions[d])
            {
                return Err(ControlError::new(
                    "source_changed",
                    "Scheduled step load revision differs from its queue source guard",
                )
                .with_detail("step_index", index));
            }
        }
        commands.push(deck_engine::ScheduledCommand {
            at_event: step.at_event,
            deadline_event: step.deadline_event.unwrap_or(step.at_event),
            command,
            require,
        });
    }
    Ok((schedule, commands))
}
fn scheduled_failure(e: impl ToString) -> ControlError {
    let s = e.to_string();
    let l = s.to_lowercase();
    let code = if l.contains("arming deadline") {
        "deadline_expired"
    } else if l.contains("loaded source") {
        "source_changed"
    } else if l.contains("position revision") {
        "position_changed"
    } else if l.contains("grid revision") {
        "grid_changed"
    } else if l.contains("control revision") {
        "manual_takeover"
    } else if l.contains("engine epoch") {
        "engine_epoch_changed"
    } else if l.contains("resource conflict") || l.contains("overlapping") {
        "resource_conflict"
    } else {
        "invalid_arguments"
    };
    ControlError::new(code, s)
}
fn persist_control(app: &tauri::AppHandle) -> Result<()> {
    let c = app.state::<Control>();
    let path = c.journal.lock().map_err(failure)?.clone();
    let Some(path) = path else {
        return Ok(());
    };
    let _writer = c.journal_writer.lock().map_err(failure)?;
    let jobs:Vec<Value>=c.jobs.lock().map_err(failure)?.iter().map(|(id,j)|json!({"id":id,"status":j.status,"native_id":j.native_id,"fingerprint":j.fingerprint})).collect();
    let receipts: Vec<_> = c
        .completed
        .lock()
        .map_err(failure)?
        .iter()
        .rev()
        .take(64)
        .cloned()
        .collect();
    let events: Vec<_> = c.events.lock().map_err(failure)?.iter().cloned().collect();
    let moves: Vec<_> = c
        .mix_prepared
        .lock()
        .map_err(failure)?
        .values()
        .map(mix_summary)
        .collect();
    let successor_queue = c
        .mix_queue
        .lock()
        .map_err(failure)?
        .as_ref()
        .map(|q| q.record.summary());
    let document = json!({"version":1,"successor_queue":successor_queue,"moves":moves,"jobs":jobs,"receipts":receipts,"events":events,"event_sequence":c.event_sequence.load(Ordering::Acquire),"performance_baseline":c.performance_baseline.lock().map_err(failure)?.clone()});
    // Serializes only off the renderer and serializes concurrent writers too.
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec(&document).map_err(failure)?;
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(failure)?;
        file.write_all(&bytes).map_err(failure)?;
        file.sync_all().map_err(failure)?;
    }
    std::fs::rename(&temporary, &path).map_err(failure)?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(failure)?;
    }
    Ok(())
}
fn restore_control(app: &tauri::AppHandle, path: std::path::PathBuf) -> Result<()> {
    let c = app.state::<Control>();
    *c.journal.lock().map_err(failure)? = Some(path.clone());
    if !path.exists() {
        return Ok(());
    }
    let data = std::fs::read(path).map_err(failure)?;
    if data.len() > 32 * 1024 * 1024 {
        return Err(ControlError::new(
            "journal_invalid",
            "Control journal exceeds limit",
        ));
    }
    let document: Value = serde_json::from_slice(&data).map_err(failure)?;
    if document["version"] != 1 {
        return Err(ControlError::new(
            "journal_invalid",
            "Unknown control journal version",
        ));
    }
    let jobs = restored_job_history(&document["jobs"])?;
    let next_sequence = jobs
        .iter()
        .map(|(id, job)| job_history_sequence(id, job))
        .max()
        .unwrap_or(0);
    *c.jobs.lock().map_err(failure)? = jobs;
    if let Ok(receipts) =
        serde_json::from_value::<Vec<(String, String, Response)>>(document["receipts"].clone())
    {
        *c.completed.lock().map_err(failure)? = receipts.into_iter().rev().collect();
    }
    *c.events.lock().map_err(failure)? = document["events"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(512)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    *c.performance_baseline.lock().map_err(failure)? = document
        .get("performance_baseline")
        .filter(|v| !v.is_null())
        .cloned();
    c.event_sequence.store(
        document["event_sequence"].as_u64().unwrap_or(0),
        Ordering::Release,
    );
    // IDs do not collide with restored jobs; request receipts retain identity.
    c.job_sequence.store(next_sequence, Ordering::Release);
    restore_mix_operations(app, &document["moves"])?;
    restore_successor_queue(app, &document["successor_queue"])?;
    Ok(())
}
fn refresh_scheduled_jobs(app: &tauri::AppHandle) -> Result<()> {
    let snapshot = app
        .state::<AppState>()
        .engine
        .transport_snapshot()
        .scheduled_batch;
    if snapshot.id == 0 {
        return Ok(());
    }
    let control = app.state::<Control>();
    let mut emitted = Vec::new();
    let mut changed = false;
    {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        for (id, job) in jobs
            .iter_mut()
            .filter(|(_, job)| job.native_id == Some(snapshot.id))
        {
            let current = serialized(&snapshot)?;
            if job.status["mode"] == "commands" {
                if reconcile_command_receipts(&mut job.status, &current, id, &mut emitted)? {
                    changed = true;
                }
                continue;
            }

            let mut previous = job.status["native"].clone();
            // Render-frame progress is deliberately not a journal/event hot path.
            if previous.is_object() {
                previous["frame"] = current["frame"].clone();
            }
            if previous == current {
                continue;
            }
            let old_state = job.status["state"].clone();
            let old_revision = job.status["revision"].clone();
            reconcile_queue_status(&mut job.status, &current, &mut emitted, id)?;
            job.status["native"] = current;
            job.status["state"] = serialized(snapshot.state)?;
            job.status["lifecycle"] = serialized(snapshot.state)?;
            job.status["reason"] = serialized(snapshot.reason)?;
            job.status["completed"] = json!(snapshot.completed);
            if terminal_job(job) && job.status.get("finished_unix_ms").is_none() {
                job.status["finished_unix_ms"] = json!(unix_ms());
            }
            if old_state != job.status["state"] || old_revision != job.status["revision"] {
                emitted.push(("job.updated",json!({"id":id,"state":job.status["state"],"reason":job.status["reason"],"revision":job.status["revision"],"completed":snapshot.completed,"frame":snapshot.frame})));
            }
            changed = true;
        }
    }
    for (kind, data) in emitted {
        event(app, kind, data);
    }
    if changed {
        persist_control(app)?;
    }
    Ok(())
}
fn reconcile_queue_status(
    status: &mut Value,
    native: &Value,
    emitted: &mut Vec<(&'static str, Value)>,
    id: &str,
) -> Result<()> {
    let mut accepted_actions = Vec::new();
    if let Some(patches) = status["patches"].as_array_mut() {
        for patch in patches
            .iter_mut()
            .filter(|patch| patch["state"] == "submitted")
        {
            if let Some(receipt) = native["updates"].as_array().and_then(|updates| {
                updates
                    .iter()
                    .find(|receipt| receipt["patch_id"] == patch["native_patch_id"])
            }) {
                let accepted = receipt["accepted"] == true;
                patch["state"] = json!(if accepted { "accepted" } else { "rejected" });
                patch["receipt"] = receipt.clone();
                patch["revision"] = receipt["revision"].clone();
                if accepted {
                    accepted_actions
                        .extend(patch["actions"].as_array().cloned().unwrap_or_default());
                }
                emitted.push(("job.patch",json!({"id":id,"patch_id":patch["patch_id"],"revision":receipt["revision"],"accepted":accepted,"reason":receipt["reason"]})));
            }
        }
    }
    if let Some(actions) = status["actions"].as_array_mut() {
        actions.extend(accepted_actions);
        for action in actions.iter_mut() {
            if let Some(receipt) = native["events"].as_array().and_then(|events| {
                events
                    .iter()
                    .find(|receipt| receipt["action_id"] == action["native_action_id"])
            }) {
                let state = match receipt["state"].as_u64().unwrap_or(0) {
                    1 => "applied",
                    2 => "verified",
                    3 => "cancelled",
                    _ => "pending",
                };
                if action["state"] != state {
                    action["state"] = json!(state);
                    action["receipt"] = receipt.clone();
                    emitted.push(("job.action",json!({"id":id,"action_id":action["action_id"],"state":state,"revision":native["revision"],"frame":receipt["frame"],"completed_frame":receipt["completed_frame"],"clock_seconds":receipt["clock_seconds"]})));
                }
            }
        }
        let next = actions
            .iter()
            .filter(|action| action["state"] == "pending")
            .filter_map(|action| action["at_event"].as_i64())
            .min()
            .map_or(Value::Null, |event| json!(event));
        let total = actions.len();
        status["next_due_event"] = next;
        status["total"] = json!(total);
    }
    if !native["revision"].is_null() {
        status["revision"] = native["revision"].clone();
    }
    if !native["open"].is_null() {
        status["open"] = native["open"].clone();
    }
    Ok(())
}
fn start_scheduled_batch(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    refresh_scheduled_jobs(app)?;
    let operation: String = arg(&request.args["schedule"], "operation_id")?;
    let fingerprint = queue_fingerprint(&request.args);
    let c = app.state::<Control>();
    if let Some(receipt) =
        scheduled_duplicate(&*c.jobs.lock().map_err(failure)?, &operation, &fingerprint)?
    {
        return Ok(("accepted", receipt));
    }
    let snapshot = app
        .state::<AppState>()
        .engine
        .scheduled_snapshot()
        .map_err(scheduled_failure)?;
    let (schedule, commands) = scheduled_commands_at(request, Some(&snapshot))?;
    let actions = queue_actions(&request.args["steps"], schedule.open, 0)?;
    let native_id = c.job_sequence.fetch_add(1, Ordering::Relaxed) + 1;
    let id = format!("job-{}-{}", std::process::id(), native_id);
    // Persist intent before arming. If acknowledgement becomes uncertain, the
    // operation ID still resolves to this job and no duplicate launch is sent.
    insert_job_bounded(
        &mut *c.jobs.lock().map_err(failure)?,
        id.clone(),
        Job {
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            native_id: Some(native_id),
            fingerprint: Some(fingerprint),
            status: json!({"mode":"scheduled","state":"armed","lifecycle":"armed","operation_id":schedule.operation_id,"schedule":schedule,"steps":request.args["steps"],"completed":0,"total":commands.len(),"revision":1,"open":schedule.open,"next_due_event":actions.iter().filter_map(|a|a["at_event"].as_i64()).min(),"actions":actions,"patches":[],"reason":null,"created_unix_ms":unix_ms(),"created_sequence":native_id,"sources":metadata_summary(&app.state::<AppState>().engine)}),
        },
    )?;
    if let Err(error) = persist_control(app) {
        if let Some(job) = c.jobs.lock().map_err(failure)?.get_mut(&id) {
            job.status["state"] = json!("failed");
            job.status["reason"] = json!("journal_unavailable");
            job.native_id = None;
        }
        return Err(error.with_detail("job_id", id));
    }
    let state = app.state::<AppState>();
    let result = state
        .engine
        .arm_scheduled_batch(native_id, &schedule, commands)
        .map_err(scheduled_failure)
        .and_then(|_| {
            let fence = state.engine.fence().map_err(failure)?;
            state
                .engine
                .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                .map_err(failure)?;
            let actual = state.engine.transport_snapshot().scheduled_batch;
            if actual.id != native_id {
                return Err(ControlError::new(
                    "resource_conflict",
                    "Atomic arm was rejected; another operation retains the native slot",
                ));
            }
            if actual.state == deck_engine::BatchLifecycle::Failed {
                return Err(ControlError::new(
                    "arm_failed",
                    format!("Atomic arm rejected: {:?}", actual.reason),
                )
                .with_detail("native", serialized(actual)?));
            }
            Ok(())
        });
    if let Err(error) = result {
        if error.code != "timeout" {
            if let Some(job) = c.jobs.lock().map_err(failure)?.get_mut(&id) {
                job.status["state"] = json!("failed");
                job.status["lifecycle"] = json!("failed");
                job.status["error"] = serialized(&error)?;
                job.native_id = None;
            }
            persist_control(app)?;
        }
        return Err(queue_error(error, &state.engine.transport_snapshot())
            .with_detail("job_id", id)
            .with_detail("operation_id", schedule.operation_id));
    }
    event(
        app,
        "job.updated",
        json!({"id":id,"state":"armed","revision":1,"operation_id":schedule.operation_id}),
    );
    refresh_scheduled_jobs(app)?;
    Ok((
        "accepted",
        json!({"job_id":id,"lifecycle":"armed","operation_id":schedule.operation_id,"native_id":native_id,"revision":1,"open":schedule.open,"event_sequence":c.event_sequence.load(Ordering::Acquire)}),
    ))
}

fn interrupted_status(mut status: Value) -> Value {
    if matches!(
        status["state"].as_str(),
        Some("armed" | "running" | "recovering")
    ) {
        status["state"] = json!("interrupted");
        status["reason"] = json!("app_restarted");
        status["lifecycle"] = json!("interrupted");
        status["finished_unix_ms"] = json!(unix_ms());
        if let Some(patches) = status["patches"].as_array_mut() {
            for patch in patches
                .iter_mut()
                .filter(|patch| patch["state"] == "submitted")
            {
                patch["state"] = json!("interrupted");
                patch["reason"] = json!("app_restarted");
            }
        }
        if let Some(actions) = status["actions"].as_array_mut() {
            for action in actions.iter_mut().filter(|action| {
                matches!(
                    action["state"].as_str(),
                    Some("pending" | "registered" | "queued" | "running" | "completing")
                )
            }) {
                action["state"] = json!("interrupted");
            }
        }
    }
    status
}
fn scheduled_duplicate(
    jobs: &HashMap<String, Job>,
    operation: &str,
    fingerprint: &str,
) -> Result<Option<Value>> {
    let Some((id, previous)) = jobs
        .iter()
        .find(|(_, j)| j.status["operation_id"] == operation)
    else {
        return Ok(None);
    };
    if previous.fingerprint.as_deref() != Some(fingerprint) {
        return Err(ControlError::new(
            "idempotency_conflict",
            "Operation ID was already used for a different command body",
        ));
    }
    Ok(Some(
        json!({"job_id":id,"lifecycle":previous.status["state"],"revision":previous.status["revision"],"duplicate":true,"replayed":false}),
    ))
}
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_millis() as u64)
}

const JOB_HISTORY_LIMIT: usize = 256;
fn terminal_job(job: &Job) -> bool {
    matches!(
        job.status["state"].as_str(),
        Some("completed" | "recovered" | "yielded" | "failed" | "interrupted" | "cancelled")
    )
}
fn job_history_sequence(id: &str, job: &Job) -> u64 {
    job.status["created_sequence"]
        .as_u64()
        .or_else(|| id.rsplit('-').next()?.parse().ok())
        .unwrap_or(0)
}
fn prune_job_history(jobs: &mut HashMap<String, Job>, limit: usize) -> Result<()> {
    let remove = jobs.len().saturating_sub(limit);
    if remove == 0 {
        return Ok(());
    }
    let mut terminal: Vec<_> = jobs
        .iter()
        .filter(|(_, job)| terminal_job(job))
        .map(|(id, job)| {
            (
                job.status["finished_unix_ms"]
                    .as_u64()
                    .or_else(|| job.status["created_unix_ms"].as_u64())
                    .unwrap_or(0),
                job_history_sequence(id, job),
                id.clone(),
            )
        })
        .collect();
    if terminal.len() < remove {
        return Err(ControlError::new(
            "busy",
            "Job capacity is occupied by active jobs; no active job can be evicted",
        ));
    }
    terminal.sort();
    for (_, _, id) in terminal.into_iter().take(remove) {
        jobs.remove(&id);
    }
    Ok(())
}
fn insert_job_bounded(jobs: &mut HashMap<String, Job>, id: String, job: Job) -> Result<()> {
    prune_job_history(jobs, JOB_HISTORY_LIMIT - 1)?;
    jobs.insert(id, job);
    Ok(())
}
fn restored_job_history(entries: &Value) -> Result<HashMap<String, Job>> {
    let mut jobs = HashMap::new();
    for entry in entries.as_array().into_iter().flatten() {
        let Some(id) = entry["id"].as_str() else {
            continue;
        };
        jobs.insert(
            id.into(),
            Job {
                cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                status: interrupted_status(entry["status"].clone()),
                native_id: None,
                fingerprint: entry["fingerprint"].as_str().map(str::to_owned),
            },
        );
    }
    prune_job_history(&mut jobs, JOB_HISTORY_LIMIT)?;
    Ok(jobs)
}

#[cfg(test)]
mod scheduled_control_tests {
    use super::*;
    fn history_job(sequence: u64, state: &str) -> Job {
        Job {
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            native_id: Some(sequence),
            fingerprint: Some(format!("plan-{sequence}")),
            status: json!({"state":state,"operation_id":format!("operation-{sequence}"),"created_unix_ms":sequence,"created_sequence":sequence}),
        }
    }
    #[test]
    fn full_history_admits_new_arm_preserves_active_and_recent_duplicate_on_restart() {
        let mut jobs = HashMap::new();
        jobs.insert("active-0".into(), history_job(0, "running"));
        for sequence in 1..256 {
            jobs.insert(
                format!("job-{sequence}"),
                history_job(sequence, "completed"),
            );
        }
        insert_job_bounded(&mut jobs, "new-arm-256".into(), history_job(256, "armed")).unwrap();
        assert_eq!(jobs.len(), JOB_HISTORY_LIMIT);
        assert!(jobs.contains_key("active-0"));
        assert!(!jobs.contains_key("job-1"));
        assert_eq!(
            scheduled_duplicate(&jobs, "operation-255", "plan-255")
                .unwrap()
                .unwrap()["job_id"],
            "job-255"
        );
        assert!(
            matches!(scheduled_duplicate(&jobs,"operation-255","different"),Err(e)if e.code=="idempotency_conflict")
        );
        let mut journal: Vec<_> = jobs
            .iter()
            .map(|(id, job)| json!({"id":id,"status":job.status,"fingerprint":job.fingerprint}))
            .collect();
        // Older journal entries arriving after the recent records must not displace them.
        journal.push(json!({"id":"old-terminal","status":{"state":"completed","created_unix_ms":0,"created_sequence":0}}));
        let restored = restored_job_history(&json!(journal)).unwrap();
        assert_eq!(restored.len(), JOB_HISTORY_LIMIT);
        assert!(!restored.contains_key("old-terminal"));
        let armed = scheduled_duplicate(&restored, "operation-256", "plan-256")
            .unwrap()
            .unwrap();
        assert_eq!(armed["lifecycle"], "interrupted");
        assert_eq!(armed["replayed"], false);
        assert!(restored.values().all(|j| j.native_id.is_none()));
        assert!(scheduled_duplicate(&restored, "operation-255", "plan-255")
            .unwrap()
            .is_some());
    }
    #[test]
    fn admission_never_evicts_active_or_unknown_states() {
        let mut jobs = HashMap::new();
        for sequence in 0..256 {
            jobs.insert(
                format!("job-{sequence}"),
                history_job(
                    sequence,
                    if sequence == 0 {
                        "unknown-future-state"
                    } else {
                        "recovering"
                    },
                ),
            );
        }
        assert!(
            matches!(insert_job_bounded(&mut jobs,"new".into(),history_job(257,"armed")),Err(e)if e.code=="busy")
        );
        assert_eq!(jobs.len(), JOB_HISTORY_LIMIT);
        assert!(jobs.contains_key("job-0"));
        assert!(!jobs.contains_key("new"));
    }
    #[test]
    fn repeated_completed_handoffs_do_not_exhaust_history() {
        let mut jobs = HashMap::new();
        jobs.insert("long-running".into(), history_job(0, "running"));
        for sequence in 1..=1000 {
            let id = format!("job-{sequence}");
            insert_job_bounded(&mut jobs, id.clone(), history_job(sequence, "armed")).unwrap();
            jobs.get_mut(&id).unwrap().status["state"] = json!("completed");
            assert!(jobs.contains_key("long-running"));
            assert!(jobs.len() <= JOB_HISTORY_LIMIT);
        }
        assert_eq!(jobs.len(), JOB_HISTORY_LIMIT);
        assert!(!jobs.contains_key("job-1"));
        assert!(scheduled_duplicate(&jobs, "operation-1000", "plan-1000")
            .unwrap()
            .is_some());
    }
    fn request() -> Request {
        Request::new(
            "batch.run",
            json!({
                "schedule":{"operation_id":"unit","clock":"A","expected_load_revisions":[1,2],"expected_position_revisions":[3,4],"expected_grid_revisions":[5,6],"expected_engine_epoch":8,"expected_control_revision":9,"arm_before_event":10,"completion_event":30,
                "recovery":{"carrier":"B","seconds":0.2,"controls":[{"type":"set_channel_fader","deck":"B","value":1},{"type":"set_channel_fader","deck":"A","value":0},{"type":"set_eq","deck":"B","low":1,"mid":1,"high":1}]}},
                "steps":[{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"clock":"A","beats":4},"expected_load_revision":2,"at_event":16,"require":{"deck":"B","playing":true,"ready":true,"advanced_from_seconds":0,"min_advanced_beats":1}}]
            }),
        )
    }
    #[test]
    fn documented_fixture_parses_the_exact_unified_step_contract() {
        let args: Value = serde_json::from_str(include_str!(
            "../../research/examples/scheduled-handover.fixture.json"
        ))
        .unwrap();
        let (schedule, commands) = scheduled_commands(&Request::new("batch.run", args)).unwrap();
        assert_eq!(commands.len(), 4);
        assert_eq!(schedule.completion_event, 24);
        assert!(matches!(
            commands[0].command,
            EngineCommand::PlayQuantized {
                deck: 1,
                clock: 0,
                clock_revision: 0,
                ..
            }
        ));
    }
    #[test]
    fn scheduling_uses_existing_native_command_and_deck_aliases() {
        let (s, c) = scheduled_commands(&request()).unwrap();
        assert_eq!(s.clock, 0);
        assert_eq!(s.recovery.carrier, 1);
        assert!(matches!(
            c[0].command,
            EngineCommand::Ramp {
                target: deck_engine::ControlTarget::ChannelFader { deck: 1 },
                clock: Some(0),
                ..
            }
        ));
        assert_eq!(c[0].deadline_event, 16);
    }
    #[test]
    fn schedule_only_fields_are_rejected_on_immediate_steps_and_standalone_commands() {
        let mut r = request();
        r.args.as_object_mut().unwrap().remove("schedule");
        assert!(serde_json::from_value::<Vec<Request>>(r.args["steps"].clone()).is_err());
        let r = Request::new(
            "automation.ramp",
            json!({"target":{"type":"channel_fader","deck":1},"value":1,"at_event":4}),
        );
        assert!(control_protocol::validate_arguments(&r).is_err());
    }
    #[test]
    fn every_deck_step_requires_declared_source_guard() {
        let mut r = request();
        r.args["steps"][0]["expected_load_revision"] = json!(1);
        assert_eq!(scheduled_commands(&r).unwrap_err().code, "source_changed");
    }
    #[test]
    fn unsupported_scheduled_business_command_is_not_ignored() {
        let mut r = request();
        r.args["steps"][0]["command"] = json!("status");
        r.args["steps"][0]["args"] = json!({});
        assert_eq!(
            scheduled_commands(&r).unwrap_err().code,
            "command_unsupported"
        );
    }
    #[test]
    fn restored_intent_is_interrupted_and_duplicate_never_replays() {
        let status = interrupted_status(
            json!({"state":"running","operation_id":"unit","schedule":{"intent":{"payoff":"retain"}},"native":{"completed":2}}),
        );
        assert_eq!(status["state"], "interrupted");
        assert_eq!(status["schedule"]["intent"]["payoff"], "retain");
        assert_eq!(status["native"]["completed"], 2);
        let mut jobs = HashMap::new();
        jobs.insert(
            "saved-job".into(),
            Job {
                cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                status,
                native_id: None,
                fingerprint: Some("exact-plan".into()),
            },
        );
        let receipt = scheduled_duplicate(&jobs, "unit", "exact-plan")
            .unwrap()
            .unwrap();
        assert_eq!(receipt["job_id"], "saved-job");
        assert_eq!(receipt["lifecycle"], "interrupted");
        assert_eq!(receipt["replayed"], false);
        assert_eq!(
            scheduled_duplicate(&jobs, "unit", "changed-plan")
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
        assert!(scheduled_duplicate(&jobs, "new-operation", "exact-plan")
            .unwrap()
            .is_none());
    }
    #[test]
    fn dialog_and_path_are_exclusive() {
        assert!(dialog_selection(&json!({}), &["path"]).is_err());
        assert!(dialog_selection(&json!({"dialog":true,"path":"a"}), &["path"]).is_err());
        assert!(dialog_selection(&json!({"dialog":true}), &["path"]).unwrap());
        assert!(!dialog_selection(&json!({"path":"a"}), &["path"]).unwrap());
    }
}

/// Omitted guards capture one transport generation; explicit guards are retained.
fn capture_schedule_guards(
    args: &Value,
    snapshot: Option<&deck_engine::EngineSnapshot>,
) -> Result<Value> {
    let mut args = args.clone();
    let first = args["steps"]
        .as_array()
        .and_then(|steps| steps.iter().filter_map(|s| s["at_event"].as_i64()).min());
    let schedule = args["schedule"]
        .as_object_mut()
        .ok_or_else(|| ControlError::new("invalid_arguments", "schedule must be an object"))?;
    if let Some(snapshot) = snapshot {
        for (key, value) in [
            ("expected_load_revisions", json!(snapshot.load_revisions)),
            (
                "expected_position_revisions",
                json!(snapshot.decks.each_ref().map(|d| d.position_revision)),
            ),
            (
                "expected_grid_revisions",
                json!(snapshot.decks.each_ref().map(|d| d.grid_revision)),
            ),
            ("expected_engine_epoch", json!(snapshot.engine_epoch)),
            (
                "expected_control_revision",
                json!(snapshot.control_revision),
            ),
        ] {
            schedule.entry(key).or_insert(value);
        }
    }
    if let Some(first) = first {
        schedule.entry("arm_before_event").or_insert(json!(first));
    }
    Ok(args)
}
fn queue_actions(steps: &Value, require_ids: bool, previous: usize) -> Result<Vec<Value>> {
    let steps = steps
        .as_array()
        .ok_or_else(|| ControlError::new("invalid_arguments", "steps must be an array"))?;
    let mut actions = Vec::with_capacity(steps.len());
    let mut names = std::collections::HashSet::new();
    for (index, step) in steps.iter().enumerate() {
        let numeric = previous + index + 1;
        let name = match step.get("action_id") {
            Some(Value::String(id)) if !id.is_empty() && id.len() <= 128 => id.clone(),
            None if !require_ids => format!("action-{numeric}"),
            _ => {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "Open queue steps require unique action_id strings of 1..128 bytes",
                ))
            }
        };
        if !names.insert(name.clone()) {
            return Err(ControlError::new(
                "invalid_arguments",
                "Duplicate action_id",
            ));
        }
        actions.push(json!({"action_id":name,"native_action_id":numeric,"at_event":step["at_event"],"state":"pending","step":step}));
    }
    Ok(actions)
}
fn queue_error(error: ControlError, snapshot: &deck_engine::EngineSnapshot) -> ControlError {
    error.with_detail("current_positions", json!(snapshot.decks.each_ref().map(|d|json!({"seconds":d.position_seconds,"event":d.beat_position}))))
        .with_detail("earliest_accepted_events",json!(snapshot.decks.each_ref().map(|d|d.beat_position.map(|beat|beat.floor() as i64+1+if d.beatgrid_uniform {0} else {d.beatgrid_anchor_index.unwrap_or(0) as i64}))))
        .with_detail("queue", serialized(&snapshot.scheduled_batch).unwrap_or(Value::Null))
        .with_detail("hint", "Inspect jobs.get; append future events using its current revision. Retrying an existing patch_id returns its receipt without replay.")
}
fn job_status(status: &Value, detail: bool) -> Value {
    if detail {
        return status.clone();
    }
    if status["mode"] == "commands" {
        let mut compact = json!({});
        for key in [
            "mode",
            "state",
            "operation_id",
            "open",
            "revision",
            "completed",
            "total",
            "reason",
            "last_error",
            "created_unix_ms",
            "finished_unix_ms",
        ] {
            if let Some(value) = status.get(key) {
                compact[key] = value.clone();
            }
        }
        let actions = status["actions"].as_array().map_or(&[][..], Vec::as_slice);
        let recent: std::collections::HashSet<_> = actions
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, action)| action_terminal(action))
            .take(8)
            .map(|(index, _)| index)
            .collect();
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        let mut selected = Vec::new();
        for (index, action) in actions.iter().enumerate() {
            *counts
                .entry(action["state"].as_str().unwrap_or("unknown").to_owned())
                .or_default() += 1;
            if action_terminal(action) && !recent.contains(&index) {
                continue;
            }
            let mut summary = json!({});
            for key in [
                "action_id",
                "state",
                "when",
                "after",
                "registered_unix_ms",
                "completed_unix_ms",
                "receipt",
                "result",
                "members",
            ] {
                if let Some(value) = action.get(key) {
                    summary[key] = value.clone();
                }
            }
            for key in ["command", "args", "expected_load_revision", "request_id"] {
                if let Some(value) = action["request"].get(key) {
                    summary[key] = value.clone();
                }
            }
            if let Some(members) = action["requests"].as_array() {
                summary.as_object_mut().unwrap().remove("command");
                summary.as_object_mut().unwrap().remove("args");
                summary["commands"] = json!(members
                    .iter()
                    .map(|request| json!({"command":request["command"],"args":request["args"]}))
                    .collect::<Vec<_>>());
            }
            if let Some(result) = summary.get_mut("result") {
                compact_control_result(result);
                if let Some(payload) = result.get_mut("result").and_then(Value::as_object_mut) {
                    payload.remove("engine");
                }
            }
            selected.push(summary);
        }
        compact["history_omitted"] = json!(actions.len() - selected.len());
        compact["counts"] = json!(counts);
        compact["actions"] = json!(selected);
        let patches = status["patches"].as_array().map_or(&[][..], Vec::as_slice);
        compact["patch_history_omitted"] = json!(patches.len().saturating_sub(8));
        compact["patches"] = json!(patches
            .iter()
            .skip(patches.len().saturating_sub(8))
            .map(|patch| {
                let mut summary = json!({});
                for key in ["patch_id", "state", "revision", "error"] {
                    if let Some(value) = patch.get(key) {
                        summary[key] = value.clone();
                    }
                }
                summary
            })
            .collect::<Vec<_>>());
        return compact;
    }
    let mut compact = status.clone();
    if let Some(results) = compact.get_mut("results") {
        compact_control_result(results);
    }
    if let Some(object) = compact.as_object_mut() {
        for key in ["steps", "sources", "fingerprint"] {
            object.remove(key);
        }
        if let Some(schedule) = object.get_mut("schedule").and_then(Value::as_object_mut) {
            for key in [
                "expected_load_revisions",
                "expected_position_revisions",
                "expected_grid_revisions",
                "expected_engine_epoch",
                "expected_control_revision",
                "recovery",
            ] {
                schedule.remove(key);
            }
        }
        if let Some(actions) = object.get_mut("actions").and_then(Value::as_array_mut) {
            for action in actions {
                if let Some(object) = action.as_object_mut() {
                    object.remove("step");
                }
            }
        }
        if let Some(patches) = object.get_mut("patches").and_then(Value::as_array_mut) {
            for patch in patches {
                if let Some(object) = patch.as_object_mut() {
                    object.remove("fingerprint");
                    object.remove("request");
                    object.remove("actions");
                }
            }
        }
    }
    compact
}
fn read_events(app: &tauri::AppHandle, request: &Request) -> Result<Value> {
    let control = app.state::<Control>();
    let since = optional::<u64>(&request.args, "since")?.unwrap_or(0);
    let job = optional::<String>(&request.args, "job_id")?;
    let operation = optional::<String>(&request.args, "operation_id")?;
    let queue = optional::<String>(&request.args, "queue_id")?;
    let types = optional::<Vec<String>>(&request.args, "types")?;
    let wait = optional::<u64>(&request.args, "wait_ms")?
        .unwrap_or(0)
        .min(60000)
        .min(request.timeout_ms.saturating_sub(50));
    let deadline = Instant::now() + Duration::from_millis(wait);
    let matches = |event: &&Value| {
        event["sequence"].as_u64().unwrap_or(0) > since
            && types
                .as_ref()
                .is_none_or(|types| types.iter().any(|kind| event["type"] == *kind))
            && operation.as_ref().is_none_or(|id| {
                event["type"] == "mix.updated" && event["data"]["operation_id"] == *id
            })
            && queue.as_ref().is_none_or(|id| {
                matches!(
                    event["type"].as_str(),
                    Some("mix.queue.updated" | "mix.queue.notice")
                ) && event["data"]["queue_id"] == *id
            })
            && job
                .as_ref()
                .is_none_or(|id| event["data"]["id"] == *id || event["data"]["job_id"] == *id)
    };
    let mut events = control.events.lock().map_err(failure)?;
    loop {
        let gap = since > control.event_sequence.load(Ordering::Acquire)
            || events
                .front()
                .and_then(|e| e["sequence"].as_u64())
                .is_some_and(|first| since > 0 && since.saturating_add(1) < first);
        let selected: Vec<_> = events.iter().filter(matches).cloned().collect();
        if !selected.is_empty() || gap || Instant::now() >= deadline {
            return Ok(
                json!({"events":selected,"resync_required":gap,"sequence":control.event_sequence.load(Ordering::Acquire),"timed_out":selected.is_empty() && !gap}),
            );
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        events = control
            .event_ready
            .wait_timeout(events, remaining)
            .map_err(failure)?
            .0;
    }
}

fn patch_duplicate(status: &Value, patch_id: &str, fingerprint: &str) -> Result<Option<Value>> {
    let Some(patch) = status["patches"]
        .as_array()
        .and_then(|patches| patches.iter().find(|p| p["patch_id"] == patch_id))
    else {
        return Ok(None);
    };
    if patch["fingerprint"] != fingerprint {
        return Err(ControlError::new(
            "idempotency_conflict",
            "patch_id was already used for different actions",
        ));
    }
    Ok(Some(
        json!({"patch_id":patch_id,"revision":status["revision"],"patch":patch_receipt(patch),"duplicate":true,"replayed":false}),
    ))
}
fn patch_receipt(patch: &Value) -> Value {
    let mut value = patch.clone();
    if let Some(object) = value.as_object_mut() {
        for key in ["fingerprint", "request", "actions"] {
            object.remove(key);
        }
    }
    value
}
fn patch_scheduled_batch(
    app: &tauri::AppHandle,
    request: &Request,
) -> Result<(&'static str, Value)> {
    refresh_scheduled_jobs(app)?;
    let id: String = arg(&request.args, "id")?;
    if app
        .state::<Control>()
        .jobs
        .lock()
        .map_err(failure)?
        .get(&id)
        .is_some_and(|job| job.status["mode"] == "commands")
    {
        return patch_command_job(app, request);
    }

    let patch_id: String = arg(&request.args, "patch_id")?;
    if patch_id.is_empty() || patch_id.len() > 128 {
        return Err(ControlError::new(
            "invalid_arguments",
            "patch_id must be 1..128 bytes",
        ));
    }
    let expected_revision: u64 = arg(&request.args, "expected_revision")?;
    let fingerprint = queue_fingerprint(&json!({"command":request.command,"args":request.args}));
    let control = app.state::<Control>();
    let (native_id, status) = {
        let jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs
            .get(&id)
            .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?;
        if let Some(mut receipt) = patch_duplicate(&job.status, &patch_id, &fingerprint)? {
            receipt["job_id"] = json!(id);
            return Ok(("accepted", receipt));
        }
        if terminal_job(job) || job.status["open"] != true {
            return Err(ControlError::new(
                "queue_closed",
                "Only an active open batch accepts patches",
            )
            .with_detail("job", job_status(&job.status, false)));
        }
        if job.status["revision"].as_u64() != Some(expected_revision) {
            return Err(
                ControlError::new("revision_conflict", "Queue revision changed")
                    .with_detail("current_revision", job.status["revision"].clone()),
            );
        }
        if job.status["patches"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["state"] == "submitted"))
        {
            return Err(ControlError::new(
                "busy",
                "Previous patch is awaiting its native receipt; inspect jobs.get",
            ));
        }
        (
            job.native_id.ok_or_else(|| {
                ControlError::new(
                    "queue_closed",
                    "Restored queues are interrupted and never replayed",
                )
            })?,
            job.status.clone(),
        )
    };
    let patches = status["patches"].as_array().cloned().unwrap_or_default();
    if patches.len() >= 128 {
        return Err(ControlError::new(
            "queue_full",
            "Queue mutation history limit reached",
        ));
    }
    let native_patch_id = patches.len() as u64 + 1;
    let existing = status["actions"].as_array().cloned().unwrap_or_default();
    let mut actions = Vec::new();
    let mut append = Vec::new();
    let mut invalidate = Vec::new();
    let mut seal = None;
    match request.command.as_str() {
        "batch.append" => {
            actions = queue_actions(&request.args["steps"], true, existing.len())?;
            if actions.iter().any(|action| {
                existing
                    .iter()
                    .any(|old| old["action_id"] == action["action_id"])
            }) {
                return Err(ControlError::new(
                    "idempotency_conflict",
                    "action_id already exists, including applied or invalidated actions",
                ));
            }
            let normalized = Request::new(
                "batch.run",
                json!({"schedule":status["schedule"],"steps":request.args["steps"]}),
            );
            let context = app
                .state::<AppState>()
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            let (_, commands) = scheduled_commands_at(&normalized, Some(&context))?;
            append = commands
                .into_iter()
                .zip(actions.iter())
                .map(|(step, action)| deck_engine::ScheduledQueueAction {
                    action_id: action["native_action_id"].as_u64().unwrap(),
                    step,
                })
                .collect();
        }
        "batch.invalidate" => {
            let ids: Vec<String> = arg(&request.args, "action_ids")?;
            invalidate = resolve_action_ids(&existing, ids)?;
        }
        "batch.seal" => {
            seal = Some(
                optional(&request.args, "completion_event")?
                    .unwrap_or(arg(&status["schedule"], "completion_event")?),
            )
        }
        _ => unreachable!(),
    }
    if request.command == "batch.append" {
        invalidate = resolve_action_ids(
            &existing,
            optional::<Vec<String>>(&request.args, "invalidate_action_ids")?.unwrap_or_default(),
        )?;
    }
    let patch = deck_engine::BatchQueuePatch {
        patch_id: native_patch_id,
        expected_revision,
        append,
        invalidate,
        seal,
    };
    let state = app.state::<AppState>();
    // The intent is durable before native submission. An uncertain acknowledgement
    // returns this record on retry; it never sends the command twice.
    {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs.get_mut(&id).unwrap();
        job.status["patches"].as_array_mut().unwrap().push(json!({"patch_id":patch_id,"native_patch_id":native_patch_id,"fingerprint":fingerprint,"expected_revision":expected_revision,"state":"submitted","request":request.args,"command":request.command,"actions":actions,"created_unix_ms":unix_ms()}));
    }
    if let Err(error) = persist_control(app) {
        if let Some(record) = control
            .jobs
            .lock()
            .map_err(failure)?
            .get_mut(&id)
            .and_then(|j| j.status["patches"].as_array_mut())
            .and_then(|p| p.last_mut())
        {
            record["state"] = json!("rejected");
            record["error"] = serialized(&error)?;
        }
        return Err(error
            .with_detail("job_id", id)
            .with_detail("patch_id", patch_id));
    }
    let result = state
        .engine
        .patch_scheduled_batch(native_id, patch)
        .map_err(scheduled_failure)
        .and_then(|_| {
            let fence = state.engine.fence().map_err(failure)?;
            state
                .engine
                .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                .map_err(failure)
        });
    if let Err(error) = result {
        if error.code != "timeout" {
            if let Some(record) = control
                .jobs
                .lock()
                .map_err(failure)?
                .get_mut(&id)
                .and_then(|j| j.status["patches"].as_array_mut())
                .and_then(|p| p.last_mut())
            {
                record["state"] = json!("rejected");
                record["error"] = serialized(&error)?;
            }
            persist_control(app)?;
        }
        return Err(queue_error(error, &state.engine.transport_snapshot())
            .with_detail("job_id", id)
            .with_detail("patch_id", patch_id));
    }
    refresh_scheduled_jobs(app)?;
    let jobs = control.jobs.lock().map_err(failure)?;
    let status = &jobs[&id].status;
    let record = status["patches"].as_array().unwrap().last().unwrap();
    if record["state"] == "rejected" {
        return Err(
            ControlError::new("patch_rejected", "Native queue rejected the mutation")
                .with_detail("receipt", patch_receipt(record))
                .with_detail("current_revision", status["revision"].clone()),
        );
    }
    Ok((
        "accepted",
        json!({"job_id":id,"patch_id":patch_id,"revision":status["revision"],"patch":patch_receipt(record),"event_sequence":control.event_sequence.load(Ordering::Acquire),"replayed":false}),
    ))
}

fn resolve_action_ids(actions: &[Value], ids: Vec<String>) -> Result<Vec<u64>> {
    ids.into_iter()
        .map(|id| {
            actions
                .iter()
                .find(|a| a["action_id"] == id)
                .and_then(|a| a["native_action_id"].as_u64())
                .ok_or_else(|| {
                    ControlError::new("invalid_arguments", format!("Unknown action_id: {id}"))
                })
        })
        .collect()
}

#[cfg(test)]
mod incremental_control_tests {
    use super::*;
    #[test]
    fn generated_incremental_discovery_has_resolvable_refs_and_optional_captured_guards() {
        fn refs(value: &Value, root: &Value) {
            match value {
                Value::Object(object) => {
                    if let Some(reference) = object
                        .get("$ref")
                        .and_then(Value::as_str)
                        .and_then(|v| v.strip_prefix('#'))
                    {
                        assert!(
                            root.pointer(reference).is_some(),
                            "Dangling schema ref {reference}"
                        );
                    }
                    for child in object.values() {
                        refs(child, root);
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        refs(value, root);
                    }
                }
                _ => {}
            }
        }
        for entry in catalog().into_iter().filter(|entry| {
            matches!(
                entry["command"].as_str(),
                Some("batch.run" | "batch.validate" | "batch.append")
            )
        }) {
            refs(&entry["input_schema"], &entry["input_schema"]);
            if entry["command"] != "batch.append" {
                assert!(!entry["input_schema"]["properties"]["schedule"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("expected_load_revisions")));
            }
        }
    }
    #[test]
    fn simple_scheduled_play_and_faders_compile_with_mechanical_guards() {
        let engine = deck_engine::EngineHandle::new().unwrap();
        let mut snapshot = engine.transport_snapshot();
        for deck in &mut snapshot.decks {
            deck.bpm = Some(120.);
            deck.beatgrid_valid = true;
            deck.beatgrid_uniform = true;
            deck.position_seconds = 1.;
            deck.track=Some(serde_json::from_value(json!({"id":"fixture","path":"fixture","title":"Fixture","artist":null,"duration_seconds":200.,"sample_rate":48000,"channels":2,"bpm":120.,"bpm_confidence":1.,"key":null,"key_confidence":null,"camelot":null,"waveform":[]})).unwrap());
        }
        let request = Request::new(
            "batch.run",
            json!({"schedule":{"operation_id":"simple","clock":"A","open":true,"expiry_seconds":60.,"completion_event":64,"completion_carrier":"B","recovery":{"carrier":"A","seconds":0.2,"controls":[{"type":"set_channel_fader","deck":"A","value":1},{"type":"set_channel_fader","deck":"B","value":0}]}},"steps":[{"action_id":"launch","command":"engine.command","args":{"type":"play","deck":"B"},"at_event":16},{"action_id":"fade","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":1},"at_event":24}]}),
        );
        let (schedule, commands) = scheduled_commands_at(&request, Some(&snapshot)).unwrap();
        assert_eq!(schedule.expected_load_revisions, snapshot.load_revisions);
        assert!(
            matches!(commands[0].command,EngineCommand::PlayQuantized{deck:1,clock:0,at_seconds:Some(at),..} if (at-8.).abs()<1e-8)
        );
        let proof = commands[1].require.unwrap();
        assert_eq!(proof.deck, 1);
        assert_eq!(proof.playing, Some(true));
        assert!(proof.ready);
        assert_eq!(proof.advanced_from_seconds, Some(1.));
        assert_eq!(proof.min_advanced_beats, 1.);
    }
    #[test]
    fn queue_guards_capture_coherently_without_overwriting_explicit_proof() {
        let engine = deck_engine::EngineHandle::new().unwrap();
        let snapshot = engine.transport_snapshot();
        let args = json!({"schedule":{"operation_id":"open","clock":"A","expected_engine_epoch":987},"steps":[{"at_event":64},{"at_event":32}]});
        let normalized = capture_schedule_guards(&args, Some(&snapshot)).unwrap();
        assert_eq!(normalized["schedule"]["expected_engine_epoch"], 987);
        assert_eq!(
            normalized["schedule"]["expected_load_revisions"],
            json!(snapshot.load_revisions)
        );
        assert_eq!(
            normalized["schedule"]["expected_position_revisions"],
            json!(snapshot.decks.each_ref().map(|d| d.position_revision))
        );
        assert_eq!(normalized["schedule"]["arm_before_event"], 32);
    }
    #[test]
    fn uncertain_patch_retry_and_restart_never_replay_or_erase_receipts() {
        let mut status = json!({"state":"running","revision":1,"open":true,"actions":[{"action_id":"launch","native_action_id":1,"at_event":16,"state":"applied"}],"patches":[{"patch_id":"phase-2","native_patch_id":1,"fingerprint":"same","state":"submitted","actions":[{"action_id":"fade","native_action_id":2,"at_event":32,"state":"pending","step":{"large":"plan"}}]}]});
        let receipt = patch_duplicate(&status, "phase-2", "same")
            .unwrap()
            .unwrap();
        assert_eq!(receipt["duplicate"], true);
        assert_eq!(receipt["replayed"], false);
        assert_eq!(
            patch_duplicate(&status, "phase-2", "changed")
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
        let native = json!({"revision":2,"open":true,"updates":[{"patch_id":1,"revision":2,"accepted":true,"reason":"none"}],"events":[{"action_id":1,"state":2,"frame":100,"clock_seconds":8},{"action_id":2,"state":3,"frame":102,"clock_seconds":8.2}]});
        let mut emitted = Vec::new();
        reconcile_queue_status(&mut status, &native, &mut emitted, "job").unwrap();
        assert_eq!(status["revision"], 2);
        assert_eq!(status["actions"][0]["state"], "verified");
        assert_eq!(status["actions"][1]["state"], "cancelled");
        assert_eq!(status["next_due_event"], Value::Null);
        let mut again = Vec::new();
        reconcile_queue_status(&mut status, &native, &mut again, "job").unwrap();
        assert!(again.is_empty());
        assert_eq!(status["actions"].as_array().unwrap().len(), 2);
        let interrupted = interrupted_status(status);
        assert_eq!(interrupted["state"], "interrupted");
        let receipt = patch_duplicate(&interrupted, "phase-2", "same")
            .unwrap()
            .unwrap();
        assert_eq!(receipt["patch"]["state"], "accepted");
        assert_eq!(receipt["replayed"], false);
        let compact = job_status(&interrupted, false);
        assert!(compact["actions"][1].get("step").is_none());
        assert!(compact["patches"][0].get("fingerprint").is_none());
    }
    #[test]
    fn stable_action_names_cannot_be_duplicated_or_forgotten() {
        assert!(queue_actions(&json!([{"at_event":8}]), true, 0).is_err());
        assert!(queue_actions(&json!([{"action_id":"x"},{"action_id":"x"}]), true, 0).is_err());
        let actions = queue_actions(
            &json!([{"action_id":"launch","at_event":8},{"action_id":"fade","at_event":16}]),
            true,
            4,
        )
        .unwrap();
        assert_eq!(actions[0]["native_action_id"], 5);
        assert_eq!(
            resolve_action_ids(&actions, vec!["fade".into()]).unwrap(),
            vec![6]
        );
        assert!(resolve_action_ids(&actions, vec!["unknown".into()]).is_err());
    }
    #[test]
    fn rejected_native_patch_does_not_install_candidate_actions_or_advance_revision() {
        let mut status = json!({"revision":4,"open":true,"actions":[],"patches":[{"patch_id":"bad","native_patch_id":2,"fingerprint":"f","state":"submitted","actions":[{"action_id":"nope"}]}]});
        reconcile_queue_status(&mut status,&json!({"revision":4,"open":true,"updates":[{"patch_id":2,"revision":4,"accepted":false,"reason":"revision_conflict"}],"events":[]}),&mut vec![],"job").unwrap();
        assert_eq!(status["revision"], 4);
        assert_eq!(status["patches"][0]["state"], "rejected");
        assert!(status["actions"].as_array().unwrap().is_empty());
    }
}

fn queue_fingerprint(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[cfg(test)]
mod compact_job_tests {
    use super::*;
    #[test]
    fn command_status_is_bounded_and_retains_pending_actions_without_mutating_full_history() {
        let mut actions:Vec<_>=(0..1000).map(|index|json!({"action_id":format!("done-{index}"),"state":"executed","ordinal":index,"completion_context":{"large":"private"},"request":{"command":"status","args":{}},"result":{"ok":true}})).collect();
        actions.push(json!({"action_id":"pending","state":"queued","native_done":false,"request":{"command":"engine.command","args":{"type":"play","deck":"B"}},"when":{"type":"after_seconds","seconds":10}}));
        let status = json!({"mode":"commands","state":"running","actions":actions,"total":1001,"completed":1000,"results":["duplicatehistory"],"native":{"events":["internal"]},"patches":(0..1000).map(|i|json!({"patch_id":format!("patch-{i}"),"state":"accepted","request":{"large":"private"},"revision":i})).collect::<Vec<_>>()});
        let original = status.clone();
        let compact = job_status(&status, false);
        assert_eq!(compact["actions"].as_array().unwrap().len(), 9);
        assert_eq!(compact["history_omitted"], 992);
        assert_eq!(compact["counts"]["executed"], 1000);
        assert_eq!(compact["counts"]["queued"], 1);
        assert!(compact["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action_id"] == "pending" && a["command"] == "engine.command"));
        assert_eq!(compact["patches"].as_array().unwrap().len(), 8);
        assert_eq!(compact["patch_history_omitted"], 992);
        assert!(compact.get("results").is_none());
        assert!(compact.get("native").is_none());
        assert!(compact.to_string().len() < 6000);
        assert_eq!(status, original);
        assert_eq!(job_status(&status, true), original);
    }
    #[test]
    fn default_load_job_receipt_keeps_guards_and_identity_without_analysis_arrays() {
        let status = json!({"state":"completed","results":[{"ok":true,"stage":"ready","result":{"load_revision":9,"sequence":42,"track":{"id":"spotify:track:test","path":"spotify:track:test","title":"Song","artist":"Artist","bpm":122,"duration_seconds":300,"key":"Am","waveform":[1,2,3],"waveform_bands":{"low":[1,2,3]},"neural_rhythm":{"beats_seconds":[0,1,2],"downbeats_seconds":[0],"bpm":122},"model":{"beat_logits":[1,2],"downbeat_logits":[3,4]},"analysis_stages":{"rhythm":{"outcome":"complete"}},"rhythm_error":null}}}]});
        let compact = job_status(&status, false);
        let result = &compact["results"][0]["result"];
        assert_eq!(result["load_revision"], 9);
        assert_eq!(result["track"]["title"], "Song");
        assert_eq!(result["track"]["bpm"], 122);
        assert!(result["track"].get("waveform").is_none());
        assert!(result["track"]["model"].get("beat_logits").is_none());
        assert!(result["track"]["neural_rhythm"]
            .get("beats_seconds")
            .is_none());
        assert_eq!(result["track"]["neural_rhythm"]["beat_count"], 3);
        assert_eq!(job_status(&status, true), status);
    }
}
