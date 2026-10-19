// Mutable software-command jobs extend the existing batch executor. Timing stays
// in the engine's NativeBatch; asset/provider work stays in unified execute().
fn queued_engine_command(request: &Request) -> Result<Option<EngineCommand>> {
    match request.command.as_str() {
        "engine.command" => Ok(Some(parse_engine(
            request
                .args
                .get("command")
                .cloned()
                .unwrap_or_else(|| request.args.clone()),
        )?)),
        "automation.ramp" => {
            let mut target: Value = arg(&request.args, "target")?;
            if target.get("deck").is_some() {
                target["deck"] = json!(deck(&target)?);
            }
            Ok(Some(EngineCommand::Ramp {
                target: serde_json::from_value(target).map_err(failure)?,
                value: arg(&request.args, "value")?,
                seconds: optional(&request.args, "seconds")?.unwrap_or(1.),
                clock: optional::<Value>(&request.args, "clock")?
                    .map(|clock| deck(&json!({"deck":clock})))
                    .transpose()?,
                beats: optional(&request.args, "beats")?,
                quantize: optional(&request.args, "quantize")?,
            }))
        }
        _ => Ok(None),
    }
}
/// A queue item has at most one readiness predecessor. Index that functional
/// graph once and stop at completed barriers, so retained session history does
/// not turn ordinary appends into repeated ancestry scans.
fn validate_action_dependencies(actions: &[&Value]) -> Result<()> {
    let index: std::collections::HashMap<&str, usize> = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| action["action_id"].as_str().map(|id| (id, index)))
        .collect();
    let mut colors = vec![0u8; actions.len()];
    for start in 0..actions.len() {
        if colors[start] != 0 {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(start);
        while let Some(node) = current {
            if action_terminal(actions[node]) || colors[node] == 2 {
                break;
            }
            if colors[node] == 1 {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "Action dependencies contain a cycle",
                ));
            }
            colors[node] = 1;
            path.push(node);
            current = actions[node]["after"]
                .as_str()
                .and_then(|after| index.get(after).copied());
        }
        for node in path {
            colors[node] = 2;
        }
    }
    Ok(())
}
fn command_job_actions(steps: &Value, existing: &[Value], job: &str) -> Result<Vec<Value>> {
    let steps = steps
        .as_array()
        .ok_or_else(|| ControlError::new("invalid_arguments", "steps must be an array"))?;
    if steps.len() > 256 || existing.len() + steps.len() > 4096 {
        return Err(ControlError::new(
            "queue_full",
            "A job retains at most 4096 actions and accepts at most 256 per append",
        ));
    }
    let mut known: std::collections::HashSet<String> = existing
        .iter()
        .filter_map(|a| a["action_id"].as_str().map(str::to_owned))
        .collect();
    let mut actions = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let mut request = step.clone();
        let object = request.as_object_mut().ok_or_else(|| {
            ControlError::new(
                "invalid_arguments",
                "Each step must be a software command object",
            )
        })?;
        let action_id = match object.remove("action_id") {
            Some(Value::String(id)) => id,
            None => format!("action-{}", existing.len() + index + 1),
            Some(_) => {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "action_id must be a string",
                ))
            }
        };
        if action_id.is_empty() || action_id.len() > 128 {
            return Err(ControlError::new(
                "invalid_arguments",
                "action_id must contain 1..128 bytes",
            ));
        }
        if !known.insert(action_id.clone()) {
            let prior = existing
                .iter()
                .find(|action| action["action_id"] == action_id);
            return Err(ControlError::new("idempotency_conflict", "Action ID already belongs to accepted work; a rejected new command does not replace that work")
                .with_detail("action_id", action_id)
                .with_detail("existing_state", prior.map_or(json!("same_append"), |action| action["state"].clone()))
                .with_detail("hint", "Use a fresh action ID for new work, or explicitly update a pending action. Do not bind new dependencies to an ID whose replacement was rejected."));
        }
        let when = object
            .remove("when")
            .unwrap_or_else(|| json!({"type":"now"}));
        if !when.is_object() {
            return Err(ControlError::new(
                "invalid_arguments",
                "when must be an object",
            ));
        }
        let after = match when.get("after") {
            None => None,
            Some(Value::String(after)) if !after.is_empty() && after.len() <= 128 => {
                Some(after.clone())
            }
            Some(_) => {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "when.after must name an action using a string of 1..128 bytes",
                ))
            }
        };
        let mut timing = when.clone();
        if let Some(object) = timing.as_object_mut() {
            object.remove("after");
            if !object.contains_key("type") {
                object.insert("type".into(), json!("now"));
            }
            if let Some(clock) = object.get("clock").cloned() {
                object.insert("clock".into(), json!(deck(&json!({"deck":clock}))?));
            }
        }
        let timing: deck_engine::QueueTiming = serde_json::from_value(timing).map_err(failure)?;
        deck_engine::validate_queue_timing(timing).map_err(scheduled_failure)?;
        let members = object.remove("commands");
        let group = members.is_some();
        let raw_requests = if let Some(members) = members {
            if !object.is_empty() {
                return Err(ControlError::new("invalid_arguments", "A command group accepts only action_id, when and commands; put Request fields on each member"));
            }
            let members = members.as_array().ok_or_else(|| {
                ControlError::new("invalid_arguments", "commands must be an array")
            })?;
            if !(2..=64).contains(&members.len()) {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "A simultaneous group requires 2..64 renderer commands",
                ));
            }
            members.clone()
        } else {
            vec![request]
        };
        let mut requests = Vec::new();
        let mut engine_commands = Vec::new();
        for (member, raw) in raw_requests.into_iter().enumerate() {
            let mut request: Request = serde_json::from_value(raw).map_err(failure)?;
            request.no_wait = false;
            request.request_id = format!("{job}:{action_id}:{member}");
            request.validate()?;
            validate_step(&request)?;
            if request.command.starts_with("jobs.")
                || matches!(request.command.as_str(), "automation.stop" | "app.stop")
            {
                return Err(ControlError::new("invalid_arguments","Queue ownership/lifecycle commands must be sent outside their own software-command queue"));
            }
            let native = queued_engine_command(&request)?;
            if group {
                let command = native.ok_or_else(|| ControlError::new("command_unsupported", "Simultaneous groups accept renderer commands only: engine.command and automation.ramp; queue provider and metadata commands separately with when.after"))?;
                if queue_business_command(&command, &request) {
                    return Err(ControlError::new("command_unsupported", "Metadata and source preparation commands cannot be simultaneous group members; queue them separately with when.after"));
                }
                engine_commands.push(command);
            } else if native.is_none()
                && when
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind != "now")
            {
                return Err(ControlError::new("command_unsupported","Source-clock timing applies to engine.command and automation.ramp; other software commands support readiness dependencies via when.after"));
            }
            requests.push(request);
        }
        if group {
            deck_engine::validate_command_group(&engine_commands).map_err(scheduled_failure)?;
        }
        let ordinal = existing.len() + index + 1;
        let mut action = json!({"action_id":action_id,"state":"registered","request":requests[0],"when":when,"after":after,"registered_unix_ms":unix_ms(),"ordinal":ordinal});
        if group {
            action["requests"] = serialized(&requests)?;
            action["members"] = json!(requests.iter().enumerate().map(|(i, request)| json!({"member_id":i,"state":"registered","command":request.command,"args":request.args})).collect::<Vec<_>>());
        }
        actions.push(action);
    }
    // References may point to existing actions or earlier/new actions in this append.
    let all: std::collections::HashSet<_> = existing
        .iter()
        .chain(actions.iter())
        .filter_map(|a| a["action_id"].as_str())
        .collect();
    for action in &actions {
        if let Some(after) = action["after"].as_str() {
            if after == action["action_id"].as_str().unwrap() || !all.contains(after) {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "when.after must name another action in this job",
                ));
            }
        }
    }
    validate_action_dependencies(&existing.iter().chain(actions.iter()).collect::<Vec<_>>())?;
    Ok(actions)
}
fn action_requests(action: &Value) -> Result<Vec<Request>> {
    if let Some(requests) = action["requests"].as_array() {
        requests
            .iter()
            .map(|request| serde_json::from_value(request.clone()).map_err(failure))
            .collect()
    } else {
        Ok(vec![
            serde_json::from_value(action["request"].clone()).map_err(failure)?
        ])
    }
}
fn action_native_ids(action: &Value) -> Vec<u64> {
    if let Some(members) = action["members"].as_array() {
        members
            .iter()
            .filter_map(|member| member["native_action_id"].as_u64())
            .collect()
    } else {
        action["native_action_id"].as_u64().into_iter().collect()
    }
}
fn compiled_action_commands(
    action: &Value,
    when: deck_engine::QueueTiming,
) -> Result<Option<Vec<deck_engine::QueuedCommand>>> {
    let requests = action_requests(action)?;
    let group = requests.len() > 1;
    let first_id = action["ordinal"].as_u64().unwrap() * 64;
    let mut commands = Vec::with_capacity(requests.len());
    for (index, request) in requests.iter().enumerate() {
        let Some(command) = queued_engine_command(request)? else {
            return Ok(None);
        };
        if queue_business_command(&command, request) {
            return Ok(None);
        }
        commands.push(deck_engine::QueuedCommand {
            action_id: first_id + index as u64,
            group_id: group.then_some(first_id),
            command,
            when,
        });
    }
    Ok(Some(commands))
}
fn settle_action_members(action: &mut Value, state: &str) {
    if let Some(members) = action["members"].as_array_mut() {
        for member in members.iter_mut().filter(|member| {
            !matches!(
                member["state"].as_str(),
                Some("executed" | "failed" | "cancelled" | "interrupted")
            )
        }) {
            member["state"] = json!(state);
        }
    }
}
fn validate_action_source_guards(
    action: &Value,
    snapshot: &deck_engine::EngineSnapshot,
) -> Result<()> {
    for request in action_requests(action)? {
        let native = queued_engine_command(&request)?;
        if let Some(deck) = native.as_ref().and_then(|command| command.deck()) {
            if request
                .expected_load_revision
                .is_some_and(|revision| revision != snapshot.load_revisions[deck])
            {
                return Err(ControlError::new(
                    "source_changed",
                    "Explicit source guard differs from loaded source",
                ));
            }
            // The local BPM can change before a future/dependent item runs.
            // Static validation and source guards happen here; native group
            // preflight resolves dynamic tempo reachability at execution.
        }
    }
    Ok(())
}
fn mark_native_action(action: &mut Value, native_id: u64) {
    let first_id = action["ordinal"].as_u64().unwrap() * 64;
    action["state"] = json!("queued");
    action["native_action_id"] = json!(first_id);
    action["native_segment"] = json!(native_id);
    if let Some(members) = action["members"].as_array_mut() {
        for (index, member) in members.iter_mut().enumerate() {
            member["native_action_id"] = json!(first_id + index as u64);
            member["state"] = json!("queued");
        }
    }
}
fn start_command_batch(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let open = optional::<bool>(&request.args, "open")?.unwrap_or(false);
    let steps = &request.args["steps"];
    if !open && steps.as_array().is_none_or(Vec::is_empty) {
        return Err(ControlError::new(
            "invalid_arguments",
            "Closed batch needs at least one step",
        ));
    }
    let operation = optional::<String>(&request.args, "operation_id")?
        .unwrap_or_else(|| request.request_id.clone());
    let fingerprint = queue_fingerprint(&request.args);
    let control = app.state::<Control>();
    if let Some(mut receipt) = scheduled_duplicate(
        &*control.jobs.lock().map_err(failure)?,
        &operation,
        &fingerprint,
    )? {
        receipt["event_sequence"] = json!(control.event_sequence.load(Ordering::Acquire));
        return Ok(("accepted", receipt));
    }
    let sequence = control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1;
    let id = format!("job-{}-{sequence}", std::process::id());
    let actions = command_job_actions(steps, &[], &id)?;
    let registered = actions
        .iter()
        .map(|a| a["action_id"].clone())
        .collect::<Vec<_>>();
    insert_job_bounded(
        &mut *control.jobs.lock().map_err(failure)?,
        id.clone(),
        Job {
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            native_id: None,
            fingerprint: Some(fingerprint),
            status: json!({"mode":"commands","state":"running","operation_id":operation,"open":open,"revision":1,"actions":actions,"patches":[],"completed":0,"total":registered.len(),"results":[],"created_unix_ms":unix_ms(),"created_sequence":sequence,"native_patch_sequence":0,"sequential":!open}),
        },
    )?;
    persist_control(app)?;
    event(
        app,
        "job.updated",
        json!({"id":id,"state":"running","revision":1,"open":open}),
    );
    let runtime = app.clone();
    let job = id.clone();
    std::thread::spawn(move || command_job_worker(runtime, job));
    Ok((
        "accepted",
        json!({"job_id":id,"revision":1,"registered":registered,"event_sequence":control.event_sequence.load(Ordering::Acquire)}),
    ))
}
fn action_terminal(action: &Value) -> bool {
    matches!(
        action["state"].as_str(),
        Some("executed" | "failed" | "cancelled" | "interrupted")
    )
}
fn command_job_event(app: &tauri::AppHandle, id: &str, action: &Value, revision: &Value) {
    event(
        app,
        "job.action",
        json!({"id":id,"action_id":action["action_id"],"state":action["state"],"revision":revision,"result":action["result"],"receipt":action["receipt"],"members":action["members"]}),
    );
}
/// Retarget logical readiness edges before persistence/native submission. The
/// renderer performs the corresponding pending-only transaction on exact IDs.
fn replacement_dependencies(
    existing: &[Value],
    candidates: &mut [Value],
    cancel: &[String],
    replacements: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<Value>> {
    let mut targets = std::collections::HashSet::new();
    for (old, new) in replacements {
        if !cancel.contains(old)
            || !existing.iter().any(|action| action["action_id"] == *old)
            || !candidates.iter().any(|action| action["action_id"] == *new)
            || !targets.insert(new)
        {
            return Err(ControlError::new("invalid_arguments", "Each replacements entry must map a distinct invalidated action to a distinct appended action"));
        }
    }
    let retarget = |action: &mut Value| -> Result<bool> {
        let Some(after) = action["after"].as_str() else {
            return Ok(false);
        };
        let Some(replacement) = replacements.get(after) else {
            return Ok(false);
        };
        if action_terminal(action) {
            return Ok(false);
        }
        if !matches!(action["state"].as_str(), Some("registered" | "queued")) {
            return Err(ControlError::new(
                "action_started",
                "A dependent action has started and cannot follow a replacement",
            ));
        }
        action["after"] = json!(replacement);
        action["when"]["after"] = json!(replacement);
        Ok(true)
    };
    let mut rebound = Vec::new();
    for action in existing {
        if cancel.iter().any(|name| action["action_id"] == *name) {
            continue;
        }
        let mut action = action.clone();
        if retarget(&mut action)? {
            rebound.push(action);
        }
    }
    for action in candidates.iter_mut() {
        retarget(action)?;
    }
    let rebound_index: std::collections::HashMap<&str, &Value> = rebound
        .iter()
        .filter_map(|action| action["action_id"].as_str().map(|id| (id, action)))
        .collect();
    let combined: Vec<_> = existing
        .iter()
        .filter(|action| !cancel.iter().any(|name| action["action_id"] == *name))
        .map(|action| {
            action["action_id"]
                .as_str()
                .and_then(|id| rebound_index.get(id).copied())
                .unwrap_or(action)
        })
        .chain(candidates.iter())
        .collect();
    for action in &combined {
        if !action_terminal(action)
            && action["after"]
                .as_str()
                .is_some_and(|after| cancel.iter().any(|name| name == after))
        {
            return Err(ControlError::new("dependency_conflict", "A pending dependent still references a removed action; remove its dependents too or provide replacements")
                .with_detail("dependent_action_id", action["action_id"].clone()));
        }
    }
    validate_action_dependencies(&combined)?;
    Ok(rebound)
}
fn patch_command_job(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let id: String = arg(&request.args, "id")?;
    let patch_id: String = arg(&request.args, "patch_id")?;
    let expected: u64 = arg(&request.args, "expected_revision")?;
    let fingerprint = queue_fingerprint(&json!({"command":request.command,"args":request.args}));
    let control = app.state::<Control>();
    let status = {
        let jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs
            .get(&id)
            .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?;
        if let Some(mut receipt) = patch_duplicate(&job.status, &patch_id, &fingerprint)? {
            receipt["job_id"] = json!(id);
            receipt["event_sequence"] = json!(control.event_sequence.load(Ordering::Acquire));
            return Ok(("accepted", receipt));
        }
        if terminal_job(job) || job.status["open"] != true {
            return Err(ControlError::new("queue_closed", "Command job is closed"));
        }
        if job.status["revision"].as_u64() != Some(expected) {
            return Err(
                ControlError::new("revision_conflict", "Queue revision changed")
                    .with_detail("current_revision", job.status["revision"].clone()),
            );
        }
        job.status.clone()
    };
    let existing = status["actions"].as_array().cloned().unwrap_or_default();
    let cancel: Vec<String> = if request.command == "batch.invalidate" {
        arg(&request.args, "action_ids")?
    } else {
        optional(&request.args, "invalidate_action_ids")?.unwrap_or_default()
    };
    let mut candidates = if request.command == "batch.append" {
        command_job_actions(&request.args["steps"], &existing, &id)?
    } else {
        Vec::new()
    };
    let replacements: std::collections::BTreeMap<String, String> =
        optional(&request.args, "replacements")?.unwrap_or_default();
    let rebound = replacement_dependencies(&existing, &mut candidates, &cancel, &replacements)?;
    let rebound_by_id: std::collections::HashMap<&str, &Value> = rebound
        .iter()
        .filter_map(|action| action["action_id"].as_str().map(|id| (id, action)))
        .collect();
    let mut native_cancel = Vec::new();
    for name in &cancel {
        let action = existing
            .iter()
            .find(|a| a["action_id"] == *name)
            .ok_or_else(|| ControlError::new("invalid_arguments", "Unknown action_id"))?;
        if action_terminal(action)
            || matches!(action["state"].as_str(), Some("running" | "completing"))
            || action["native_done"] == true
        {
            return Err(ControlError::new(
                "action_started",
                "Only registered or queued actions can be updated or removed",
            ));
        }
        if action["state"] == "queued" {
            native_cancel.extend(action_native_ids(action));
        }
    }
    // Claim the mutation durably before a callback-side invalidation transaction.
    let native_id = {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs.get_mut(&id).unwrap();
        job.status["patches"].as_array_mut().unwrap().push(json!({"patch_id":patch_id,"fingerprint":fingerprint,"state":"submitted","request":request.args,"actions":candidates,"expected_revision":expected}));
        job.native_id
    };
    persist_control(app)?;
    let mutated_native = !native_cancel.is_empty();
    if mutated_native {
        let outcome = (|| -> Result<()> {
            let native_id = native_id.ok_or_else(|| {
                ControlError::new("queue_closed", "Native segment no longer exists")
            })?;
            let snapshot = app
                .state::<AppState>()
                .engine
                .transport_snapshot()
                .scheduled_batch;
            let mut context = status.clone();
            for action in context["actions"].as_array_mut().unwrap() {
                if let Some(updated) = action["action_id"]
                    .as_str()
                    .and_then(|id| rebound_by_id.get(id).copied())
                {
                    *action = updated.clone();
                }
            }
            let mut native_candidates = candidates.clone();
            for action in &mut native_candidates {
                mark_native_action(action, native_id);
            }
            context["actions"]
                .as_array_mut()
                .unwrap()
                .extend(native_candidates);
            let mut append = Vec::new();
            let mut all_native = true;
            for action in &candidates {
                validate_action_source_guards(
                    action,
                    &app.state::<AppState>().engine.transport_snapshot(),
                )?;
                match resolved_action_timing(app, action, &context, Some(native_id))? {
                    Some(when) => match compiled_action_commands(action, when)? {
                        Some(commands) => append.extend(commands),
                        None => {
                            all_native = false;
                            break;
                        }
                    },
                    None => {
                        all_native = false;
                        break;
                    }
                }
            }
            if !all_native {
                append.clear();
            }
            let mut rebind_dependencies = Vec::new();
            for (old, new) in &replacements {
                let previous = existing
                    .iter()
                    .find(|action| action["action_id"] == *old)
                    .unwrap();
                if previous["state"] != "queued" {
                    continue;
                }
                let replacement = candidates
                    .iter()
                    .find(|action| action["action_id"] == *new)
                    .unwrap();
                if !all_native {
                    if rebound.iter().any(|action| {
                        action["after"] == *new
                            && action["state"] == "queued"
                            && action["native_action_id"].is_number()
                    }) {
                        return Err(ControlError::new("command_unsupported", "A native pending action with native dependents must be replaced by renderer commands whose timing is ready"));
                    }
                    continue;
                }
                let anchor = replacement["ordinal"].as_u64().unwrap() * 64;
                rebind_dependencies.extend(
                    action_native_ids(previous)
                        .into_iter()
                        .map(|id| (id, anchor)),
                );
            }
            let patch = deck_engine::CommandQueuePatch {
                patch_id: status["native_patch_sequence"].as_u64().unwrap_or(0) + 1,
                expected_revision: snapshot.revision,
                append,
                invalidate: native_cancel,
                rebind_dependencies,
            };
            app.state::<AppState>()
                .engine
                .patch_command_queue(native_id, patch)
                .map_err(scheduled_failure)?;
            command_queue_fence(app, request.timeout_ms)?;
            let snapshot = app
                .state::<AppState>()
                .engine
                .transport_snapshot()
                .scheduled_batch;
            if snapshot
                .updates
                .last()
                .is_none_or(|receipt| !receipt.accepted)
            {
                return Err(ControlError::new(
                    "patch_rejected",
                    "Native action or dependent started before the pending replacement could commit",
                ));
            }
            if all_native {
                for action in &mut candidates {
                    mark_native_action(action, native_id);
                }
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            if let Some(record) = control
                .jobs
                .lock()
                .map_err(failure)?
                .get_mut(&id)
                .and_then(|job| job.status["patches"].as_array_mut())
                .and_then(|patches| patches.last_mut())
            {
                record["state"] = json!("rejected");
                record["error"] = serialized(&error)?;
            }
            persist_control(app)?;
            event(
                app,
                "job.patch",
                json!({"id":id,"patch_id":patch_id,"accepted":false,"revision":expected,"error":error}),
            );
            return Err(error);
        }
    }
    let (revision, changed) = {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs.get_mut(&id).unwrap();
        let mut changed = Vec::new();
        for action in job.status["actions"].as_array_mut().unwrap() {
            if let Some(updated) = action["action_id"]
                .as_str()
                .and_then(|id| rebound_by_id.get(id).copied())
            {
                *action = updated.clone();
                changed.push(action.clone());
            }
            if cancel.iter().any(|name| action["action_id"] == *name) {
                action["state"] = json!("cancelled");
                settle_action_members(action, "cancelled");
                changed.push(action.clone());
            }
        }
        job.status["actions"]
            .as_array_mut()
            .unwrap()
            .extend(candidates.clone());
        job.status["revision"] = json!(expected + 1);
        job.status["total"] = json!(job.status["actions"].as_array().unwrap().len());
        if request.command == "batch.seal" {
            job.status["open"] = json!(false);
        }
        let record = job.status["patches"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        record["state"] = json!("accepted");
        record["revision"] = json!(expected + 1);
        if mutated_native {
            job.status["native_patch_sequence"] =
                json!(status["native_patch_sequence"].as_u64().unwrap_or(0) + 1);
        }
        (job.status["revision"].clone(), changed)
    };
    persist_control(app)?;
    for action in changed.iter().chain(candidates.iter()) {
        command_job_event(app, &id, action, &revision);
    }
    event(
        app,
        "job.patch",
        json!({"id":id,"patch_id":patch_id,"revision":revision,"accepted":true}),
    );
    Ok((
        "accepted",
        json!({"job_id":id,"patch_id":patch_id,"revision":revision,"registered":candidates.iter().map(|a|a["action_id"].clone()).collect::<Vec<_>>(),"event_sequence":control.event_sequence.load(Ordering::Acquire)}),
    ))
}
fn command_queue_fence(app: &tauri::AppHandle, timeout: u64) -> Result<()> {
    let state = app.state::<AppState>();
    let fence = state.engine.fence().map_err(failure)?;
    state
        .engine
        .wait_for_fence(fence, Duration::from_millis(timeout))
        .map_err(failure)
}
fn receipt_timing(receipt: &Value) -> Value {
    let mut timing = serde_json::Map::new();
    for key in [
        "resolved_clock",
        "resolved_clock_seconds",
        "resolved_frame",
        "clock_source_headroom_seconds",
        "ramp_end_frame",
    ] {
        if let Some(value) = receipt.get(key).filter(|value| !value.is_null()) {
            timing.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(timing)
}
fn native_action_error(native: &Value, receipt: &Value, members: &Value) -> Value {
    let failure = &receipt["failure"];
    let (message, hint) = match failure["cause"].as_str() {
        Some("source_missing") => ("A command's deck has no loaded source", "Queue deck.load and make this command depend on its successful completion."),
        Some("clock_source_changed") => ("The scheduled clock source changed", "Inspect the loaded source and register a new timing target against the current source."),
        Some("clock_not_playing") => ("The timing clock is paused", "Use a playing clock or an explicit successful Play dependency."),
        Some("clock_eof") => ("The timing clock reached the end of its source", "Inspect source headroom and choose an earlier absolute target or a different playing clock."),
        Some("incoming_already_playing") => ("The incoming deck is already playing", "Inspect the deck state and remove the redundant Play command."),
        Some("grid_missing") => ("A required beat grid is unavailable", "Prepare a valid grid before scheduling beat-based timing or Sync."),
        Some("jog_held") => ("A required deck is being jogged", "Release the jog control before registering this action again."),
        Some("loop_active") => ("A required deck has an active loop", "Inspect or release the loop before registering this launch again."),
        Some("preparation_pending") => ("The incoming source is still preparing", "Make the command depend on deck.load completion."),
        Some("sync_master_self") => ("The incoming Sync deck is also the timing master", "Set the playing outgoing deck as master before preparing and launching its follower."),
        Some("hot_cue_missing") => ("The requested hot cue is empty", "Inspect cues and choose or create a populated cue slot."),
        Some("playback_bpm_reference_missing") => ("Absolute playback BPM has no valid local grid reference", "Prepare or correct the grid before setting playback BPM."),
        Some("playback_bpm_unreachable") => ("The requested playback BPM exceeds the supported tempo range at execution", "Choose a playback BPM reachable within -90..100 percent of the execution-time local grid BPM."),
        Some("dependency_failed") => ("A required queue action did not complete successfully", "Inspect the failed dependency and register a fresh plan with successful dependencies."),
        Some("launch_aborted") => ("The prepared native launch was aborted", "Inspect deck ownership and source state before scheduling a fresh launch."),
        Some("deadline_missed") => ("The requested source timestamp has already passed", "Choose a future absolute target; use an unquantized zero offset for readiness-only execution."),
        _ => ("The native queue action could not execute", "Inspect this receipt and the queue's reason before submitting a revised action."),
    };
    json!({"code":"native_action_failed","reason":native["reason"],"message":message,"receipt":receipt,"members":members,"failure":failure,"details":{"failure":failure,"hint":hint}})
}
fn reconcile_command_receipts(
    status: &mut Value,
    native: &Value,
    id: &str,
    emitted: &mut Vec<(&'static str, Value)>,
) -> Result<bool> {
    let mut changed = false;
    let interrupted = matches!(
        native["state"].as_str(),
        Some("failed" | "yielded" | "interrupted")
    );
    if let Some(actions) = status["actions"].as_array_mut() {
        for action in actions.iter_mut().filter(|a| {
            matches!(a["state"].as_str(), Some("queued" | "running"))
                && a["native_action_id"].is_number()
        }) {
            let previous_state = action["state"].clone();
            let mut timing_changed = false;
            let ids = action_native_ids(action);
            let receipts: Vec<Value> = ids
                .iter()
                .filter_map(|id| {
                    native["events"]
                        .as_array()
                        .and_then(|events| {
                            events.iter().find(|receipt| receipt["action_id"] == *id)
                        })
                        .cloned()
                })
                .collect();
            if let Some(members) = action["members"].as_array_mut() {
                for member in members {
                    if let Some(receipt) = receipts
                        .iter()
                        .find(|receipt| receipt["action_id"] == member["native_action_id"])
                    {
                        let state = match receipt["state"].as_u64() {
                            Some(1) => "running",
                            Some(2) => "executed",
                            Some(3) => "cancelled",
                            Some(4) => "failed",
                            _ => "queued",
                        };
                        if member["receipt"] != *receipt || member["state"] != state {
                            timing_changed |=
                                receipt_timing(&member["receipt"]) != receipt_timing(receipt);
                            member["receipt"] = receipt.clone();
                            member["state"] = json!(state);
                            changed = true;
                        }
                    }
                }
            }
            if let Some(receipt) = receipts
                .iter()
                .max_by_key(|receipt| receipt["completed_frame"].as_u64().unwrap_or(0))
            {
                if action["receipt"] != *receipt {
                    timing_changed |= receipt_timing(&action["receipt"]) != receipt_timing(receipt);
                    action["receipt"] = receipt.clone();
                    changed = true;
                }
            }
            let all_verified = receipts.len() == ids.len()
                && !ids.is_empty()
                && receipts.iter().all(|receipt| receipt["state"] == 2);
            let failed = receipts
                .iter()
                .find(|receipt| receipt["state"] == 4 && receipt["failure"].is_object())
                .or_else(|| receipts.iter().find(|receipt| receipt["state"] == 4));
            if let Some(receipt) = failed {
                action["receipt"] = receipt.clone();
                action["state"] = json!("failed");
                settle_action_members(action, "failed");
                action["result"] =
                    json!({"error":native_action_error(native, receipt, &action["members"])});
                changed = true;
                emitted.push(("job.action",json!({"id":id,"action_id":action["action_id"],"state":"failed","result":action["result"],"receipt":action["receipt"],"members":action["members"]})));
            } else if receipts.iter().any(|receipt| receipt["state"] == 3) && !interrupted {
                action["state"] = json!("cancelled");
                settle_action_members(action, "cancelled");
                changed = true;
                emitted.push(("job.action",json!({"id":id,"action_id":action["action_id"],"state":"cancelled","receipt":action["receipt"],"members":action["members"]})));
            } else {
                if receipts
                    .iter()
                    .any(|receipt| receipt["state"] == 1 || receipt["state"] == 2)
                    && action["state"] == "queued"
                    && !interrupted
                {
                    action["state"] = json!("running");
                    changed = true;
                    emitted.push(("job.action",json!({"id":id,"action_id":action["action_id"],"state":"running","receipt":action["receipt"],"members":action["members"]})));
                }
                if all_verified {
                    action["native_done"] = json!(true);
                }
            }
            if interrupted && action["native_done"] != true && !action_terminal(action) {
                action["state"] = json!("failed");
                settle_action_members(action, "failed");
                action["result"] = json!({"error":{"code":"native_queue_interrupted","reason":native["reason"],"members":action["members"]}});
                changed = true;
                emitted.push(("job.action",json!({"id":id,"action_id":action["action_id"],"state":"failed","result":action["result"],"members":action["members"]})));
            }
            if timing_changed
                && action["state"] == previous_state
                && !action_terminal(action)
                && action["native_done"] != true
            {
                emitted.push(("job.action", json!({"id":id,"action_id":action["action_id"],"state":action["state"],"receipt":action["receipt"],"members":action["members"]})));
            }
        }
    }
    status["native"] = native.clone();
    if changed {
        emitted.push(("job.progress", json!({"id":id})));
    }
    Ok(changed)
}
fn queue_business_command(command: &EngineCommand, request: &Request) -> bool {
    matches!(
        command,
        EngineCommand::SetBpm { .. }
            | EngineCommand::SetBeatgrid { .. }
            | EngineCommand::SetCue { .. }
            | EngineCommand::SetCuePoint { .. }
            | EngineCommand::SetHotCue { .. }
            | EngineCommand::SetHotCuePoint { .. }
            | EngineCommand::ClearHotCue { .. }
            | EngineCommand::RestoreHotCues { .. }
            | EngineCommand::MemoryCueSave { .. }
            | EngineCommand::MemoryCueDelete { .. }
            | EngineCommand::RestoreMemoryCues { .. }
    ) || request.args.get("bands").is_some()
        || request.args.get("snap").is_some()
}
/// Business services have no renderer command to inspect. Their deck argument
/// defines source ownership; global mutations conservatively wait for both decks.
fn queued_business_decks(request: &Request, native: Option<&EngineCommand>) -> u8 {
    if control_protocol::selection::is_read(&request.command) {
        return 0;
    }
    if let Some(command) = native {
        return command.deck().map_or(3, |deck| 1 << deck);
    }
    if request.command.starts_with("deck.") || request.command == "cues.set" {
        return deck(&request.args).map_or(3, |deck| 1 << deck);
    }
    // These services write metadata/files only; analysis and UI/controller
    // mutations retain the conservative global barrier.
    if request.command.starts_with("playlist.")
        || matches!(
            request.command.as_str(),
            "library.set_tags" | "library.import" | "library.export_xml"
        )
    {
        return 0;
    }
    3
}
fn business_conflicts_with_native(
    request: &Request,
    native: Option<&EngineCommand>,
    owned_decks: u8,
) -> bool {
    queued_business_decks(request, native) & owned_decks != 0
}
fn action_timing(
    action: &Value,
    status: &Value,
    native_id: Option<u64>,
) -> Result<Option<deck_engine::QueueTiming>> {
    let mut timing = action["when"].clone();
    let object = timing
        .as_object_mut()
        .ok_or_else(|| ControlError::new("invalid_arguments", "when must be an object"))?;
    object.remove("after");
    if !object.contains_key("type") {
        object.insert("type".into(), json!("now"));
    }
    if let Some(clock) = object.get("clock").cloned() {
        object.insert("clock".into(), json!(deck(&json!({"deck":clock}))?));
    }
    if let Some(after) = action["after"].as_str() {
        let dependency = status["actions"]
            .as_array()
            .and_then(|actions| actions.iter().find(|a| a["action_id"] == after))
            .ok_or_else(|| ControlError::new("invalid_arguments", "Dependency disappeared"))?;
        if matches!(
            dependency["state"].as_str(),
            Some("failed" | "cancelled" | "interrupted")
        ) {
            return Err(ControlError::new(
                "dependency_failed",
                "Dependency did not execute successfully",
            ));
        }
        if let Some(native) = dependency["native_action_id"]
            .as_u64()
            .filter(|_| dependency["native_segment"].as_u64() == native_id)
        {
            let kind = object["type"].as_str().unwrap_or("now");
            if matches!(kind, "now" | "after_seconds" | "after_beats") {
                return Ok(Some(deck_engine::QueueTiming::AfterAction {
                    action_id: native,
                    seconds: if kind == "after_seconds" {
                        object.get("seconds").and_then(Value::as_f64)
                    } else {
                        None
                    },
                    clock: object
                        .get("clock")
                        .and_then(Value::as_u64)
                        .map(|n| n as usize),
                    beats: if kind == "after_beats" {
                        object.get("beats").and_then(Value::as_f64)
                    } else {
                        None
                    },
                    quantize: object.get("quantize").and_then(Value::as_f64),
                }));
            }
        }
        if dependency["state"] != "executed" {
            return Ok(None);
        }
    }
    Ok(Some(serde_json::from_value(timing).map_err(failure)?))
}
fn finish_command_action(
    app: &tauri::AppHandle,
    id: &str,
    name: &str,
    response: Response,
) -> Result<()> {
    let mut response = serialized(&response)?;
    compact_control_result(&mut response);
    let completion = app.state::<AppState>().engine.transport_snapshot();
    let control = app.state::<Control>();
    let (action, revision) = {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        let Some(job) = jobs.get_mut(id) else {
            return Ok(());
        };
        if job.cancel.load(Ordering::Acquire) || terminal_job(job) {
            return Ok(());
        }
        let action = job.status["actions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|a| a["action_id"] == name)
            .unwrap();
        action["state"] = json!(if response["ok"] == true {
            "executed"
        } else {
            "failed"
        });
        settle_action_members(
            action,
            if response["ok"] == true {
                "executed"
            } else {
                "failed"
            },
        );
        action["result"] = response.clone();
        action["completed_unix_ms"] = json!(unix_ms());
        action["completion_context"] = json!({"frame":completion.rendered_frames,"clock_seconds":completion.decks.each_ref().map(|deck|deck.position_seconds),"load_revisions":completion.load_revisions,"grid_revisions":completion.decks.each_ref().map(|deck|deck.grid_revision),"engine_epoch":completion.engine_epoch,"sample_rate":completion.sample_rate});
        if action["receipt"]["completed_frame"].is_number() {
            action["completion_context"]["frame"] = action["receipt"]["completed_frame"].clone();
            if action["receipt"]["completed_clock_seconds"].is_array() {
                action["completion_context"]["clock_seconds"] =
                    action["receipt"]["completed_clock_seconds"].clone();
            }
            for (source, target) in [
                ("completed_load_revisions", "load_revisions"),
                ("completed_grid_revisions", "grid_revisions"),
            ] {
                if action["receipt"][source].is_array() {
                    action["completion_context"][target] = action["receipt"][source].clone();
                }
            }
        }
        let action = action.clone();
        job.status["completed"] = json!(job.status["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| action_terminal(a))
            .count());
        job.status["results"].as_array_mut().unwrap().push(response);
        (action, job.status["revision"].clone())
    };
    persist_control(app)?;
    command_job_event(app, id, &action, &revision);
    Ok(())
}
fn fail_command_job_actions(status: &mut Value, error: &ControlError) -> Vec<Value> {
    let mut failed = Vec::new();
    if let Some(actions) = status["actions"].as_array_mut() {
        for action in actions.iter_mut().filter(|action| !action_terminal(action)) {
            action["state"] = json!("failed");
            settle_action_members(action, "failed");
            action["result"] =
                json!({"ok":false,"request_id":action["request"]["request_id"],"error":error});
            action["completed_unix_ms"] = json!(unix_ms());
            failed.push(action.clone());
        }
    }
    if let Some(results) = status["results"].as_array_mut() {
        results.extend(failed.iter().map(|action| action["result"].clone()));
    }
    status["completed"] = json!(status["actions"].as_array().map_or(0, Vec::len));
    status["last_error"] = serialized(error).unwrap_or(Value::Null);
    status["state"] = json!("failed");
    status["finished_unix_ms"] = json!(unix_ms());
    failed
}
fn command_job_worker(app: tauri::AppHandle, id: String) {
    loop {
        let before = app
            .state::<Control>()
            .event_sequence
            .load(Ordering::Acquire);
        if let Err(error) = command_job_tick(&app, &id) {
            event(&app, "job.error", json!({"id":id,"error":error}));
            let (native, failed, revision) =
                if let Ok(mut jobs) = app.state::<Control>().jobs.lock() {
                    if let Some(job) = jobs.get_mut(&id) {
                        let failed = fail_command_job_actions(&mut job.status, &error);
                        (job.native_id, failed, job.status["revision"].clone())
                    } else {
                        (None, Vec::new(), Value::Null)
                    }
                } else {
                    (None, Vec::new(), Value::Null)
                };
            if let Some(native) = native {
                let _ = app
                    .state::<AppState>()
                    .engine
                    .cancel_scheduled_batch(native, false);
            }
            for action in &failed {
                if action["request"]["command"] == "deck.load" {
                    if let (Ok(deck), Some(owner)) = (
                        deck(&action["request"]["args"]),
                        action["request"]["request_id"].as_str(),
                    ) {
                        app.state::<AppState>()
                            .source_jobs
                            .cancel_owner(deck, owner);
                    }
                }
            }
            let _ = persist_control(&app);
            for action in failed {
                command_job_event(&app, &id, &action, &revision);
            }
            event(
                &app,
                "job.updated",
                json!({"id":id,"state":"failed","error":error}),
            );
        }
        let control = app.state::<Control>();
        let terminal = control
            .jobs
            .lock()
            .map(|jobs| jobs.get(&id).is_none_or(terminal_job))
            .unwrap_or(true);
        if terminal {
            break;
        }
        if control.event_sequence.load(Ordering::Acquire) != before {
            continue;
        }
        if let Ok(events) = control.events.lock() {
            let _guard = control
                .event_ready
                .wait_timeout(events, Duration::from_millis(50));
        };
    }
}
fn stop_failed_sequential_job(status: &mut Value) -> Option<Vec<Value>> {
    if status["sequential"] != true { return None; }
    let error = status["actions"].as_array()?.iter()
        .find(|action| action["state"] == "failed")?["result"]["error"].clone();
    let mut cancelled = Vec::new();
    for action in status["actions"].as_array_mut()? {
        if !action_terminal(action) {
            action["state"] = json!("cancelled");
            settle_action_members(action, "cancelled");
            cancelled.push(action.clone());
        }
    }
    let attempted: Vec<_> = status["actions"].as_array()?.iter()
        .filter(|action| matches!(action["state"].as_str(), Some("executed" | "failed")))
        .map(|action| action["result"].clone()).collect();
    status["completed"] = json!(attempted.len());
    status["results"] = json!(attempted);
    status["state"] = json!("failed");
    status["open"] = json!(false);
    status["last_error"] = error;
    status["finished_unix_ms"] = json!(unix_ms());
    Some(cancelled)
}

fn command_job_tick(app: &tauri::AppHandle, id: &str) -> Result<()> {
    refresh_scheduled_jobs(app)?;
    let control = app.state::<Control>();
    let _submission = control.scheduled_submission.lock().map_err(failure)?;
    let (status, native_id, cancelled) = {
        let jobs = control.jobs.lock().map_err(failure)?;
        let Some(job) = jobs.get(id) else {
            return Ok(());
        };
        (
            job.status.clone(),
            job.native_id,
            job.cancel.load(Ordering::Acquire),
        )
    };
    if cancelled {
        for action in status["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|action| {
                action["state"] == "running" && action["request"]["command"] == "deck.load"
            })
        {
            if let (Ok(deck), Some(owner)) = (
                deck(&action["request"]["args"]),
                action["request"]["request_id"].as_str(),
            ) {
                app.state::<AppState>()
                    .source_jobs
                    .cancel_owner(deck, owner);
            }
        }
        if let Some(native) = native_id {
            app.state::<AppState>()
                .engine
                .cancel_scheduled_batch(native, false)
                .map_err(failure)?;
            command_queue_fence(app, 5000)?;
        }
        let actions = {
            let mut jobs = control.jobs.lock().map_err(failure)?;
            let job = jobs.get_mut(id).unwrap();
            job.native_id = None;
            job.status["state"] = json!("cancelled");
            job.status["open"] = json!(false);
            job.status["finished_unix_ms"] = json!(unix_ms());
            let mut changed = Vec::new();
            for action in job.status["actions"].as_array_mut().unwrap() {
                if !action_terminal(action) {
                    action["state"] = json!("cancelled");
                    settle_action_members(action, "cancelled");
                    changed.push(action.clone());
                }
            }
            changed
        };
        persist_control(app)?;
        for action in actions {
            command_job_event(app, id, &action, &status["revision"]);
        }
        event(
            app,
            "job.updated",
            json!({"id":id,"state":"cancelled","revision":status["revision"]}),
        );
        return Ok(());
    }
    let mut stopped = status.clone();
    if let Some(cancelled) = stop_failed_sequential_job(&mut stopped) {
        if let Some(native) = native_id {
            app.state::<AppState>().engine.cancel_scheduled_batch(native, false).map_err(failure)?;
            command_queue_fence(app, 5000)?;
        }
        {
            let mut jobs = control.jobs.lock().map_err(failure)?;
            let job = jobs.get_mut(id).unwrap();
            job.native_id = None;
            job.status = stopped;
        }
        persist_control(app)?;
        for action in cancelled {
            command_job_event(app, id, &action, &status["revision"]);
        }
        event(app, "job.updated", json!({"id":id,"state":"failed","revision":status["revision"]}));
        return Ok(());
    }
    if let Some(action) = status["actions"].as_array().unwrap().iter().find(|a| {
        matches!(a["state"].as_str(), Some("queued" | "running")) && a["native_done"] == true
    }) {
        let name = action["action_id"].as_str().unwrap();
        let request: Request =
            serde_json::from_value(action["request"].clone()).map_err(failure)?;
        for member in action_requests(action)? {
            if let Some(command) = queued_engine_command(&member)? {
                app.state::<AppState>()
                    .controller
                    .lock()
                    .map_err(failure)?
                    .observe(&command, true);
            }
        }
        let response = Response::success(
            &request,
            "executed",
            json!({"receipt":action["receipt"],"members":action["members"],"load_revision":queued_engine_command(&request)?.and_then(|c|c.deck()).map(|d|app.state::<AppState>().engine.load_revision(d))}),
        );
        finish_command_action(app, id, name, response)?;
        return Ok(());
    }
    let native_ownership = app
        .state::<AppState>()
        .engine
        .transport_snapshot()
        .scheduled_batch;
    let owned_decks = native_ownership.owned_decks;
    let actions = status["actions"].as_array().unwrap();
    let pending = actions
        .iter()
        .any(|a| matches!(a["state"].as_str(), Some("queued" | "running")));
    let software_running = actions
        .iter()
        .filter(|a| a["state"] == "running" && !a["native_action_id"].is_number())
        .count();
    let registered = actions
        .iter()
        .filter(|a| a["state"] == "registered")
        .find(|action| {
            if status["sequential"] == true && pending {
                return false;
            }
            if matches!(action_timing(action, &status, native_id), Ok(None)) {
                return false;
            }
            let Ok(request) = serde_json::from_value::<Request>(action["request"].clone()) else {
                return true;
            };
            let business = queued_engine_command(&request)
                .ok()
                .flatten()
                .as_ref()
                .is_none_or(|command| queue_business_command(command, &request));
            if business {
                let command = queued_engine_command(&request).ok().flatten();
                if software_running >= 4
                    || business_conflicts_with_native(&request, command.as_ref(), owned_decks)
                {
                    return false;
                }
                if let Some(after) = action["after"].as_str() {
                    if actions
                        .iter()
                        .any(|old| old["action_id"] == after && old["state"] != "executed")
                    {
                        return false;
                    }
                }
            }
            // Source-dependent work waits for this queue's matching deck load;
            // the command's canonical deck accessor also covers nested args.
            let targets = action_requests(action)
                .unwrap_or_default()
                .iter()
                .filter_map(|request| {
                    queued_engine_command(request)
                        .ok()
                        .flatten()
                        .and_then(|command| command.deck())
                        .or_else(|| deck(&request.args).ok())
                })
                .collect::<Vec<_>>();
            let clock = action["when"]
                .get("clock")
                .and_then(|clock| deck(&json!({"deck":clock})).ok());
            if actions.iter().any(|other| {
                other["state"] == "running"
                    && other["request"]["command"] == "deck.load"
                    && deck(&other["request"]["args"])
                        .is_ok_and(|deck| targets.contains(&deck) || Some(deck) == clock)
            }) {
                return false;
            }
            true
        });
    let Some(action) = registered else {
        if !pending && status["open"] != true {
            if let Some(native) = native_id {
                app.state::<AppState>()
                    .engine
                    .cancel_scheduled_batch(native, false)
                    .map_err(failure)?;
                command_queue_fence(app, 5000)?;
            }
            let mut jobs = control.jobs.lock().map_err(failure)?;
            let job = jobs.get_mut(id).unwrap();
            job.native_id = None;
            job.status["state"] = json!(if job.status["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["state"] == "failed")
            {
                "failed"
            } else {
                "completed"
            });
            job.status["finished_unix_ms"] = json!(unix_ms());
            let state = job.status["state"].clone();
            drop(jobs);
            persist_control(app)?;
            event(
                app,
                "job.updated",
                json!({"id":id,"state":state,"revision":status["revision"]}),
            );
        }
        return Ok(());
    };
    let request: Request = serde_json::from_value(action["request"].clone()).map_err(failure)?;
    let name = action["action_id"].as_str().unwrap();
    let mut timing = match resolved_action_timing(app, action, &status, native_id) {
        Ok(Some(when)) => when,
        Ok(None) => return Ok(()),
        Err(error) => {
            finish_command_action(
                app,
                id,
                name,
                Response::failure(request.request_id.clone(), error),
            )?;
            return Ok(());
        }
    };
    let native_command = queued_engine_command(&request)?;
    let business = native_command
        .as_ref()
        .is_none_or(|command| queue_business_command(command, &request));
    if business {
        if business_conflicts_with_native(&request, native_command.as_ref(), owned_decks) {
            return Ok(());
        }
        // An unrelated deck mutation keeps the native cohort and all its pending
        // actions. The engine captures fresh per-action guards for later appends.
        if !matches!(
            timing,
            deck_engine::QueueTiming::Now | deck_engine::QueueTiming::AfterAction { .. }
        ) {
            finish_command_action(app,id,name,Response::failure(request.request_id.clone(),ControlError::new("command_unsupported","This software command supports immediate/dependency readiness execution, not renderer timing")))?;
            return Ok(());
        }
        {
            let mut jobs = control.jobs.lock().map_err(failure)?;
            let action = jobs.get_mut(id).unwrap().status["actions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|a| a["action_id"] == name)
                .unwrap();
            action["state"] = json!("running");
        }
        persist_control(app)?;
        command_job_event(
            app,
            id,
            &json!({"action_id":name,"state":"running"}),
            &status["revision"],
        );
        // Provider/UI waits do not hold submission ownership; appends can register
        // while the existing unified command is loading or waiting for readiness.
        drop(_submission);
        let runtime = app.clone();
        let job = id.to_string();
        let action_name = name.to_string();
        std::thread::spawn(move || {
            if job_cancelled(&runtime, &request) {
                return;
            }
            let response = execute(&runtime, request);
            if let Err(error) = finish_command_action(&runtime, &job, &action_name, response) {
                event(
                    &runtime,
                    "job.error",
                    json!({"id":job,"action_id":action_name,"error":error}),
                );
            }
        });
        return Ok(());
    }
    let mut segment = native_id;
    let current = app
        .state::<AppState>()
        .engine
        .transport_snapshot()
        .scheduled_batch;
    let member_count = action_requests(action)?.len();
    let exhausted = current.total + member_count > 64;
    if segment.is_some()
        && (exhausted
            || !(current.state.active()
                || current.command_queue
                    && current.state == deck_engine::BatchLifecycle::Completed))
    {
        if pending {
            return Ok(());
        }
        app.state::<AppState>()
            .engine
            .cancel_scheduled_batch(segment.unwrap(), false)
            .map_err(failure)?;
        command_queue_fence(app, 5000)?;
        segment = None;
    }
    if segment.is_none() {
        let native = control.job_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        app.state::<AppState>()
            .engine
            .arm_command_queue(native)
            .map_err(failure)?;
        command_queue_fence(app, 5000)?;
        control
            .jobs
            .lock()
            .map_err(failure)?
            .get_mut(id)
            .unwrap()
            .native_id = Some(native);
        segment = Some(native);
    }
    if segment != native_id {
        timing = resolved_action_timing(app, action, &status, segment)?.ok_or_else(|| {
            ControlError::new(
                "dependency_pending",
                "Dependency is not complete in the new native segment",
            )
        })?;
    }
    let snapshot = app.state::<AppState>().engine.transport_snapshot();
    let native = segment.unwrap();
    let patch_sequence = status["native_patch_sequence"].as_u64().unwrap_or(0) + 1;
    if let Err(error) = validate_action_source_guards(action, &snapshot) {
        finish_command_action(
            app,
            id,
            name,
            Response::failure(request.request_id.clone(), error),
        )?;
        return Ok(());
    }
    let patch = deck_engine::CommandQueuePatch {
        patch_id: patch_sequence,
        expected_revision: snapshot.scheduled_batch.revision,
        append: compiled_action_commands(action, timing)?.unwrap(),
        invalidate: Vec::new(),
        rebind_dependencies: Vec::new(),
    };
    {
        let mut jobs = control.jobs.lock().map_err(failure)?;
        let job = jobs.get_mut(id).unwrap();
        let action = job.status["actions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|a| a["action_id"] == name)
            .unwrap();
        mark_native_action(action, native);
        job.status["native_patch_sequence"] = json!(patch_sequence);
    }
    persist_control(app)?;
    if let Err(error) = app
        .state::<AppState>()
        .engine
        .patch_command_queue(native, patch)
        .map_err(scheduled_failure)
        .and_then(|_| command_queue_fence(app, 5000))
    {
        finish_command_action(
            app,
            id,
            name,
            Response::failure(request.request_id.clone(), error),
        )?;
        return Ok(());
    }
    let snapshot = app
        .state::<AppState>()
        .engine
        .transport_snapshot()
        .scheduled_batch;
    if snapshot
        .updates
        .last()
        .is_none_or(|receipt| !receipt.accepted)
    {
        finish_command_action(
            app,
            id,
            name,
            Response::failure(
                request.request_id.clone(),
                ControlError::new(
                    "patch_rejected",
                    "Native command queue rejected this action",
                )
                .with_detail("queue", serialized(snapshot)?),
            ),
        )?;
    } else {
        let mut queued = action.clone();
        mark_native_action(&mut queued, native);
        command_job_event(app, id, &queued, &status["revision"]);
    }
    Ok(())
}

fn command_step_schema() -> Value {
    let request = serde_json::to_value(schemars::schema_for!(Request)).unwrap();
    let when = json!({"type":"object","description":"Native timing: type now|at_event|at_seconds|after_beats|after_seconds, clock A/B or0/1, event/seconds/beats/quantize; optional after names an item whose actual completion anchors this condition. A group completes after every member. Missing type is now.","properties":{"type":{"type":"string","enum":["now","at_event","at_seconds","after_beats","after_seconds"]},"after":{"type":"string"},"clock":{"oneOf":[{"type":"integer","enum":[0,1]},{"type":"string","enum":["A","B"]}]},"event":{"type":"integer"},"seconds":{"type":"number"},"beats":{"type":"number"},"quantize":{"type":"number"}},"additionalProperties":false});
    let mut scalar = request.clone();
    scalar["properties"]["action_id"] = json!({"type":"string","minLength":1,"maxLength":128});
    scalar["properties"]["when"] = when.clone();
    json!({"oneOf":[scalar,{"type":"object","properties":{"action_id":{"type":"string","minLength":1,"maxLength":128},"when":when,"commands":{"type":"array","minItems":2,"maxItems":64,"items":request,"description":"Renderer-only commands with distinct native control targets, applied atomically on the same frame; each ramp keeps its own duration."}},"required":["commands"],"additionalProperties":false}]})
}

#[cfg(test)]
mod command_queue_control_tests {
    use super::*;

    #[test]
    fn closed_sequential_jobs_stop_after_the_first_runtime_failure() {
        let original = json!({"sequential":true,"state":"running","completed":1,"open":false,"actions":[
            {"action_id":"first","state":"executed","result":{"ok":true}},
            {"action_id":"failure","state":"failed","result":{"error":{"code":"deck_empty"}}},
            {"action_id":"must-not-run","state":"registered"}
        ]});
        let mut status = original.clone();
        let cancelled = stop_failed_sequential_job(&mut status).unwrap();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(status["state"], "failed");
        assert_eq!(status["completed"], 2);
        assert_eq!(status["results"].as_array().unwrap().len(), 2);
        assert_eq!(status["actions"][2]["state"], "cancelled");
        assert_eq!(status["last_error"]["code"], "deck_empty");
        let mut open = original;
        open["sequential"] = json!(false);
        assert!(stop_failed_sequential_job(&mut open).is_none());
        assert_eq!(open["actions"][2]["state"], "registered");
    }
    fn group_step() -> Value {
        json!({"action_id":"blend","commands":[
            {"command":"automation.ramp","args":{"target":{"type":"eq_low","deck":"B"},"value":0.8,"seconds":0.25}},
            {"command":"automation.ramp","args":{"target":{"type":"tempo_percent","deck":"B"},"value":1.5,"seconds":1.0}}
        ],"when":{"type":"after_seconds","seconds":2}})
    }
    #[test]
    fn group_registration_is_one_logical_item_with_stable_distinct_native_members() {
        let actions = command_job_actions(&json!([group_step(), {"action_id":"next","command":"engine.command","args":{"type":"set_channel_fader","deck":"A","value":0.5},"when":{"after":"blend"}}]), &[], "job").unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["members"].as_array().unwrap().len(), 2);
        assert_eq!(actions[0]["state"], "registered");
        let compiled = compiled_action_commands(
            &actions[0],
            deck_engine::QueueTiming::AfterSeconds { seconds: 2.0 },
        )
        .unwrap()
        .unwrap();
        assert_eq!(compiled.len(), 2);
        assert_eq!(compiled[0].action_id, 64);
        assert_eq!(compiled[1].action_id, 65);
        assert_eq!(compiled[0].group_id, Some(64));
        assert_eq!(compiled[0].when, compiled[1].when);
        let next = compiled_action_commands(&actions[1], deck_engine::QueueTiming::Now)
            .unwrap()
            .unwrap();
        assert!(next[0].action_id > compiled[1].action_id);
        assert_eq!(next[0].group_id, None);
        let mut group = actions[0].clone();
        mark_native_action(&mut group, 42);
        assert_eq!(action_native_ids(&group), vec![64, 65]);
        let status = json!({"actions":[group]});
        assert!(matches!(
            action_timing(&actions[1], &status, Some(42)).unwrap(),
            Some(deck_engine::QueueTiming::AfterAction { action_id: 64, .. })
        ));
    }
    #[test]
    fn pending_replacement_retargets_dependents_preserving_native_identity_and_offsets() {
        let existing = vec![
            json!({"action_id":"old","state":"queued","ordinal":1,"native_action_id":64,"native_segment":42,"when":{"type":"after_seconds","seconds":3},"after":null}),
            json!({"action_id":"dependent","state":"queued","ordinal":2,"native_action_id":128,"native_segment":42,"when":{"after":"old","type":"after_seconds","seconds":0.25},"after":"old"}),
        ];
        let mut candidates = vec![
            json!({"action_id":"new","state":"registered","ordinal":3,"when":{"type":"after_seconds","seconds":1},"after":null}),
        ];
        let replacements = std::collections::BTreeMap::from([("old".into(), "new".into())]);
        let rebound =
            replacement_dependencies(&existing, &mut candidates, &["old".into()], &replacements)
                .unwrap();
        assert_eq!(rebound.len(), 1);
        assert_eq!(rebound[0]["after"], "new");
        assert_eq!(
            rebound[0]["when"],
            json!({"after":"new","type":"after_seconds","seconds":0.25})
        );
        assert_eq!(rebound[0]["native_action_id"], 128);
        assert_eq!(rebound[0]["native_segment"], 42);
        assert_eq!(existing[1]["after"], "old");
        let mut context = json!({"actions":[rebound[0].clone(),candidates[0].clone()]});
        context["actions"][1]["native_action_id"] = json!(192);
        context["actions"][1]["native_segment"] = json!(42);
        assert!(matches!(
            action_timing(&rebound[0], &context, Some(42)).unwrap(),
            Some(deck_engine::QueueTiming::AfterAction {
                action_id: 192,
                seconds: Some(0.25),
                ..
            })
        ));
    }
    #[test]
    fn pending_replacement_rejects_started_dependents_cycles_and_dangling_removal() {
        let existing = vec![
            json!({"action_id":"old","state":"queued","when":{"type":"now"},"after":null}),
            json!({"action_id":"dependent","state":"queued","when":{"after":"old"},"after":"old"}),
        ];
        let original = existing.clone();
        let replacements = std::collections::BTreeMap::from([("old".into(), "new".into())]);
        let mut cyclic = vec![
            json!({"action_id":"new","state":"registered","when":{"after":"dependent"},"after":"dependent"}),
        ];
        assert_eq!(
            replacement_dependencies(&existing, &mut cyclic, &["old".into()], &replacements)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        let mut started = existing.clone();
        started[1]["state"] = json!("running");
        let mut replacement = vec![
            json!({"action_id":"new","state":"registered","when":{"type":"now"},"after":null}),
        ];
        assert_eq!(
            replacement_dependencies(&started, &mut replacement, &["old".into()], &replacements)
                .unwrap_err()
                .code,
            "action_started"
        );
        assert_eq!(
            replacement_dependencies(&existing, &mut [], &["old".into()], &Default::default())
                .unwrap_err()
                .code,
            "dependency_conflict"
        );
        assert!(replacement_dependencies(
            &existing,
            &mut [],
            &["old".into(), "dependent".into()],
            &Default::default()
        )
        .unwrap()
        .is_empty());
        assert_eq!(existing, original);
    }
    #[test]
    fn dependency_validation_handles_long_history_and_shared_pending_chains() {
        let mut actions:Vec<_> = (0usize..4094).map(|index| json!({"action_id":format!("history-{index}"),"state":"executed","after":index.checked_sub(1).map(|previous| format!("history-{previous}"))})).collect();
        actions.push(json!({"action_id":"pending","state":"queued","after":"history-4093"}));
        let mut new = vec![
            json!({"action_id":"new","state":"registered","when":{"after":"pending"},"after":"pending"}),
        ];
        assert!(
            replacement_dependencies(&actions, &mut new, &[], &Default::default())
                .unwrap()
                .is_empty()
        );
        // Validate a full active chain too; the indexed colors visit each edge
        // once rather than walking the same ancestry for every starting node.
        for action in &mut actions {
            action["state"] = json!("queued");
        }
        assert!(validate_action_dependencies(&actions.iter().collect::<Vec<_>>()).is_ok());
        actions[0]["after"] = json!("pending");
        assert!(validate_action_dependencies(&actions.iter().collect::<Vec<_>>()).is_err());
    }
    #[test]
    fn absolute_playback_bpm_uses_canonical_command_in_groups_and_reference_errors_are_specific() {
        let actions = command_job_actions(&json!([{"action_id":"tempo-bass","commands":[
            {"command":"engine.command","args":{"type":"set_playback_bpm","deck":"B","bpm":124}},
            {"command":"automation.ramp","args":{"target":{"type":"eq_low","deck":"B"},"value":1,"seconds":1}}
        ]}]), &[], "job").unwrap();
        let commands = compiled_action_commands(&actions[0], deck_engine::QueueTiming::Now)
            .unwrap()
            .unwrap();
        assert!(matches!(
            commands[0].command,
            EngineCommand::SetPlaybackBpm { deck: 1, bpm: 124. }
        ));
        assert_eq!(commands[0].group_id, commands[1].group_id);
        assert!(validate_playback_bpm_reference(Some(120.), 124., 1).is_ok());
        assert_eq!(
            validate_playback_bpm_reference(None, 124., 1)
                .unwrap_err()
                .code,
            "grid_unknown"
        );
        assert_eq!(
            validate_playback_bpm_reference(Some(30.), 300., 1)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        let mut conflict = json!({"action_id":"bad","commands":[
            {"command":"engine.command","args":{"type":"set_playback_bpm","deck":"B","bpm":124}},
            {"command":"automation.ramp","args":{"target":{"type":"tempo_percent","deck":"B"},"value":1,"seconds":1}}
        ]});
        assert!(command_job_actions(&json!([conflict.clone()]), &[], "job").is_err());
        conflict["commands"][0]["args"]["bpm"] = json!(301.);
        assert!(command_job_actions(&json!([conflict]), &[], "job").is_err());
    }
    #[test]
    fn atomic_append_validates_entire_plan_before_accepting_any_prefix() {
        let existing = vec![
            json!({"action_id":"pause-b","state":"executed","request":{"request_id":"old-accepted"},"receipt":{"completed_frame":100}}),
        ];
        let original = existing.clone();
        for steps in [
            json!([{"action_id":"isolate","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":0}},{"action_id":"pause-b","command":"engine.command","args":{"type":"pause","deck":"B"}}]),
            json!([{"action_id":"isolate","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":0}},{"action_id":"master","command":"engine.command","args":{"type":"set_master","deck":"B"},"when":{"after":"missing"}}]),
            json!([{"action_id":"isolate","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":0}},{"action_id":"load","command":"deck.load","args":{"deck":"B","uri":"spotify:track:5u4hhtZ7f4rWkMZEZcTKr"}}]),
            json!([{"action_id":"a","command":"status","when":{"after":"b"}},{"action_id":"b","command":"status","when":{"after":"a"}}]),
        ] {
            assert!(command_job_actions(&steps, &existing, "job").is_err());
            assert_eq!(existing, original);
        }
        let error = command_job_actions(
            &json!([{"action_id":"pause-b","command":"status"}]),
            &existing,
            "job",
        )
        .unwrap_err();
        assert_eq!(error.code, "idempotency_conflict");
        assert_eq!(error.details["action_id"], "pause-b");
        assert_eq!(error.details["existing_state"], "executed");
        let accepted = command_job_actions(&json!([
            {"action_id":"master-new","command":"engine.command","args":{"type":"set_master","deck":"B"},"when":{"after":"pause-new"}},
            {"action_id":"pause-new","command":"engine.command","args":{"type":"pause","deck":"A"}}
        ]),&existing,"job").unwrap();
        assert_eq!(accepted.len(), 2);
        assert!(accepted
            .iter()
            .all(|action| action["state"] == "registered"));
        assert_eq!(accepted[0]["after"], "pause-new");
    }
    #[test]
    fn malformed_id_or_dependency_cannot_be_silently_dropped() {
        for steps in [
            json!([{"action_id":7,"command":"status"}]),
            json!([{"action_id":"a","command":"status","when":{"after":false}}]),
            json!([{"action_id":"a","command":"status","when":{"after":""}}]),
            json!([{"action_id":"a","command":"status","when":[]}]),
        ] {
            assert_eq!(
                command_job_actions(&steps, &[], "job").unwrap_err().code,
                "invalid_arguments"
            );
        }
    }
    #[test]
    fn spotify_uri_validation_is_shared_by_direct_and_queued_loads() {
        let uri = "spotify:track:5u4hhtZ7f4rWkMZEZcTKr";
        let direct = validate_control_track_uri(uri).unwrap_err();
        let queued =
            validate_step(&Request::new("deck.load", json!({"deck":"B","uri":uri}))).unwrap_err();
        assert_eq!(direct.code, "invalid_track_uri");
        assert_eq!(queued.code, direct.code);
        assert_eq!(queued.message, direct.message);
        assert_eq!(queued.details["argument"], "uri");
        assert!(queued.message.contains("22"));
        assert!(queued.details["hint"]
            .as_str()
            .unwrap()
            .contains("spotify.liked"));
        assert!(validate_step(&Request::new(
            "deck.load",
            json!({"deck":"B","uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC"})
        ))
        .is_ok());
    }
    #[test]
    fn queued_resolution_and_member_specific_failure_are_streamed_without_polling() {
        let mut action = command_job_actions(&json!([group_step()]), &[], "job")
            .unwrap()
            .remove(0);
        mark_native_action(&mut action, 42);
        let mut status = json!({"actions":[action]});
        let native = json!({"state":"armed","events":[
            {"action_id":64,"state":0,"resolved_clock":0,"resolved_clock_seconds":20.,"clock_source_headroom_seconds":15.},
            {"action_id":65,"state":0,"resolved_clock":0,"resolved_clock_seconds":20.,"clock_source_headroom_seconds":15.}
        ]});
        let mut emitted = Vec::new();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        let queued: Vec<_> = emitted
            .iter()
            .filter(|(kind, _)| *kind == "job.action")
            .collect();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].1["state"], "queued");
        assert_eq!(
            queued[0].1["members"][1]["receipt"]["resolved_clock_seconds"],
            20.
        );
        emitted.clear();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        assert!(emitted.iter().all(|(kind, _)| *kind != "job.action"));
        let failed = json!({"state":"failed","reason":"prerequisite_failed","events":[
            {"action_id":64,"state":4,"completed_frame":500},
            {"action_id":65,"state":4,"completed_frame":500,"failure":{"cause":"sync_master_self","deck":1,"action_id":65}}
        ]});
        reconcile_command_receipts(&mut status, &failed, "job", &mut emitted).unwrap();
        let error = &status["actions"][0]["result"]["error"];
        assert_eq!(error["failure"]["action_id"], 65);
        assert_eq!(error["receipt"]["action_id"], 65);
        assert!(error["message"].as_str().unwrap().contains("master"));
        assert!(error["details"]["hint"]
            .as_str()
            .unwrap()
            .contains("outgoing"));
        assert_eq!(status["actions"][0]["members"][0]["state"], "failed");
    }
    #[test]
    fn compact_status_preserves_group_member_execution_and_omits_internal_requests() {
        let mut action = command_job_actions(&json!([group_step()]), &[], "job")
            .unwrap()
            .remove(0);
        mark_native_action(&mut action, 42);
        action["members"][0]["state"] = json!("executed");
        action["members"][0]["receipt"] = json!({"frame":10,"completed_frame":20});
        let status = json!({"mode":"commands","state":"running","actions":[action],"patches":[]});
        let compact = job_status(&status, false);
        assert_eq!(
            compact["actions"][0]["commands"].as_array().unwrap().len(),
            2
        );
        assert_eq!(compact["actions"][0]["members"][0]["state"], "executed");
        assert_eq!(
            compact["actions"][0]["members"][0]["receipt"]["completed_frame"],
            20
        );
        assert!(compact["actions"][0]["requests"].is_null());
        assert_eq!(job_status(&status, true), status);
    }
    #[test]
    fn invalid_group_members_are_rejected_before_registration_without_partial_success() {
        let mut group = group_step();
        group["commands"][1] =
            json!({"command":"deck.load","args":{"deck":"B","path":"/fixture.wav"}});
        assert_eq!(
            command_job_actions(&json!([group]), &[], "job")
                .unwrap_err()
                .code,
            "command_unsupported"
        );
        let mut group = group_step();
        group["commands"][1] = group["commands"][0].clone();
        assert!(command_job_actions(&json!([group]), &[], "job").is_err());
        let mut group = group_step();
        group["commands"][1]["args"]["value"] = json!(500);
        assert!(command_job_actions(&json!([group]), &[], "job").is_err());
        let mut group = group_step();
        group["command"] = json!("status");
        assert!(command_job_actions(&json!([group]), &[], "job").is_err());
        assert!(command_job_actions(&json!([{"commands":[]}]), &[], "job").is_err());
    }
    #[test]
    fn group_completion_waits_every_member_and_uses_latest_receipt_once() {
        let mut action = command_job_actions(&json!([group_step()]), &[], "job")
            .unwrap()
            .remove(0);
        mark_native_action(&mut action, 42);
        let mut status = json!({"actions":[action]});
        let mut emitted = Vec::new();
        let mut native = json!({"state":"running","events":[{"action_id":64,"state":2,"frame":100,"completed_frame":200},{"action_id":65,"state":1,"frame":100}]});
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        assert_eq!(status["actions"][0]["state"], "running");
        assert_ne!(status["actions"][0]["native_done"], true);
        assert_eq!(status["actions"][0]["members"][0]["state"], "executed");
        assert_eq!(
            emitted
                .iter()
                .filter(|(kind, _)| *kind == "job.action")
                .count(),
            1
        );
        native["events"][1]["state"] = json!(2);
        native["events"][1]["completed_frame"] = json!(900);
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        assert_eq!(status["actions"][0]["native_done"], true);
        assert_eq!(status["actions"][0]["receipt"]["completed_frame"], 900);
        assert!(status["actions"][0]["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["state"] == "executed"));
        // The worker finalizes the one group action; reconciliation never emits
        // individual member terminal events or finalizes the group twice.
        status["actions"][0]["state"] = json!("executed");
        let before = emitted.len();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        assert_eq!(emitted.len(), before);
    }
    #[test]
    fn interrupted_group_preserves_completed_member_and_settles_unresolved_members_once() {
        let mut action = command_job_actions(&json!([group_step()]), &[], "job")
            .unwrap()
            .remove(0);
        mark_native_action(&mut action, 42);
        let mut status = json!({"actions":[action]});
        let native = json!({"state":"yielded","reason":"manual_takeover","events":[
            {"action_id":64,"state":2,"frame":100,"completed_frame":200},
            {"action_id":65,"state":0,"frame":0}
        ]});
        let mut emitted = Vec::new();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        let action = &status["actions"][0];
        assert_eq!(action["state"], "failed");
        assert_eq!(action["members"][0]["state"], "executed");
        assert_eq!(action["members"][0]["receipt"]["completed_frame"], 200);
        assert_eq!(action["members"][1]["state"], "failed");
        assert_eq!(action["members"][1]["receipt"]["state"], 0);
        let events: Vec<_> = emitted
            .iter()
            .filter(|(kind, _)| *kind == "job.action")
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1["state"], "failed");
        assert_eq!(events[0].1["members"], action["members"]);
        let settled = status["actions"].clone();
        emitted.clear();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        assert!(emitted.iter().all(|(kind, _)| *kind != "job.action"));
        assert_eq!(status["actions"], settled);
    }
    #[test]
    fn interrupted_native_cancellation_emits_one_failure_and_never_overwrites_terminal_state() {
        let mut status = json!({"actions":[{"action_id":"queued","state":"queued","native_action_id":1},{"action_id":"started","state":"running","native_action_id":2}]});
        let native = json!({"state":"yielded","reason":"manual_takeover","events":[{"action_id":1,"state":3},{"action_id":2,"state":4}]});
        let mut emitted = Vec::new();
        reconcile_command_receipts(&mut status, &native, "job", &mut emitted).unwrap();
        let actions: Vec<_> = emitted
            .iter()
            .filter(|(kind, _)| *kind == "job.action")
            .map(|(_, event)| event)
            .collect();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions
                .iter()
                .filter(|event| event["action_id"] == "queued")
                .count(),
            1
        );
        assert!(actions.iter().all(|event| event["state"] == "failed"));
        assert_eq!(
            status["actions"][0]["result"]["error"]["code"],
            "native_queue_interrupted"
        );
        assert_eq!(
            status["actions"][0]["result"]["error"]["reason"],
            "manual_takeover"
        );
        assert_eq!(
            status["actions"][1]["result"]["error"]["code"],
            "native_action_failed"
        );
        let terminal = status["actions"].clone();
        let mut again = Vec::new();
        reconcile_command_receipts(&mut status, &native, "job", &mut again).unwrap();
        assert!(again.iter().all(|(kind, _)| *kind != "job.action"));
        assert_eq!(status["actions"], terminal);
        let mut explicit =
            json!({"actions":[{"action_id":"stop","state":"queued","native_action_id":1}]});
        let mut cancelled = Vec::new();
        reconcile_command_receipts(
            &mut explicit,
            &json!({"state":"cancelled","events":[{"action_id":1,"state":3}]}),
            "job",
            &mut cancelled,
        )
        .unwrap();
        assert_eq!(explicit["actions"][0]["state"], "cancelled");
        assert_eq!(
            cancelled
                .iter()
                .filter(|(kind, _)| *kind == "job.action")
                .count(),
            1
        );
    }
    #[test]
    fn worker_failure_settles_each_registered_or_queued_item_and_preserves_completed_work() {
        let mut status = json!({"state":"running","revision":2,"results":[],"actions":[{"action_id":"done","state":"executed","result":{"ok":true}},{"action_id":"a","state":"registered","request":{"request_id":"job:a"}},{"action_id":"b","state":"queued","request":{"request_id":"job:b"}}]});
        let error = ControlError::new(
            "resource_conflict",
            "Another native operation is still active",
        );
        let failed = fail_command_job_actions(&mut status, &error);
        assert_eq!(failed.len(), 2);
        assert_eq!(status["state"], "failed");
        assert_eq!(status["completed"], 3);
        assert_eq!(status["actions"][0]["state"], "executed");
        assert!(failed.iter().all(|action| action["state"] == "failed"
            && action["result"]["error"]["code"] == "resource_conflict"));
        assert_eq!(status["results"].as_array().unwrap().len(), 2);
        assert!(fail_command_job_actions(&mut status, &error).is_empty());
    }
    #[test]
    fn business_source_barriers_use_native_scoped_ownership_without_global_cancellation() {
        let load_b = Request::new("deck.load", json!({"deck":"B","path":"/fixture.wav"}));
        let load_a = Request::new("deck.load", json!({"deck":"A","path":"/fixture.wav"}));
        assert!(!business_conflicts_with_native(&load_b, None, 1));
        assert!(business_conflicts_with_native(&load_a, None, 1));
        assert!(business_conflicts_with_native(&load_b, None, 3));
        assert!(!business_conflicts_with_native(
            &Request::new("status", json!({})),
            None,
            3
        ));
        let bpm = Request::new(
            "engine.command",
            json!({"type":"set_bpm","deck":"B","bpm":122}),
        );
        let command = queued_engine_command(&bpm).unwrap().unwrap();
        assert!(!business_conflicts_with_native(&bpm, Some(&command), 1));
        assert!(business_conflicts_with_native(&bpm, Some(&command), 2));
    }
    #[test]
    fn accepted_native_future_remains_queued_until_actual_dispatch_receipt() {
        let mut status =
            json!({"actions":[{"action_id":"future","state":"queued","native_action_id":1}]});
        let mut emitted = Vec::new();
        reconcile_command_receipts(
            &mut status,
            &json!({"state":"armed","events":[{"action_id":1,"state":0}]}),
            "job",
            &mut emitted,
        )
        .unwrap();
        assert_eq!(status["actions"][0]["state"], "queued");
        assert!(emitted.iter().all(|(kind, _)| *kind != "job.action"));
        reconcile_command_receipts(
            &mut status,
            &json!({"state":"running","events":[{"action_id":1,"state":1,"frame":25}]}),
            "job",
            &mut emitted,
        )
        .unwrap();
        assert_eq!(status["actions"][0]["state"], "running");
        assert!(emitted
            .iter()
            .any(|(kind, event)| *kind == "job.action" && event["state"] == "running"));
    }
    #[test]
    fn arbitrary_software_pipeline_registers_before_assets_and_defers_source_capture() {
        let actions=command_job_actions(&json!([
            {"action_id":"load","command":"deck.load","args":{"deck":"B","uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC"}},
            {"action_id":"cue","command":"engine.command","args":{"type":"seek","deck":"B","seconds":8},"when":{"after":"load"}},
            {"action_id":"tempo","command":"engine.command","args":{"type":"set_tempo","deck":"B","percent":1},"when":{"after":"cue"}},
            {"action_id":"play","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"tempo","type":"after_beats","clock":"A","beats":8,"quantize":4}}
        ]),&[],"job").unwrap();
        assert_eq!(actions.len(), 4);
        assert!(actions.iter().all(|action| action["state"] == "registered"));
        assert!(actions
            .iter()
            .all(|action| action["request"]["expected_load_revision"].is_null()));
        assert_eq!(actions[0]["request"]["command"], "deck.load");
        let status = json!({"actions":actions});
        assert!(action_timing(&status["actions"][1], &status, None)
            .unwrap()
            .is_none());
    }
    #[test]
    fn dependency_timing_uses_actual_native_completion_in_same_segment() {
        let status = json!({"actions":[{"action_id":"launch","state":"running","native_action_id":8,"native_segment":42}]});
        let action = json!({"action_id":"bass","after":"launch","when":{"after":"launch","type":"after_beats","clock":"A","beats":16,"quantize":4}});
        assert!(matches!(
            action_timing(&action, &status, Some(42)).unwrap(),
            Some(deck_engine::QueueTiming::AfterAction {
                action_id: 8,
                clock: Some(0),
                beats: Some(16.),
                quantize: Some(4.),
                ..
            })
        ));
        assert!(action_timing(&action, &status, Some(43)).unwrap().is_none());
    }
    #[test]
    fn dependency_cycles_and_reusing_executed_ids_are_rejected() {
        assert!(command_job_actions(&json!([{"action_id":"a","command":"status","when":{"after":"b"}},{"action_id":"b","command":"status","when":{"after":"a"}}]),&[],"job").is_err());
        assert!(command_job_actions(
            &json!([{"action_id":"a","command":"status"}]),
            &[json!({"action_id":"a","state":"executed"})],
            "job"
        )
        .is_err());
        assert!(command_job_actions(
            &json!([{"action_id":"a","command":"jobs.cancel","args":{"id":"unrelated"}}]),
            &[],
            "job"
        )
        .is_err());
    }
    #[test]
    fn native_dispatch_receipt_is_not_reported_as_executed_until_verified() {
        let mut status = json!({"mode":"commands","actions":[{"action_id":"fade","state":"queued","native_action_id":3}]});
        let mut emitted = Vec::new();
        reconcile_command_receipts(
            &mut status,
            &json!({"events":[{"action_id":3,"state":1,"frame":10}],"state":"running"}),
            "job",
            &mut emitted,
        )
        .unwrap();
        assert_ne!(status["actions"][0]["native_done"], true);
        assert_eq!(status["actions"][0]["state"], "running");
        reconcile_command_receipts(&mut status,&json!({"events":[{"action_id":3,"state":2,"frame":10,"completed_frame":200}],"state":"running"}),"job",&mut emitted).unwrap();
        assert_eq!(status["actions"][0]["native_done"], true);
        assert_eq!(status["actions"][0]["receipt"]["completed_frame"], 200);
    }
    #[test]
    fn restored_command_jobs_interrupt_only_unfinished_work_without_replaying() {
        let original = json!({"mode":"commands","state":"running","revision":5,"actions":[{"action_id":"load","state":"executed"},{"action_id":"play","state":"registered"},{"action_id":"fade","state":"queued"}],"patches":[{"patch_id":"p","state":"submitted","fingerprint":"raw"}]});
        let restored = interrupted_status(original);
        assert_eq!(restored["state"], "interrupted");
        assert_eq!(restored["actions"][0]["state"], "executed");
        assert_eq!(restored["actions"][1]["state"], "interrupted");
        assert_eq!(restored["actions"][2]["state"], "interrupted");
        assert_eq!(
            patch_duplicate(&restored, "p", "raw").unwrap().unwrap()["replayed"],
            false
        );
    }
}

fn resolved_action_timing(
    app: &tauri::AppHandle,
    action: &Value,
    status: &Value,
    native: Option<u64>,
) -> Result<Option<deck_engine::QueueTiming>> {
    let timing = action_timing(action, status, native)?;
    if timing.is_none() || matches!(timing, Some(deck_engine::QueueTiming::AfterAction { .. })) {
        return Ok(timing);
    }
    let Some(after) = action["after"].as_str() else {
        return Ok(timing);
    };
    let Some(dependency) = status["actions"]
        .as_array()
        .and_then(|actions| actions.iter().find(|a| a["action_id"] == after))
    else {
        return Ok(timing);
    };
    if dependency["state"] != "executed" {
        return Ok(timing);
    }
    let reference = &dependency["completion_context"];
    match timing {
        Some(deck_engine::QueueTiming::AfterSeconds { seconds }) => {
            let snapshot = app.state::<AppState>().engine.transport_snapshot();
            if reference["engine_epoch"].as_u64() != Some(snapshot.engine_epoch) {
                return Err(ControlError::new(
                    "engine_epoch_changed",
                    "Dependency belongs to another audio renderer",
                ));
            }
            let frame = reference["frame"].as_u64().ok_or_else(|| {
                ControlError::new(
                    "invalid_arguments",
                    "Dependency completion clock is unavailable",
                )
            })?;
            let due = frame.saturating_add((seconds * snapshot.sample_rate as f64).ceil() as u64);
            Ok(Some(if due <= snapshot.rendered_frames {
                deck_engine::QueueTiming::Now
            } else {
                deck_engine::QueueTiming::AfterSeconds {
                    seconds: (due - snapshot.rendered_frames) as f64 / snapshot.sample_rate as f64,
                }
            }))
        }
        Some(deck_engine::QueueTiming::AfterBeats {
            clock,
            beats,
            quantize,
        }) => {
            let snapshot = app
                .state::<AppState>()
                .engine
                .scheduled_snapshot()
                .map_err(scheduled_failure)?;
            if reference["load_revisions"][clock].as_u64() != Some(snapshot.load_revisions[clock])
                || reference["grid_revisions"][clock].as_u64()
                    != Some(snapshot.decks[clock].grid_revision)
            {
                return Err(ControlError::new(
                    "source_changed",
                    "Dependency beat clock was replaced or its grid changed",
                ));
            }
            if beats == 0. && quantize.is_none() {
                return Ok(Some(deck_engine::QueueTiming::Now));
            }
            let seconds = reference["clock_seconds"][clock].as_f64().ok_or_else(|| {
                ControlError::new("grid_unknown", "Dependency clock is unavailable")
            })?;
            let deck = &snapshot.decks[clock];
            let mut beat = deck.grid_position(seconds).ok_or_else(|| {
                ControlError::new("grid_unknown", "Dependency has no valid beat clock")
            })? + beats;
            if let Some(quantize) = quantize {
                beat = (beat / quantize).ceil() * quantize;
            }
            let seconds = deck.grid_time(beat).ok_or_else(|| {
                ControlError::new("grid_unknown", "Dependency beat clock is unavailable")
            })?;
            Ok(Some(deck_engine::QueueTiming::AtSeconds { clock, seconds }))
        }
        _ => Ok(timing),
    }
}
