//! Duplex JSONL transport for the existing control queue. Scheduling stays in the app.
use control_protocol::{ControlError, Request, Response};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;

type Output = Arc<Mutex<std::io::Stdout>>;
fn emit(output: &Output, value: Value) {
    if let Ok(mut output) = output.lock() {
        let _ = writeln!(output, "{value}");
        let _ = output.flush();
    }
}
fn call(socket: &Path, request: &Request) -> Response {
    control_protocol::ipc::call(socket, request)
        .unwrap_or_else(|e| Response::failure(request.request_id.clone(), e))
}
fn invalid(message: impl ToString) -> ControlError {
    ControlError::new("invalid_request", message.to_string())
}
fn response_value(response: Response, kind: &str, session: &str, action: Option<&str>) -> Value {
    let mut value = serde_json::to_value(response).unwrap();
    let rejected = value["ok"] == false && matches!(kind, "registered" | "updated" | "removed");
    value["type"] = json!(if rejected { "rejected" } else { kind });
    if rejected {
        value["operation"] = json!(match kind {
            "registered" => "add",
            "updated" => "update",
            _ => "remove",
        });
        if let Some(details) = value["error"]["details"].as_object_mut() {
            details.remove("expected_arguments");
            details.entry("hint").or_insert_with(|| json!("Inspect this item with op:status; revise pending work using op:add/update/remove. Command schemas are available through commands describe."));
        }
    }
    value["session_id"] = json!(session);
    if let Some(action) = action {
        value["action_id"] = json!(action);
    }
    value
}
fn input_response(
    response: Response,
    kind: &str,
    session: &str,
    action: Option<&str>,
    input: &Value,
) -> Value {
    let kind = if !response.ok && input.get("items").is_some() {
        "registered"
    } else {
        kind
    };
    let mut value = response_value(response, kind, session, action);
    if input.get("items").is_none() {
        if let Some(id) = action.or_else(|| input["id"].as_str()) {
            value["item_id"] = json!(id);
        }
    }
    if let Some(entries) = input.get("items") {
        let count = entries.as_array().map_or(0, Vec::len);
        let accepted = value["ok"] == true;
        value["atomic"] = json!(true);
        value["item_count"] = json!(count);
        value["registered_count"] = json!(if accepted { count } else { 0 });
        value["items"] = entries
            .as_array()
            .map(|entries| {
                json!(entries
                    .iter()
                    .map(|item| {
                        let mut item = item.clone();
                        if accepted {
                            let id = item["id"].clone();
                            item["type"] = json!("registered");
                            item["ok"] = json!(true);
                            item["item_id"] = id.clone();
                            item["action_id"] = id;
                            item["version"] = json!(1);
                        }
                        item
                    })
                    .collect::<Vec<_>>())
            })
            .unwrap_or_else(|| entries.clone());
    }
    value
}
fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str, ControlError> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("Missing {key}")))
}
fn step(input: &Value, id: &str) -> Result<Value, ControlError> {
    let mut value = if let Some(commands) = input.get("commands") {
        if [
            "command",
            "args",
            "fields",
            "view",
            "timeout_ms",
            "expected_load_revision",
            "require",
        ]
        .iter()
        .any(|key| input.get(*key).is_some())
        {
            return Err(invalid("A commands group cannot also contain outer command/args or per-command options; put options on each member"));
        }
        let commands = commands
            .as_array()
            .ok_or_else(|| invalid("commands must be an array of 2..64 native command objects"))?;
        if !(2..=64).contains(&commands.len()) {
            return Err(invalid("commands must contain 2..64 native command objects; use command/args for one command"));
        }
        for (index, command) in commands.iter().enumerate() {
            let request: Request = serde_json::from_value(command.clone())
                .map_err(|error| invalid(format!("commands[{index}]: {error}")))?;
            request
                .validate()
                .map_err(|error| invalid(format!("commands[{index}]: {}", error.message)))?;
            if !matches!(
                request.command.as_str(),
                "engine.command" | "automation.ramp"
            ) {
                return Err(invalid(format!("commands[{index}]: parallel groups support engine.command and automation.ramp; queue loading and other commands as separate dependent items")));
            }
        }
        json!({"action_id":id,"commands":commands})
    } else {
        json!({"action_id":id,"command":required_text(input,"command")?,"args":input.get("args").cloned().unwrap_or_else(||json!({}))})
    };
    for key in [
        "when",
        "expected_load_revision",
        "require",
        "fields",
        "view",
        "timeout_ms",
    ] {
        if let Some(item) = input.get(key) {
            value[key] = item.clone();
        }
    }
    Ok(value)
}
/// Return only canonical requests: convenience input contains no execution logic.
fn adapt(
    input: &Value,
    session: &str,
    job: &str,
    revision: u64,
) -> Result<(Request, String, Option<String>), ControlError> {
    if input["op"] == "call" {
        let mut envelope = input.clone();
        if let Some(object) = envelope.as_object_mut() {
            object.remove("op");
        }
        let request = serde_json::from_value(envelope).map_err(invalid)?;
        return Ok((request, "response".into(), None));
    }
    let op = required_text(input, "op")?;
    let id = input["id"].as_str();
    let (command, args, kind, action) = match op {
        "add" if input.get("items").is_some() => {
            if input.as_object().is_none_or(|object| {
                object
                    .keys()
                    .any(|key| !["op", "items", "request_id"].contains(&key.as_str()))
            }) {
                return Err(invalid("Atomic add accepts only op, items and optional request_id; give each item its own id/command/when"));
            }
            let items = input["items"]
                .as_array()
                .filter(|items| !items.is_empty() && items.len() <= 256)
                .ok_or_else(|| invalid("items must be an array of 1..256 queue items"))?;
            let steps = items.iter().map(|item| {
                if ["op", "items", "action_id", "request_id"].iter().any(|key| item.get(*key).is_some()) {
                    return Err(invalid("Atomic items contain id, command/commands and options; operations and request_id belong on the outer add"));
                }
                step(item, required_text(item, "id")?)
            }).collect::<Result<Vec<_>, _>>()?;
            (
                "batch.append",
                json!({"id":job,"expected_revision":revision,"steps":steps}),
                "registered",
                None,
            )
        }
        "add" => {
            let id = required_text(input, "id")?;
            (
                "batch.append",
                json!({"id":job,"expected_revision":revision,"steps":[step(input,id)?]}),
                "registered",
                Some(id.to_owned()),
            )
        }
        "update" => {
            let old = required_text(input, "id")?;
            let new = required_text(input, "action_id")?;
            if old == new {
                return Err(invalid(
                    "Update requires a new action_id; completed actions cannot be rewritten",
                ));
            }
            (
                "batch.append",
                json!({"id":job,"expected_revision":revision,"invalidate_action_ids":[old],"replacements":{old:new},"steps":[step(input,new)?]}),
                "updated",
                Some(new.to_owned()),
            )
        }
        "remove" => {
            let id = required_text(input, "id")?;
            (
                "batch.invalidate",
                json!({"id":job,"expected_revision":revision,"action_ids":[id]}),
                "removed",
                Some(id.to_owned()),
            )
        }
        "status" => ("jobs.get", json!({"id":job}), "status", None),
        "stop" => ("jobs.cancel", json!({"id":job}), "stopped", None),
        _ => {
            return Err(invalid(
                "Unknown op; use add, update, remove, status, stop, or call",
            ))
        }
    };
    let mut request = Request::new(command, args);
    if let Some(key) = input["request_id"].as_str() {
        request.request_id = key.to_owned();
    }
    if command.starts_with("batch.") {
        // Retrying the same action reuses its receipt, including after a lost acknowledgement.
        request.args["patch_id"] = json!(input["request_id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                if let Some(items) = input["items"].as_array() {
                    for item in items {
                        item["id"].as_str().hash(&mut hash);
                    }
                } else {
                    action.as_deref().or(id).unwrap_or("").hash(&mut hash);
                }
                format!("{session}:{op}:{:016x}", hash.finish())
            }));
    }
    Ok((request, kind.into(), action))
}

/// Validate the exact atomic session plan before starting its playback clock.
pub fn validation_args(input: &Value) -> Result<Value, ControlError> {
    if input["op"] != "add" || !input["items"].is_array() {
        return Err(invalid("A session plan must contain op:add and items"));
    }
    let (request, _, _) = adapt(input, "validate", "validate", 1)?;
    Ok(json!({"steps": request.args["steps"]}))
}
/// Pin convenience dependency names before delegating to the canonical queue.
fn dependency(
    input: &mut Value,
    aliases: &HashMap<String, String>,
    accepted: &HashMap<String, (String, u64)>,
    ambiguous: &HashSet<String>,
    new_ids: &HashSet<String>,
) -> Result<(), ControlError> {
    let Some(when) = input.get_mut("when").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let selected = when.remove("after_version");
    let Some(after) = when.get("after").and_then(Value::as_str).map(str::to_owned) else {
        return if selected.is_some() {
            Err(invalid(
                "after_version requires after with an acknowledged item ID",
            ))
        } else {
            Ok(())
        };
    };
    if let Some(selected) = selected {
        let version = selected
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("after_version must be a positive acknowledged version"))?;
        let actual = accepted
            .iter()
            .find(|(_, (id, known))| id == &after && *known == version)
            .map(|(actual, _)| actual.clone())
            .ok_or_else(|| {
                ControlError::new(
                    "dependency_unknown",
                    "Dependency version was not acknowledged in this session",
                )
                .with_detail("item_id", after.as_str())
                .with_detail("version", version)
            })?;
        when.insert("after".into(), json!(actual));
    } else if ambiguous.contains(&after) {
        let mut versions: Vec<u64> = accepted
            .values()
            .filter_map(|(id, version)| (id == &after).then_some(*version))
            .collect();
        versions.sort_unstable();
        versions.dedup();
        return Err(ControlError::new("dependency_ambiguous", "This item ID was reused in a rejected add; select an acknowledged after_version or use a new item ID")
            .with_detail("item_id", after.as_str()).with_detail("accepted_versions", json!(versions)));
    } else if let Some(actual) = aliases.get(&after) {
        when.insert("after".into(), json!(actual));
    } else if !accepted.contains_key(&after) && !new_ids.contains(&after) {
        return Err(ControlError::new(
            "dependency_unknown",
            "Dependency was not acknowledged in this session or included in this atomic append",
        )
        .with_detail("item_id", after.as_str()));
    }
    Ok(())
}
fn attempted_reuses(input: &Value, aliases: &HashMap<String, String>) -> Vec<String> {
    if input.get("op").is_some_and(|op| op != "add") {
        return Vec::new();
    }
    let ids: Vec<&Value> = input["items"].as_array().map_or_else(
        || vec![&input["id"]],
        |items| items.iter().map(|item| &item["id"]).collect(),
    );
    ids.into_iter()
        .filter_map(Value::as_str)
        .filter(|id| aliases.contains_key(*id))
        .map(str::to_owned)
        .collect()
}
fn annotate_ambiguities(
    value: &mut Value,
    ambiguous: &HashSet<String>,
    known: &HashMap<String, (String, u64)>,
) {
    let mut ids: Vec<_> = ambiguous.iter().collect();
    ids.sort();
    value["dependency_ambiguities"] = json!(ids
        .into_iter()
        .map(|id| {
            let mut versions: Vec<_> = known
                .values()
                .filter_map(|(item, version)| (item == id).then_some(*version))
                .collect();
            versions.sort_unstable();
            versions.dedup();
            json!({"item_id":id,"accepted_versions":versions})
        })
        .collect::<Vec<_>>());
}
struct Observer {
    socket: PathBuf,
    job: String,
    session: String,
    cursor: u64,
    revision: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    output: Output,
    items: Arc<Mutex<HashMap<String, (String, u64)>>>,
}
fn session_feedback(event: &Value) -> bool {
    match event["type"].as_str() {
        Some("job.progress") => false,
        Some("job.patch") if event["data"]["accepted"] == true => false,
        Some("job.action") if event["data"]["state"] == "registered" => false,
        _ => true,
    }
}
fn annotate_item(value: &mut Value, items: &Mutex<HashMap<String, (String, u64)>>) {
    let identity = value["action_id"]
        .as_str()
        .and_then(|id| items.lock().ok()?.get(id).cloned());
    if let Some((id, version)) = identity {
        value["item_id"] = json!(id);
        value["version"] = json!(version);
    }
}
fn annotate_status(value: &mut Value, items: &Mutex<HashMap<String, (String, u64)>>) {
    if let Some(actions) = value["result"]["actions"].as_array_mut() {
        for action in actions {
            annotate_item(action, items);
        }
    }
}
impl Observer {
    fn run(self) {
        let Self {
            socket,
            job,
            session,
            mut cursor,
            revision,
            stop,
            failed,
            output,
            items,
        } = self;
        loop {
            let stopping = stop.load(Ordering::Acquire);
            let mut request = Request::new(
                "events.get",
                json!({"job_id":job,"since":cursor,"wait_ms":if stopping {0}else{1000}}),
            );
            request.timeout_ms = 1500;
            let response = call(&socket, &request);
            if !response.ok {
                emit(
                    &output,
                    response_value(response, "session_error", &session, None),
                );
                failed.store(true, Ordering::Release);
                break;
            }
            let result = response.result.unwrap_or_default();
            if result["resync_required"] == true {
                let state = call(&socket, &Request::new("jobs.get", json!({"id":job})));
                if let Some(value) = state.result.as_ref().and_then(|v| v["revision"].as_u64()) {
                    revision.fetch_max(value, Ordering::AcqRel);
                }
                let mut value = response_value(state, "session_resync", &session, None);
                annotate_status(&mut value, &items);
                emit(&output, value);
            }
            if let Some(events) = result["events"].as_array() {
                for event in events {
                    if let Some(value) = event["data"]["revision"].as_u64() {
                        revision.fetch_max(value, Ordering::AcqRel);
                    }
                    // The input receipt already acknowledges registration. Keep
                    // native progress/CAS bookkeeping out of the agent's stream.
                    if !session_feedback(event) {
                        continue;
                    }
                    let mut event = event.clone();
                    event["session_id"] = json!(session);
                    annotate_item(&mut event["data"], &items);
                    emit(&output, event);
                }
            }
            cursor = result["sequence"].as_u64().unwrap_or(cursor);
            if stopping {
                break;
            }
        }
    }
}
/// `start` opts into the owned mutable queue; the old raw request stream remains compatible.
pub fn run(socket: PathBuf, start: bool) -> Result<(), ControlError> {
    let output = Arc::new(Mutex::new(std::io::stdout()));
    if !start {
        for line in std::io::stdin().lock().lines() {
            let line = line.map_err(invalid)?;
            let response = match serde_json::from_str::<Request>(&line) {
                Ok(request) => call(&socket, &request),
                Err(error) => Response::failure("", invalid(error)),
            };
            emit(&output, serde_json::to_value(response).unwrap());
        }
        return Ok(());
    }
    let session = format!(
        "session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
    );
    let create = call(
        &socket,
        &Request::new(
            "batch.run",
            json!({"operation_id":session,"open":true,"steps":[]}),
        ),
    );
    if !create.ok {
        return Err(create
            .error
            .unwrap_or_else(|| invalid("Queue creation failed")));
    }
    let result = create
        .result
        .as_ref()
        .ok_or_else(|| invalid("Missing queue creation receipt"))?;
    let job = result["job_id"]
        .as_str()
        .ok_or_else(|| invalid("Missing job_id in queue receipt"))?
        .to_owned();
    let cursor = result["event_sequence"].as_u64().unwrap_or(0);
    let revision = Arc::new(AtomicU64::new(result["revision"].as_u64().unwrap_or(1)));
    let mut ready = response_value(create, "session_ready", &session, None);
    ready["job_id"] = json!(job);
    ready["revision"] = json!(revision.load(Ordering::Acquire));
    ready["cursor"] = json!(cursor);
    ready["operations"] = json!(["add", "update", "remove", "status", "stop", "call"]);
    emit(&output, ready);
    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let items = Arc::new(Mutex::new(HashMap::new()));
    let observer = {
        let items = items.clone();
        let (socket, job, session, revision, stop, failed, output) = (
            socket.clone(),
            job.clone(),
            session.clone(),
            revision.clone(),
            stop.clone(),
            failed.clone(),
            output.clone(),
        );
        std::thread::spawn(move || {
            Observer {
                socket,
                job,
                session,
                cursor,
                revision,
                stop,
                failed,
                output,
                items,
            }
            .run()
        })
    };
    let (tx, rx) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    // Stable local receipts prevent CAS retries from changing previously acknowledged bodies.
    let mut receipts: HashMap<String, (Value, Response)> = HashMap::new();
    let mut receipt_order = VecDeque::new();
    let mut input_receipts: HashMap<String, (Value, Value)> = HashMap::new();
    let mut input_order = VecDeque::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut versions: HashMap<String, u64> = HashMap::new();
    let mut ambiguous = HashSet::new();
    let mut accepted_items = HashMap::new();
    let mut explicitly_stopped = false;
    while !failed.load(Ordering::Acquire) {
        let line = match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => line.map_err(invalid),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let input = line.and_then(|line| serde_json::from_str::<Value>(&line).map_err(invalid));
        let mut input = match input {
            Ok(input) => input,
            Err(error) => {
                emit(
                    &output,
                    response_value(Response::failure("", error), "response", &session, None),
                );
                continue;
            }
        };
        let original_input = input.clone();
        let input_key = input["request_id"].as_str().map(str::to_owned);
        let replay = input_key.as_ref().and_then(|key| input_receipts.get(key));
        if let Some((prior, _)) = replay {
            if prior != &original_input {
                ambiguous.extend(attempted_reuses(&input, &aliases));
                emit(
                    &output,
                    input_response(
                        Response::failure(
                            input_key.as_deref().unwrap_or(""),
                            ControlError::new(
                                "idempotency_conflict",
                                "request_id already belongs to different input",
                            ),
                        ),
                        "response",
                        &session,
                        None,
                        &input,
                    ),
                );
                continue;
            }
        }
        let replayed_input = replay.map(|(_, normalized)| normalized.clone());
        if replayed_input.is_none() {
            if input.get("op").is_none() && input.is_object() {
                input["op"] = json!("add");
            }
            if input["op"] == "add" && input.get("items").is_none() && input.get("id").is_none() {
                input["id"] = json!(input["request_id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("item-{}", Request::new("", json!({})).request_id)));
            }
        }
        let logical = input["id"].as_str().map(str::to_owned).or_else(|| {
            replayed_input
                .as_ref()
                .and_then(|input| input["id"].as_str())
                .map(str::to_owned)
        });
        if let Some(normalized) = replayed_input.as_ref() {
            input = normalized.clone();
        }
        let reused_ids = attempted_reuses(&input, &aliases);
        if replayed_input.is_none() {
            if let Some(logical) = logical.as_ref() {
                if matches!(input["op"].as_str(), Some("remove" | "update")) {
                    if let Some(actual) = aliases.get(logical) {
                        input["id"] = json!(actual);
                    }
                }
                if input["op"] == "update" && input.get("action_id").is_none() {
                    let version = versions.get(logical).copied().unwrap_or(1) + 1;
                    input["action_id"] = json!(format!("{logical}@{version}"));
                }
            }
            let new_ids: HashSet<String> = input["items"]
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry["id"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let normalized =
                if let Some(entries) = input.get_mut("items").and_then(Value::as_array_mut) {
                    entries.iter_mut().try_for_each(|entry| {
                        dependency(entry, &aliases, &accepted_items, &ambiguous, &new_ids)
                    })
                } else {
                    dependency(&mut input, &aliases, &accepted_items, &ambiguous, &new_ids)
                };
            if let Err(error) = normalized {
                ambiguous.extend(reused_ids);
                emit(
                    &output,
                    input_response(
                        Response::failure(input["request_id"].as_str().unwrap_or(""), error),
                        "registered",
                        &session,
                        logical.as_deref(),
                        &input,
                    ),
                );
                continue;
            }
        }
        let (mut request, kind, action) =
            match adapt(&input, &session, &job, revision.load(Ordering::Acquire)) {
                Ok(request) => request,
                Err(error) => {
                    ambiguous.extend(reused_ids);
                    emit(
                        &output,
                        input_response(
                            Response::failure(input["request_id"].as_str().unwrap_or(""), error),
                            match input["op"].as_str() {
                                Some("add") => "registered",
                                Some("update") => "updated",
                                Some("remove") => "removed",
                                _ => "response",
                            },
                            &session,
                            logical.as_deref(),
                            &input,
                        ),
                    );
                    continue;
                }
            };
        let item_version = action
            .as_ref()
            .and_then(|id| items.lock().ok()?.get(id).map(|(_, version)| *version))
            .or_else(|| {
                logical.as_ref().map(|id| {
                    versions.get(id).copied().unwrap_or(1)
                        + u64::from(input["op"] == "update" && replayed_input.is_none())
                })
            });
        let registrations: Vec<(String, String, u64)> =
            if let Some(entries) = input["items"].as_array() {
                entries
                    .iter()
                    .filter_map(|item| item["id"].as_str())
                    .map(|id| (id.to_owned(), id.to_owned(), 1))
                    .collect()
            } else if matches!(input["op"].as_str(), Some("add" | "update")) {
                logical
                    .as_ref()
                    .zip(action.as_ref())
                    .zip(item_version)
                    .map(|((logical, actual), version)| {
                        vec![(logical.clone(), actual.clone(), version)]
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
        // Annotate events that can arrive before the registration reply; roll
        // speculative identities back if the single atomic patch is rejected.
        let mut inserted = Vec::new();
        {
            let mut known = items.lock().unwrap();
            for (logical, actual, version) in &registrations {
                if !known.contains_key(actual) {
                    known.insert(actual.clone(), (logical.clone(), *version));
                    inserted.push(actual.clone());
                }
            }
        }
        let original_request_id = request.request_id.clone();
        let patch = request.args["patch_id"].as_str().map(str::to_owned);
        let mut body = request.args.clone();
        if let Some(object) = body.as_object_mut() {
            object.remove("expected_revision");
        }
        let cached = patch.as_ref().and_then(|p| receipts.get(p));
        let locally_replayed = cached.is_some_and(|(prior, _)| prior == &body);
        let mut response = if let Some((prior_body, prior_response)) = cached {
            if prior_body == &body {
                let mut response = prior_response.clone();
                response.request_id = request.request_id.clone();
                response
            } else {
                Response::failure(
                    &request.request_id,
                    ControlError::new(
                        "idempotency_conflict",
                        "Action identity already belongs to a different command",
                    ),
                )
            }
        } else {
            let mut response = call(&socket, &request);
            // Only the authoritative CAS conflict can trigger a retry. A transport failure
            // is not a licence to replay a command with a different identity.
            if patch.is_some()
                && response
                    .error
                    .as_ref()
                    .is_some_and(|e| e.code == "revision_conflict")
            {
                if let Some(current) = response
                    .error
                    .as_ref()
                    .and_then(|e| e.details["current_revision"].as_u64())
                {
                    revision.fetch_max(current, Ordering::AcqRel);
                    request.args["expected_revision"] = json!(current);
                    request.request_id = Request::new("", json!({})).request_id;
                    response = call(&socket, &request);
                }
            }
            if response.ok {
                if let Some(patch) = patch {
                    receipt_order.push_back(patch.clone());
                    receipts.insert(patch, (body, response.clone()));
                    if receipt_order.len() > 4096 {
                        if let Some(old) = receipt_order.pop_front() {
                            receipts.remove(&old);
                        }
                    }
                }
            }
            response
        };
        response.request_id = original_request_id;
        if !response.ok {
            ambiguous.extend(reused_ids);
            let mut known = items.lock().unwrap();
            for actual in inserted {
                known.remove(&actual);
            }
        }
        if response.ok && replayed_input.is_none() {
            if let Some(key) = input_key {
                input_order.push_back(key.clone());
                input_receipts.insert(key, (original_input, input.clone()));
                if input_order.len() > 4096 {
                    if let Some(old) = input_order.pop_front() {
                        input_receipts.remove(&old);
                    }
                }
            }
        }
        if response.ok
            && replayed_input.is_none()
            && !locally_replayed
            && matches!(input["op"].as_str(), Some("add" | "update"))
        {
            for (logical, actual, version) in &registrations {
                accepted_items.insert(actual.clone(), (logical.clone(), *version));
                aliases.insert(logical.clone(), actual.clone());
                versions.insert(logical.clone(), *version);
                if input["op"] == "update" {
                    ambiguous.remove(logical);
                }
            }
        }
        if let Some(value) = response
            .result
            .as_ref()
            .and_then(|r| r["revision"].as_u64())
        {
            revision.fetch_max(value, Ordering::AcqRel);
        }
        explicitly_stopped = input["op"] == "stop" && response.ok;
        let mut value = input_response(response, &kind, &session, action.as_deref(), &input);
        if kind == "status" {
            annotate_status(&mut value, &items);
            annotate_ambiguities(&mut value, &ambiguous, &accepted_items);
        }
        if matches!(input["op"].as_str(), Some("add" | "update")) {
            if input.get("items").is_some() {
                // The atomic receipt is annotated as one accepted/rejected patch.
            } else if let Some(commands) = input.get("commands") {
                value["commands"] = commands.clone();
                value["member_count"] = json!(commands.as_array().map_or(0, Vec::len));
            } else {
                value["command"] = input["command"].clone();
                value["args"] = input.get("args").cloned().unwrap_or_else(|| json!({}));
            }
            if let Some(when) = input.get("when") {
                value["when"] = when.clone();
            }
        }
        if locally_replayed {
            value["duplicate"] = json!(true);
            value["replayed"] = json!(false);
        }
        if let Some(logical) = logical {
            value["item_id"] = json!(logical);
        }
        if let Some(version) = item_version {
            value["version"] = json!(version);
        }
        emit(&output, value);
        if explicitly_stopped {
            break;
        }
    }
    if !explicitly_stopped {
        let response = call(&socket, &Request::new("jobs.cancel", json!({"id":job})));
        emit(&output, response_value(response, "stopped", &session, None));
    }
    stop.store(true, Ordering::Release);
    let _ = observer.join();
    emit(
        &output,
        json!({"type":"session_closed","session_id":session,"job_id":job,"reason":if failed.load(Ordering::Acquire){"connection_lost"}else if explicitly_stopped{"stop"}else{"eof"}}),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_add_delegates_one_patch_and_dependency_versions_are_explicit() {
        let input = json!({"op":"add","items":[
            {"id":"load","command":"deck.load","args":{"deck":"B","uri":"spotify:track:fixture"}},
            {"id":"play","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"load"}}
        ],"request_id":"whole-plan"});
        let (request, kind, action) = adapt(&input, "session", "job", 4).unwrap();
        assert_eq!(request.command, "batch.append");
        assert_eq!(request.args["steps"].as_array().unwrap().len(), 2);
        assert_eq!(request.args["steps"][1]["when"]["after"], "load");
        assert_eq!(request.args["patch_id"], "whole-plan");
        assert_eq!(kind, "registered");
        assert!(action.is_none());
        for bad in [
            json!({"op":"add","items":[]}),
            json!({"op":"add","id":"outer","items":input["items"]}),
            json!({"op":"add","items":[{"command":"status"}]}),
            json!({"op":"add","items":[{"id":"nested","op":"remove"}]}),
        ] {
            assert!(adapt(&bad, "session", "job", 1).is_err());
        }
        let aliases = HashMap::from([("pause".into(), "pause@2".into())]);
        let known = HashMap::from([
            ("pause".into(), ("pause".into(), 1)),
            ("pause@2".into(), ("pause".into(), 2)),
        ]);
        let ambiguous = HashSet::from(["pause".into()]);
        let mut unversioned = json!({"when":{"after":"pause"}});
        assert_eq!(
            dependency(
                &mut unversioned,
                &aliases,
                &known,
                &ambiguous,
                &HashSet::new()
            )
            .unwrap_err()
            .code,
            "dependency_ambiguous"
        );
        for (version, actual) in [(1, "pause"), (2, "pause@2")] {
            let mut selected = json!({"when":{"after":"pause","after_version":version}});
            dependency(&mut selected, &aliases, &known, &ambiguous, &HashSet::new()).unwrap();
            assert_eq!(selected["when"], json!({"after":actual}));
        }
        let mut missing = json!({"when":{"after":"pause","after_version":3}});
        assert_eq!(
            dependency(&mut missing, &aliases, &known, &ambiguous, &HashSet::new())
                .unwrap_err()
                .code,
            "dependency_unknown"
        );
        let mut malformed = json!({"when":{"after":"pause","after_version":0}});
        assert!(dependency(
            &mut malformed,
            &aliases,
            &known,
            &ambiguous,
            &HashSet::new()
        )
        .is_err());
        assert!(ambiguous.contains("pause"));
    }

    #[test]
    fn parallel_group_is_one_backend_step_with_shared_timing() {
        let commands = json!([
            {"command":"automation.ramp","args":{"target":{"type":"eq_low","deck":"B"},"value":1,"seconds":2}},
            {"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":4}}
        ]);
        let (request, kind, action) = adapt(
            &json!({"op":"add","id":"blend","commands":commands,"when":{"after":"start-b"}}),
            "session",
            "job",
            4,
        )
        .unwrap();
        assert_eq!(kind, "registered");
        assert_eq!(action.as_deref(), Some("blend"));
        assert_eq!(request.args["steps"].as_array().unwrap().len(), 1);
        assert_eq!(
            request.args["steps"][0],
            json!({"action_id":"blend","commands":commands,"when":{"after":"start-b"}})
        );
        for input in [
            json!({"commands":[]}),
            json!({"commands":{}}),
            json!({"commands":[{"command":"engine.command"}]}),
            json!({"commands":[{"command":"deck.load"},{"command":"engine.command"}]}),
            json!({"commands":[{"command":"engine.command","when":{}},{"command":"engine.command"}]}),
            json!({"command":"engine.command","commands":commands}),
            json!({"args":{},"commands":commands}),
        ] {
            assert!(step(&input, "bad").is_err(), "accepted {input}");
        }
    }
    #[test]
    fn status_and_stream_share_logical_item_versions() {
        let items = Mutex::new(HashMap::from([("entry@2".into(), ("entry".into(), 2))]));
        let mut value = json!({"result":{"actions":[{"action_id":"entry@2","state":"queued"}]}});
        annotate_status(&mut value, &items);
        assert_eq!(value["result"]["actions"][0]["item_id"], "entry");
        assert_eq!(value["result"]["actions"][0]["version"], 2);
    }
    #[test]
    fn queued_reads_preserve_canonical_selection_and_timeout() {
        let (request, _, _) = adapt(&json!({"op":"add","id":"read","command":"state.get","args":{},"view":"compact","fields":["/engine/master_gain"],"timeout_ms":8000}), "session", "job", 1).unwrap();
        let mut selected = request.args["steps"][0].clone();
        selected.as_object_mut().unwrap().remove("action_id");
        let canonical: Request = serde_json::from_value(selected).unwrap();
        canonical.validate().unwrap();
        assert_eq!(canonical.view.as_deref(), Some("compact"));
        assert_eq!(canonical.fields, Some(vec!["/engine/master_gain".into()]));
        assert_eq!(canonical.timeout_ms, 8000);
    }
    #[test]
    fn preflight_accepts_the_exact_session_append_without_owning_or_renaming_items() {
        let plan = json!({"op":"add","request_id":"first-pair","items":[
            {"id":"opener","command":"engine.command","args":{"type":"play","deck":"A"}},
            {"id":"incoming","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"opener","type":"at_seconds","clock":"A","seconds":120}},
            {"id":"blend","commands":[{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":16}},{"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"A"},"value":0,"seconds":16}}],"when":{"after":"incoming"}}
        ]});
        let original = plan.clone();
        let args = validation_args(&plan).unwrap();
        assert_eq!(args["steps"][0]["action_id"], "opener");
        assert_eq!(args["steps"][1]["when"]["after"], "opener");
        assert_eq!(args["steps"][2]["commands"], plan["items"][2]["commands"]);
        assert!(args.get("id").is_none());
        assert!(args.get("patch_id").is_none());
        assert_eq!(plan, original);
        assert!(validation_args(&json!({"op":"update","items":[]})).is_err());
        assert!(
            validation_args(&json!({"op":"add","items":[{"command":"engine.command"}]})).is_err()
        );
    }

    #[test]
    fn rejected_session_input_is_compact_and_cannot_look_registered() {
        let response = Response::failure(
            "edit",
            ControlError::new("action_started", "Already executed")
                .with_detail("expected_arguments", json!({"large_schema":true}))
                .with_detail("example", "lower-level batch example")
                .with_detail("hint", "Keep the original recovery hint")
                .with_detail("current_revision", 8),
        );
        let canonical = response_value(response.clone(), "response", "session", None);
        assert!(canonical["error"]["details"]["expected_arguments"].is_object());
        let value = response_value(response, "updated", "session", Some("item"));
        assert_eq!(value["type"], "rejected");
        assert_eq!(value["operation"], "update");
        assert_eq!(value["error"]["details"]["current_revision"], 8);
        assert_eq!(
            value["error"]["details"]["hint"],
            "Keep the original recovery hint"
        );
        assert!(value["error"]["details"]
            .get("expected_arguments")
            .is_none());
        assert_eq!(
            value["error"]["details"]["example"],
            "lower-level batch example"
        );
    }
    #[test]
    fn feedback_keeps_execution_and_errors_without_duplicate_bookkeeping() {
        for event in [
            json!({"type":"job.progress","data":{}}),
            json!({"type":"job.patch","data":{"accepted":true}}),
            json!({"type":"job.action","data":{"state":"registered"}}),
        ] {
            assert!(!session_feedback(&event));
        }
        for event in [
            json!({"type":"job.patch","data":{"accepted":false}}),
            json!({"type":"job.action","data":{"state":"running"}}),
            json!({"type":"job.action","data":{"state":"executed"}}),
            json!({"type":"job.action","data":{"state":"failed"}}),
            json!({"type":"job.action","data":{"state":"cancelled"}}),
            json!({"type":"job.error","data":{}}),
            json!({"type":"job.updated","data":{"state":"failed"}}),
        ] {
            assert!(session_feedback(&event));
        }
    }
    #[test]
    fn help_load_example_uses_the_canonical_spotify_uri_argument() {
        let line = crate::HELP
            .lines()
            .find_map(|line| line.strip_prefix("  Queue load: "))
            .unwrap();
        let input: Value = serde_json::from_str(line).unwrap();
        let request = Request::new(input["command"].as_str().unwrap(), input["args"].clone());
        control_protocol::validate_arguments(&request).unwrap();
        assert!(request.args["uri"]
            .as_str()
            .unwrap()
            .starts_with("spotify:track:"));
        assert!(request.args.get("track").is_none());
    }
    #[test]
    fn explicit_call_retains_canonical_envelope_without_queue_fields() {
        let (request,kind,action) = adapt(&json!({"op":"call","command":"state.get","args":{},"request_id":"inspection","fields":["/engine/decks"]}),"session","job",1).unwrap();
        assert_eq!(request.command, "state.get");
        assert_eq!(request.request_id, "inspection");
        assert_eq!(request.fields, Some(vec!["/engine/decks".into()]));
        assert_eq!(kind, "response");
        assert!(action.is_none());
        assert!(adapt(
            &json!({"op":"call","command":"state.get","bogus":true}),
            "session",
            "job",
            1
        )
        .is_err());
    }
    #[test]
    fn queue_adapter_preserves_dependency_and_one_atomic_replacement() {
        let (request,_,_) = adapt(&json!({"op":"update","id":"start","action_id":"start@2","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"load","type":"after_beats","clock":"A","beats":16,"quantize":4}}),"session","job",8).unwrap();
        assert_eq!(request.command, "batch.append");
        assert_eq!(request.args["invalidate_action_ids"], json!(["start"]));
        assert_eq!(request.args["replacements"], json!({"start":"start@2"}));
        assert_eq!(request.args["expected_revision"], 8);
        assert_eq!(
            request.args["steps"][0]["when"],
            json!({"after":"load","type":"after_beats","clock":"A","beats":16,"quantize":4})
        );
        assert!(request.args["patch_id"].as_str().unwrap().len() <= 128);
    }
}
