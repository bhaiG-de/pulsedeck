//! Local product control: the desktop process remains the sole runtime owner.
use crate::AppState;
use control_protocol::{ControlError, Request, Response};
use deck_engine::EngineCommand;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

type Result<T> = std::result::Result<T, ControlError>;
#[derive(Default)]
pub struct Control {
    ui: Mutex<HashMap<String, Value>>,
    pending: Mutex<HashMap<String, std::sync::mpsc::Sender<Result<Value>>>>,
    completed: Mutex<VecDeque<(String, String, Response)>>,
    execution: Mutex<()>,
    events: Mutex<VecDeque<Value>>,
    event_sequence: AtomicU64,
    jobs: Mutex<HashMap<String, Job>>,
    job_sequence: AtomicU64,
    _server: Mutex<Option<control_protocol::ipc::Server>>,
}
#[derive(Clone)]
struct Job {
    cancel: Arc<std::sync::atomic::AtomicBool>,
    status: Value,
}
fn failure(e: impl ToString) -> ControlError {
    let message = e.to_string();
    let lower = message.to_lowercase();
    let code = if message.contains("Timed out") {
        "timeout"
    } else if lower.contains("cancelled") {
        "cancelled"
    } else if lower.contains("newer load")
        || lower.contains("superseded")
        || lower.contains("loaded source revision")
    {
        "source_changed"
    } else if lower.contains("streaming-service track")
        || lower.contains("recording is unavailable")
    {
        "recording_prohibited"
    } else if lower.ends_with("(os error 17)") {
        "file_exists"
    } else if message.starts_with("Expected ")
        || message.starts_with("Invalid ")
        || message.starts_with("Choose ")
        || message.starts_with("Select ")
        || lower.contains("must be")
    {
        "invalid_arguments"
    } else if message.contains("queue is full") || message.contains("queue full") {
        "queue_full"
    } else if message.contains("revision conflict") {
        "revision_conflict"
    } else if message.contains("not found") {
        "not_found"
    } else if message.contains("Configure a Spotify Client ID") {
        "configuration_required"
    } else if lower.contains("sign in") || lower.contains("sign-in") {
        "auth_required"
    } else {
        "operation_failed"
    };
    let mut error = ControlError::new(code, message);
    error.retryable = code == "queue_full";
    error
}
fn serialized<T: serde::Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(failure)
}
fn arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    serde_json::from_value(args.get(key).cloned().ok_or_else(|| {
        ControlError::new("invalid_arguments", format!("Missing argument: {key}"))
    })?)
    .map_err(|e| ControlError::new("invalid_arguments", format!("{key}: {e}")))
}
fn optional<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| ControlError::new("invalid_arguments", format!("{key}: {e}"))),
    }
}
fn deck(args: &Value) -> Result<usize> {
    let value = args
        .get("deck")
        .ok_or_else(|| ControlError::new("invalid_arguments", "Missing deck A/B"))?;
    match value {
        Value::String(s) if s.eq_ignore_ascii_case("A") => Ok(0),
        Value::String(s) if s.eq_ignore_ascii_case("B") => Ok(1),
        Value::Number(n) if n.as_u64() == Some(0) => Ok(0),
        Value::Number(n) if n.as_u64() == Some(1) => Ok(1),
        _ => Err(ControlError::new(
            "invalid_arguments",
            "Deck must be A or B",
        )),
    }
}
fn id(args: &Value, key: &str) -> Result<i64> {
    if let Some(s) = args.get(key).and_then(Value::as_str) {
        return s
            .strip_prefix("local:")
            .unwrap_or(s)
            .parse()
            .map_err(|_| ControlError::new("invalid_arguments", "Expected local:ID"));
    }
    arg(args, key)
}
fn event(app: &tauri::AppHandle, kind: &str, data: Value) {
    let control = app.state::<Control>();
    let sequence = control.event_sequence.fetch_add(1, Ordering::AcqRel) + 1;
    if let Ok(mut events) = control.events.lock() {
        events.push_back(json!({"sequence":sequence,"type":kind,"data":data}));
        while events.len() > 512 {
            events.pop_front();
        }
    };
}
pub fn start(app: &tauri::AppHandle) -> std::result::Result<(), String> {
    app.manage(Control::default());
    let runtime = app.clone();
    let server =
        control_protocol::ipc::Server::start(control_protocol::socket_path(), move |request| {
            execute(&runtime, request)
        })
        .map_err(|e| e.to_string())?;
    *app.state::<Control>()
        ._server
        .lock()
        .map_err(|_| crate::poisoned())? = Some(server);
    Ok(())
}
#[tauri::command]
pub async fn control_request(app: tauri::AppHandle, request: Request) -> Response {
    let id = request.request_id.clone();
    tauri::async_runtime::spawn_blocking(move || execute(&app, request))
        .await
        .unwrap_or_else(|e| Response::failure(id, failure(e)))
}
#[tauri::command]
pub fn ui_publish(
    app: tauri::AppHandle,
    target: String,
    snapshot: Value,
) -> std::result::Result<(), String> {
    if target.len() > 80 || snapshot.to_string().len() > 256_000 {
        return Err("UI snapshot exceeds limits".into());
    }
    let control = app.state::<Control>();
    let mut ui = control.ui.lock().map_err(|_| crate::poisoned())?;
    if snapshot.is_null() {
        ui.remove(&target);
    } else {
        ui.insert(target, snapshot);
    }
    Ok(())
}
#[tauri::command]
pub fn ui_reply(
    app: tauri::AppHandle,
    request_id: String,
    result: Option<Value>,
    error: Option<ControlError>,
) {
    if let Ok(mut pending) = app.state::<Control>().pending.lock() {
        if let Some(sender) = pending.remove(&request_id) {
            let _ = sender.send(error.map_or_else(|| Ok(result.unwrap_or(Value::Null)), Err));
        }
    }
}
pub fn execute(app: &tauri::AppHandle, request: Request) -> Response {
    if let Err(e) = request.validate() {
        return Response::failure(request.request_id, e);
    }
    if let Err(e) = control_protocol::validate_arguments(&request) {
        return Response::failure(request.request_id, e);
    }
    let fingerprint = json!([
        request.command,
        request.args,
        request.expected_load_revision,
        request.no_wait
    ])
    .to_string();
    let control = app.state::<Control>();
    // A duplicate request cannot interleave between checking receipts and execution.
    // Read-only queries remain available while a load or UI request is waiting.
    let mutation = !matches!(
        request.command.as_str(),
        "status"
            | "state.get"
            | "engine.transport"
            | "engine.metadata"
            | "events.get"
            | "jobs.get"
            | "jobs.list"
            | "ui.state"
            | "commands.list"
            | "commands.describe"
            | "capabilities"
            | "jobs.cancel"
            | "automation.stop"
            | "deck.load.cancel"
            | "spotify.analysis.cancel"
            | "engine.snapshot"
            | "app.state"
            | "app.activity"
            | "library.tracks"
            | "library.track"
            | "track.inspect"
            | "playlist.list"
            | "cues.get"
            | "recording.status"
            | "controller.state"
            | "midi.history"
            | "midi.ports"
            | "audio.devices"
            | "spotify.status"
            | "spotify.native.status"
            | "spotify.playlists"
            | "spotify.liked"
            | "spotify.playlist_tracks"
            | "spotify.album_tracks"
            | "spotify.devices"
            | "spotify.analysis.get"
            | "spotify.analysis.status"
            | "sampler.state"
    );
    let _execution = if mutation {
        Some(control.execution.lock().unwrap_or_else(|e| e.into_inner()))
    } else {
        None
    };
    if let Some((_, previous, response)) = control
        .completed
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(id, _, _)| id == &request.request_id)
    {
        return if previous == &fingerprint {
            response.clone()
        } else {
            Response::failure(
                request.request_id,
                ControlError::new(
                    "request_id_conflict",
                    "This request ID was used for different arguments",
                ),
            )
        };
    }
    let response = match dispatch(app, &request) {
        Ok((stage, result)) => Response::success(&request, stage, result),
        Err(mut e) => {
            e = e.with_detail("command", request.command.clone());
            if request.command == "ui.call" {
                e = e
                    .with_detail("target", request.args["target"].clone())
                    .with_detail("action", request.args["action"].clone());
                if e.code == "invalid_arguments" {
                    let mut corrected = request.args.clone();
                    let expected = &e.details["expected"];
                    corrected["value"] = if let Some(values) = expected["enum"].as_array() {
                        values.first().cloned().unwrap_or(Value::Null)
                    } else {
                        match expected["type"].as_str() {
                            Some("number" | "integer") => {
                                let min = expected["minimum"].as_f64().unwrap_or(0.0);
                                let max = expected["maximum"].as_f64().unwrap_or(min.max(1.0));
                                let n = request.args["value"]
                                    .as_f64()
                                    .unwrap_or(min)
                                    .clamp(min, max);
                                if expected["type"] == "integer" {
                                    json!(n.floor() as i64)
                                } else {
                                    json!(n)
                                }
                            }
                            Some("boolean") => json!(true),
                            Some("string") => json!(request.args["value"].to_string()),
                            Some("object") => json!({}),
                            _ => request.args["value"].clone(),
                        }
                    };
                    if let Some(argument) = e.details["argument"].as_str() {
                        if let Some(object) = corrected["value"].as_object_mut() {
                            object.remove(argument);
                        }
                    }
                    e = e.with_detail(
                        "example",
                        format!(
                            "pulsedeckctl call ui.call --args '{}' --json",
                            corrected.to_string().replace('\'', "'\\''")
                        ),
                    );
                }
            }
            if let Some(descriptor) = catalog().iter().find(|d| d["command"] == request.command) {
                if e.details.get("example").is_none() {
                    e = e.with_detail("example", descriptor["example_cli"].clone());
                }
                if e.details.get("expected").is_none() {
                    e = e.with_detail("expected_arguments", descriptor["input_schema"].clone());
                }
            }
            Response::failure(request.request_id.clone(), e)
        }
    };
    if mutation {
        let mut receipts = control.completed.lock().unwrap_or_else(|e| e.into_inner());
        receipts.push_back((request.request_id.clone(), fingerprint, response.clone()));
        while receipts.len() > 256 {
            receipts.pop_front();
        }
        event(
            app,
            "command.completed",
            json!({"request_id":request.request_id,"command":request.command,"ok":response.ok,"stage":response.stage}),
        );
        if response.ok
            && (request.command.starts_with("playlist.")
                || request.command.starts_with("library.")
                || request.command.starts_with("cues."))
        {
            let _ = app.emit("library-updated", ());
        }
    }
    response
}
fn apply(app: &tauri::AppHandle, request: &Request, mut command: EngineCommand) -> Result<Value> {
    command.validate().map_err(|e| {
        ControlError::new("invalid_arguments", e).with_detail(
            "expected",
            serde_json::to_value(schemars::schema_for!(EngineCommand)).unwrap(),
        )
    })?;
    let state = app.state::<AppState>();
    let d = command.deck();
    let cue_edit = matches!(
        &command,
        EngineCommand::SetCue { .. }
            | EngineCommand::SetCuePoint { .. }
            | EngineCommand::SetHotCue { .. }
            | EngineCommand::SetHotCuePoint { .. }
            | EngineCommand::ClearHotCue { .. }
            | EngineCommand::RestoreHotCues { .. }
            | EngineCommand::MemoryCueSave { .. }
            | EngineCommand::MemoryCueDelete { .. }
            | EngineCommand::RestoreMemoryCues { .. }
    );
    let source_revision = d.map(|d| state.engine.load_revision(d));
    if let Some(d) = d {
        if d > 1 {
            return Err(ControlError::new("invalid_arguments", "Deck must be A/B"));
        }
        if request
            .expected_load_revision
            .is_some_and(|r| state.engine.load_revision(d) != r)
        {
            return Err(ControlError::new(
                "source_changed",
                "Deck contains another loaded source",
            ));
        }
        let snap = state.engine.snapshot();
        if let EngineCommand::Seek { seconds, .. } = &mut command {
            if request.args.get("snap").and_then(Value::as_str) == Some("beat") {
                let d = &snap.decks[d];
                let period = 60.0 / d.bpm.unwrap_or(120.0);
                *seconds = (((*seconds - d.beatgrid_offset_seconds) / period).round() * period
                    + d.beatgrid_offset_seconds)
                    .clamp(0.0, d.track.as_ref().map_or(0.0, |t| t.duration_seconds));
            }
        }
        if matches!(
            command,
            EngineCommand::Play { .. }
                | EngineCommand::TogglePlay { .. }
                | EngineCommand::SetHotCue { .. }
                | EngineCommand::SetHotCuePoint { .. }
                | EngineCommand::TriggerHotCue { .. }
                | EngineCommand::Seek { .. }
                | EngineCommand::SetCue { .. }
                | EngineCommand::MemoryCueSave { .. }
        ) && snap.decks[d].track.is_none()
        {
            let deck = if d == 0 { "A" } else { "B" };
            return Err(ControlError::new(
                "deck_empty",
                format!("Deck {deck} has no loaded track"),
            )
            .with_detail("deck", deck)
            .with_detail("expected_state", json!({"track_loaded":true}))
            .with_detail(
                "example",
                format!("pulsedeckctl deck {deck} load --path /music/song.flac --json"),
            ));
        }
        if let EngineCommand::TriggerHotCue { index, .. } = command {
            if snap.decks[d]
                .hot_cues
                .get(index)
                .is_none_or(Option::is_none)
            {
                let deck = if d == 0 { "A" } else { "B" };
                let slot = (b'A' + index as u8) as char;
                return Err(ControlError::new(
                    "cue_empty",
                    format!("Deck {deck} hot cue {slot} is empty"),
                )
                .with_detail("deck", deck)
                .with_detail("slot", slot.to_string())
                .with_detail(
                    "example",
                    format!("pulsedeckctl deck {deck} hotcue {slot} set --at 0s --json"),
                ));
            }
        }
        if let EngineCommand::SetHotCuePoint { seconds, .. } = command {
            if !(0.0..=state.engine.track_duration(d).unwrap_or(0.0)).contains(&seconds) {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "Cue time is outside track duration",
                )
                .with_detail("argument", "seconds")
                .with_detail(
                    "expected",
                    json!({"minimum":0,"maximum":state.engine.track_duration(d)}),
                ));
            }
        }
        // Partial controls resolve on the server and preserve the other bands.
        if let EngineCommand::SetEq { low, mid, high, .. } = &mut command {
            if let Some(partial) = request.args.get("bands") {
                *low = optional(partial, "low")?.unwrap_or(snap.decks[d].eq_low);
                *mid = optional(partial, "mid")?.unwrap_or(snap.decks[d].eq_mid);
                *high = optional(partial, "high")?.unwrap_or(snap.decks[d].eq_high);
            }
        }
    }
    command
        .validate()
        .map_err(|e| ControlError::new("invalid_arguments", e))?;
    crate::engine_command_inner(
        app.clone(),
        app.state(),
        command,
        true,
        request.expected_load_revision,
    )
    .map_err(failure)?;
    if request.no_wait {
        return Ok(json!({"accepted":true}));
    }
    let fence = state.engine.fence().map_err(failure)?;
    state
        .engine
        .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
        .map_err(failure)?;
    let persisted = if let Some(d) = d {
        if Some(state.engine.load_revision(d)) != source_revision {
            return Err(ControlError::new(
                "source_changed",
                "Source changed before this command completed",
            )
            .with_detail("expected_load_revision", json!(source_revision))
            .with_detail("actual_load_revision", state.engine.load_revision(d))
            .with_detail("sequence", fence));
        }
        if cue_edit {
            persist_deck(app, d, source_revision)?
        } else {
            None
        }
    } else {
        None
    };
    Ok(
        json!({"sequence":fence,"load_revision":d.map(|d|state.engine.load_revision(d)),"cue_revision":persisted,"engine":state.engine.transport_snapshot()}),
    )
}
pub fn restore_cues(
    app: &tauri::AppHandle,
    d: usize,
    source: &str,
) -> std::result::Result<(), String> {
    let state = app.state::<AppState>();
    let (_, cues) = state
        .library
        .lock()
        .map_err(|_| crate::poisoned())?
        .source_cues(source)?;
    state.engine.command(EngineCommand::RestoreHotCues {
        deck: d,
        cues: cues.hot_cues,
    })?;
    state.engine.command(EngineCommand::RestoreMemoryCues {
        deck: d,
        cues: cues.memory_cues,
    })?;
    state.engine.command(EngineCommand::SetCuePoint {
        deck: d,
        seconds: cues.cue_seconds,
    })
}
fn persist_deck(app: &tauri::AppHandle, d: usize, expected: Option<u64>) -> Result<Option<u64>> {
    let state = app.state::<AppState>();
    let metadata = state.engine.metadata_snapshot();
    let snapshot = state.engine.transport_snapshot();
    if snapshot.load_revisions[d] != metadata.load_revisions[d] {
        return Err(ControlError::new(
            "source_not_ready",
            "Deck source is still being applied; wait for load readiness",
        ));
    }
    if expected.is_some_and(|r| r != metadata.load_revisions[d]) {
        return Err(ControlError::new(
            "source_changed",
            "Source changed before cues could be persisted",
        ));
    }
    let Some(track) = &metadata.tracks[d] else {
        return Ok(None);
    };
    if track.path.starts_with("demo:") {
        return Ok(None);
    }
    let lib = state.library.lock().map_err(failure)?;
    let (_, mut cues) = lib.source_cues(&track.path).map_err(failure)?;
    cues.cue_seconds = snapshot.decks[d].cue_seconds;
    cues.hot_cues = snapshot.decks[d].hot_cues;
    cues.update_memory(snapshot.decks[d].memory_cues);
    let revision = lib
        .save_source_cues(&track.path, &cues, None)
        .map_err(|e| ControlError::new("persistence_failed", e))?;
    let other = 1 - d;
    if metadata.tracks[other]
        .as_ref()
        .is_some_and(|t| t.path == track.path)
        && snapshot.load_revisions[other] == metadata.load_revisions[other]
    {
        state
            .engine
            .command_for_revision(
                EngineCommand::RestoreHotCues {
                    deck: other,
                    cues: cues.hot_cues,
                },
                Some(snapshot.load_revisions[other]),
            )
            .map_err(failure)?;
        state
            .engine
            .command_for_revision(
                EngineCommand::RestoreMemoryCues {
                    deck: other,
                    cues: cues.memory_cues,
                },
                Some(snapshot.load_revisions[other]),
            )
            .map_err(failure)?;
        state
            .engine
            .command_for_revision(
                EngineCommand::SetCuePoint {
                    deck: other,
                    seconds: cues.cue_seconds,
                },
                Some(snapshot.load_revisions[other]),
            )
            .map_err(failure)?;
        let fence = state.engine.fence().map_err(failure)?;
        state
            .engine
            .wait_for_fence(fence, Duration::from_secs(5))
            .map_err(failure)?;
    }
    Ok(Some(revision))
}
fn dispatch(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let a = &request.args;
    let state = app.state::<AppState>();
    if a.get("deck").is_some()
        && request
            .expected_load_revision
            .is_some_and(|r| deck(a).is_ok_and(|d| state.engine.load_revision(d) != r))
    {
        return Err(ControlError::new(
            "source_changed",
            "Deck source revision changed",
        ));
    }
    if request.no_wait
        && matches!(
            request.command.as_str(),
            "deck.load"
                | "deck.analyze"
                | "library.import"
                | "library.import_xml"
                | "engine.wait"
                | "ui.call"
        )
    {
        let mut step = request.clone();
        step.no_wait = false;
        let mut batch = request.clone();
        batch.args = json!({"steps":[step]});
        return start_batch(app, &batch);
    }
    macro_rules! call { ($e:expr) => { serialized($e.map_err(failure)?)? }; }
    macro_rules! asynchronous {
        ($e:expr) => {
            call!(tauri::async_runtime::block_on($e))
        };
    }
    let result = match request.command.as_str() {
        "app.show" => {
            app.get_webview_window("main")
                .ok_or_else(|| ControlError::new("ui_unavailable", "Main window is unavailable"))?
                .show()
                .map_err(failure)?;
            Value::Null
        }
        "app.hide" => {
            app.get_webview_window("main")
                .ok_or_else(|| ControlError::new("ui_unavailable", "Main window is unavailable"))?
                .hide()
                .map_err(failure)?;
            Value::Null
        }
        "app.stop" => {
            for d in 0..2 {
                persist_deck(app, d, None)?;
            }
            if state.engine.recording_status().active {
                state.engine.stop_recording().map_err(failure)?;
            }
            let runtime = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                runtime.exit(0);
            });
            json!({"shutting_down":true})
        }
        "status" | "state.get" => {
            json!({"protocol_version":control_protocol::VERSION,"instance":std::process::id(),"isolated":std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v|v=="1"),"engine":state.engine.transport_snapshot(),"metadata":metadata_summary(&state.engine),"controller":crate::get_controller_surface(app.state()),"audio":state.audio_status.lock().map_err(failure)?.clone(),"midi":state.midi.lock().map_err(failure)?.status(),"recording":state.engine.recording_status(),"source_jobs":state.source_jobs.snapshot(),"analysis":state.analysis.lock().map_err(failure)?.clone()})
        }
        "app.state" => call!(crate::get_app_state(app.state())),
        "app.activity" => call!(crate::get_activity_state(app.state())),
        "engine.snapshot" => serialized(state.engine.snapshot())?,
        "engine.transport" => serialized(state.engine.transport_snapshot())?,
        "engine.metadata" => serialized(state.engine.metadata_snapshot())?,
        "engine.command" => {
            let command = parse_engine(a.get("command").cloned().unwrap_or_else(|| a.clone()))?;
            return Ok((
                if request.no_wait {
                    "accepted"
                } else {
                    "applied"
                },
                apply(app, request, command)?,
            ));
        }
        "deck.load" => {
            let d = deck(a)?;
            if state.engine.transport_snapshot().decks[d].playing
                && !optional::<bool>(a, "replace_playing")?.unwrap_or(false)
            {
                return Err(ControlError::new(
                    "deck_playing",
                    "Pause the deck or explicitly set replace_playing=true",
                ));
            }
            let result = if optional::<bool>(a, "demo")?.unwrap_or(false) {
                asynchronous!(crate::load_demo(app.clone(), d))
            } else if let Some(uri) = optional::<String>(a, "uri")? {
                asynchronous!(crate::spotify_decks::load_spotify_track(
                    app.clone(),
                    d,
                    uri
                ))
            } else {
                let path = if let Some(track) = a.get("track") {
                    let tracks = state
                        .library
                        .lock()
                        .map_err(failure)?
                        .tracks()
                        .map_err(failure)?;
                    let track_id = id(&json!({"track":track}), "track")?;
                    tracks
                        .into_iter()
                        .find(|t| t.id == track_id)
                        .ok_or_else(|| ControlError::new("track_not_found", "Track not found"))?
                        .path
                } else {
                    arg(a, "path")?
                };
                asynchronous!(crate::load_track(app.clone(), d, path))
            };
            let fence = state.engine.fence().map_err(failure)?;
            state
                .engine
                .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                .map_err(failure)?;
            return Ok((
                "ready",
                json!({"track":result,"load_revision":state.engine.load_revision(d),"sequence":fence}),
            ));
        }
        "deck.demo" => {
            let d = deck(a)?;
            asynchronous!(crate::load_demo(app.clone(), d))
        }
        "deck.instant_double" => {
            let d = deck(a)?;
            asynchronous!(crate::instant_double(app.clone(), d))
        }
        "deck.unload" => {
            let d = deck(a)?;
            state.source_jobs.cancel(d);
            state.loading[d].fetch_add(1, Ordering::AcqRel);
            state.engine.unload(d).map_err(failure)?;
            Value::Null
        }
        "deck.load.cancel" => call!(crate::spotify_decks::cancel_spotify_load(
            app.clone(),
            deck(a)?
        )),
        "deck.analyze" => asynchronous!(crate::reanalyze_loaded_track(app.clone(), deck(a)?)),
        "automation.ramp" => {
            let mut target: Value = arg(a, "target")?;
            if target.get("deck").is_some() {
                target["deck"] = json!(deck(&target)?);
            }
            let target: deck_engine::ControlTarget =
                serde_json::from_value(target).map_err(failure)?;
            let clock = optional::<Value>(a, "clock")?
                .map(|v| deck(&json!({"deck":v})))
                .transpose()?;
            if clock.is_some_and(|d| !state.engine.transport_snapshot().decks[d].playing) {
                return Err(ControlError::new(
                    "clock_not_playing",
                    "Beat-clock deck must be playing",
                ));
            }
            let command = EngineCommand::Ramp {
                target,
                value: arg(a, "value")?,
                seconds: optional(a, "seconds")?.unwrap_or(1.0),
                clock,
                beats: optional(a, "beats")?,
                quantize: optional(a, "quantize")?,
            };
            return Ok(("applied", apply(app, request, command)?));
        }
        "engine.wait" => {
            let d = optional::<Value>(a, "deck")?.map(|_| deck(a)).transpose()?;
            let revision = d.map(|d| state.engine.load_revision(d));
            if request.expected_load_revision.is_some()
                && request.expected_load_revision != revision
            {
                return Err(ControlError::new("source_changed", "Wait source guard does not match the current deck; supply deck and its current load revision"));
            }
            let fence = state.engine.fence().map_err(failure)?;
            state
                .engine
                .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                .map_err(failure)?;
            let start = Instant::now();
            let ramps = optional::<bool>(a, "ramps")?.unwrap_or(false);
            while d.is_some_and(|d| state.engine.transport_snapshot().decks[d].preparation_pending)
                || ramps
                    && state
                        .engine
                        .transport_snapshot()
                        .active_ramps
                        .iter()
                        .any(|r| *r)
            {
                if d.is_some_and(|d| Some(state.engine.load_revision(d)) != revision) {
                    return Err(ControlError::new(
                        "source_changed",
                        "Deck source changed while waiting",
                    ));
                }
                if job_cancelled(app, request) {
                    return Err(ControlError::new(
                        "cancelled",
                        "Job cancelled while waiting for the engine",
                    ));
                }
                if start.elapsed() >= Duration::from_millis(request.timeout_ms) {
                    return Err(ControlError::new(
                        "timeout",
                        "Preparation or ramps remain pending",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if d.is_some_and(|d| Some(state.engine.load_revision(d)) != revision) {
                return Err(ControlError::new(
                    "source_changed",
                    "Deck source changed before readiness was verified",
                ));
            }
            json!({"sequence":fence,"load_revision":revision})
        }
        "library.import" => asynchronous!(crate::import_tracks(app.clone(), arg(a, "paths")?)),
        "library.tracks" => {
            let mut tracks = state
                .library
                .lock()
                .map_err(failure)?
                .tracks()
                .map_err(failure)?;
            if let Some(playlist) = a.get("playlist") {
                let id = id(&json!({"id":playlist}), "id")?;
                let playlist = state
                    .library
                    .lock()
                    .map_err(failure)?
                    .playlists()
                    .map_err(failure)?
                    .into_iter()
                    .find(|p| p.id == id)
                    .ok_or_else(|| ControlError::new("playlist_not_found", "Playlist not found"))?;
                tracks.retain(|t| playlist.track_ids.contains(&t.id));
            }
            if let Some(search) = optional::<String>(a, "search")? {
                let search = search.to_lowercase();
                tracks.retain(|t| {
                    format!(
                        "{} {} {} {} {}",
                        t.title,
                        t.artist,
                        t.bpm,
                        t.key.as_deref().unwrap_or(""),
                        t.tags.join(" ")
                    )
                    .to_lowercase()
                    .contains(&search)
                });
            }
            match optional::<String>(a, "sort")?.as_deref().unwrap_or("title") {
                "title" => tracks.sort_by_key(|t| t.title.to_lowercase()),
                "bpm" => tracks.sort_by(|a, b| a.bpm.total_cmp(&b.bpm)),
                "duration" => {
                    tracks.sort_by(|a, b| a.duration_seconds.total_cmp(&b.duration_seconds))
                }
                _ => {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "Sort must be title, bpm or duration",
                    ))
                }
            }
            let total = tracks.len();
            let offset = optional::<usize>(a, "offset")?.unwrap_or(0);
            let limit = optional::<usize>(a, "limit")?.unwrap_or(100).min(1000);
            let mut page = serialized(
                tracks
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>(),
            )?;
            if !optional::<bool>(a, "waveform")?.unwrap_or(false) {
                for t in page.as_array_mut().unwrap() {
                    t.as_object_mut().unwrap().remove("waveform");
                    t.as_object_mut().unwrap().remove("waveform_bands");
                }
            }
            json!({"tracks":page,"total":total,"offset":offset,"next_offset":(offset.saturating_add(limit)<total).then_some(offset.saturating_add(limit))})
        }
        "library.track" => {
            let track_id = id(a, "id")?;
            serialized(
                state
                    .library
                    .lock()
                    .map_err(failure)?
                    .tracks()
                    .map_err(failure)?
                    .into_iter()
                    .find(|t| t.id == track_id)
                    .ok_or_else(|| ControlError::new("track_not_found", "Track not found"))?,
            )?
        }
        "library.set_tags" => {
            let track = state
                .library
                .lock()
                .map_err(failure)?
                .set_tags(id(a, "id")?, arg(a, "tags")?)
                .map_err(failure)?;
            let mut track = serialized(track)?;
            track.as_object_mut().unwrap().remove("waveform");
            track.as_object_mut().unwrap().remove("waveform_bands");
            return Ok(("persisted", track));
        }
        "track.inspect" => {
            if a.get("deck").is_some() == a.get("source").is_some() {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "Supply exactly one of source or deck",
                ));
            }
            let source = resolve_source(app, a, request.expected_load_revision)?;
            let metadata = state.engine.metadata_snapshot();
            let d = a.get("deck").map(|_| deck(a)).transpose()?;
            let track = if let Some(d) = d {
                if request
                    .expected_load_revision
                    .is_some_and(|r| r != metadata.load_revisions[d])
                    || metadata.tracks[d].as_ref().is_none_or(|t| t.path != source)
                {
                    return Err(ControlError::new(
                        "source_changed",
                        "Deck source changed during inspection",
                    ));
                }
                serialized(metadata.tracks[d].as_ref().unwrap())?
            } else {
                let lib = state.library.lock().map_err(failure)?;
                if source.starts_with("spotify:") {
                    serialized(lib.spotify_analysis(&source).map_err(failure)?.ok_or_else(|| ControlError::new("analysis_unavailable", "No cached analysis; inspect a loaded or previously analyzed source"))?)?
                } else if let Some(track) = lib.track_by_path(&source).map_err(failure)? {
                    serialized(track)?
                } else {
                    serialized(
                        metadata
                            .tracks
                            .iter()
                            .flatten()
                            .find(|t| t.path == source)
                            .ok_or_else(|| {
                                ControlError::new(
                                    "track_not_found",
                                    "Import a local file or inspect a loaded deck",
                                )
                            })?,
                    )?
                }
            };
            let mut result = crate::track_inspection::view(
                track,
                optional(a, "start_seconds")?.unwrap_or(0.0),
                optional(a, "end_seconds")?,
                optional(a, "points")?.unwrap_or(64),
            )?;
            let (cue_revision, cues) = state
                .library
                .lock()
                .map_err(failure)?
                .source_cues(&source)
                .map_err(failure)?;
            result["metadata"]["tags"] = json!(state
                .library
                .lock()
                .map_err(failure)?
                .track_by_path(&source)
                .map_err(failure)?
                .map(|t| t.tags)
                .unwrap_or_default());
            result["source"] = json!(source);
            result["requested_source"] = a.get("source").cloned().unwrap_or(Value::Null);
            result["load_revision"] = json!(d.map(|d| metadata.load_revisions[d]));
            result["cues"] = json!({"revision":cue_revision,"cues":cues});
            result
        }
        "library.remove" => call!(crate::delete_track(app.state(), id(a, "id")?)),
        "library.import_xml" => asynchronous!(crate::import_rekordbox_xml(
            app.clone(),
            Some(arg(a, "path")?)
        )),
        "library.export_xml" => {
            use std::io::Write;
            let path: String = arg(a, "path")?;
            let lib = state.library.lock().map_err(failure)?;
            let xml = crate::library::export_xml(
                &lib.tracks().map_err(failure)?,
                &lib.playlists().map_err(failure)?,
            )
            .map_err(failure)?;
            let overwrite = optional::<bool>(a, "overwrite")?.unwrap_or(false);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(overwrite)
                .truncate(overwrite)
                .create_new(!overwrite)
                .open(&path)
                .map_err(failure)?;
            file.write_all(xml.as_bytes()).map_err(failure)?;
            json!({"path":path})
        }
        "playlist.list" => serialized(
            state
                .library
                .lock()
                .map_err(failure)?
                .playlists()
                .map_err(failure)?,
        )?,
        "playlist.create" => call!(crate::create_playlist(app.state(), arg(a, "name")?)),
        "playlist.add" => call!(crate::add_to_playlist(
            app.state(),
            id(a, "playlist")?,
            id(a, "track")?
        )),
        "playlist.edit" => {
            state
                .library
                .lock()
                .map_err(failure)?
                .playlist_edit(
                    id(a, "playlist")?,
                    &arg::<String>(a, "action")?,
                    optional::<String>(a, "name")?.as_deref(),
                    &optional::<Vec<i64>>(a, "tracks")?.unwrap_or_default(),
                )
                .map_err(failure)?;
            Value::Null
        }
        "cues.get" | "cues.set" => {
            let source = resolve_source(app, a, request.expected_load_revision)?;
            let duration = if request.command == "cues.set" {
                Some(source_duration(app, &source)?)
            } else {
                None
            };
            let lib = state.library.lock().map_err(failure)?;
            let (revision, mut cues) = lib.source_cues(&source).map_err(failure)?;
            if request.command == "cues.set" {
                if let Some(value) = a.get("cues") {
                    let values = value.as_object().ok_or_else(|| {
                        ControlError::new("invalid_arguments", "cues must be an object")
                    })?;
                    let mut merged = serialized(&cues)?;
                    for (key, value) in values {
                        merged[key] = value.clone();
                    }
                    cues = serde_json::from_value(merged)
                        .map_err(|e| ControlError::new("invalid_arguments", e.to_string()))?;
                } else {
                    let kind: String = arg(a, "kind")?;
                    let seconds: Option<f64> = optional(a, "seconds")?;
                    if kind == "main" {
                        cues.cue_seconds = seconds.ok_or_else(|| {
                            ControlError::new("invalid_arguments", "Main cue requires seconds")
                        })?;
                    } else {
                        if a.get("seconds").is_none() {
                            return Err(ControlError::new(
                                "invalid_arguments",
                                "Provide seconds for the cue point, or null to clear it",
                            )
                            .with_detail("argument", "seconds")
                            .with_detail(
                                "expected",
                                json!({"type":["number","null"],"minimum":0}),
                            ));
                        }
                        let index: usize = arg(a, "index")?;
                        let slots: &mut [Option<f64>] = match kind.as_str() {
                            "hot" => &mut cues.hot_cues,
                            "memory" => &mut cues.memory_cues,
                            _ => {
                                return Err(ControlError::new(
                                    "invalid_arguments",
                                    "Cue kind must be main, hot or memory",
                                ))
                            }
                        };
                        if index >= slots.len() {
                            return Err(ControlError::new(
                                "invalid_arguments",
                                "Cue index outside capacity",
                            ));
                        }
                        slots[index] = seconds;
                        if seconds.is_none() {
                            cues.annotations.remove(&format!("{kind}:{index}"));
                        }
                        if let Some(label) = optional::<String>(a, "label")? {
                            cues.annotations.insert(format!("{kind}:{index}"), label);
                        }
                    }
                }
                let duration = duration.unwrap();
                cues.normalize_memory();
                if cues.cue_seconds > duration
                    || cues
                        .hot_cues
                        .iter()
                        .chain(cues.memory_cues.iter())
                        .flatten()
                        .any(|s| *s > duration)
                {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "Cue time is outside track duration",
                    ));
                }
                let revision = lib
                    .save_source_cues(&source, &cues, optional(a, "expected_revision")?)
                    .map_err(failure)?;
                // Keep the persistence lock until queued updates apply, preventing
                // the legacy observer from writing older deck values over this edit.
                let metadata = state.engine.metadata_snapshot();
                for (d, t) in metadata.tracks.iter().enumerate() {
                    if t.as_ref().is_some_and(|t| t.path == source) {
                        for command in [
                            EngineCommand::RestoreHotCues {
                                deck: d,
                                cues: cues.hot_cues,
                            },
                            EngineCommand::RestoreMemoryCues {
                                deck: d,
                                cues: cues.memory_cues,
                            },
                            EngineCommand::SetCuePoint {
                                deck: d,
                                seconds: cues.cue_seconds,
                            },
                        ] {
                            state
                                .engine
                                .command_for_revision(command, Some(metadata.load_revisions[d]))
                                .map_err(|e| {
                                    failure(e)
                                        .with_detail("persisted", true)
                                        .with_detail("source", source.clone())
                                        .with_detail("cue_revision", revision)
                                })?;
                        }
                    }
                }
                let fence = state.engine.fence().map_err(failure)?;
                state
                    .engine
                    .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                    .map_err(failure)?;
                return Ok((
                    "persisted",
                    json!({"source":source,"revision":revision,"cues":cues}),
                ));
            }
            json!({"source":source,"revision":revision,"cues":cues})
        }
        "audio.devices" => asynchronous!(crate::list_audio_devices(app.clone())),
        "audio.start" => {
            let routing = optional(a, "routing")?.unwrap_or_default();
            asynchronous!(crate::start_audio(
                app.clone(),
                app.state(),
                optional(a, "device_name")?,
                optional(a, "buffer_frames")?,
                routing
            ))
        }
        "audio.stop" => asynchronous!(crate::stop_audio(app.clone(), app.state())),
        "midi.ports" => call!(crate::list_midi_ports()),
        "midi.connect" => call!(crate::connect_midi(
            app.state(),
            arg(a, "input_name")?,
            optional(a, "output_name")?
        )),
        "midi.disconnect" => call!(crate::disconnect_midi(app.state())),
        "midi.history" => call!(crate::get_midi_history(app.state())),
        "controller.state" => serialized(crate::get_controller_surface(app.state()))?,
        "controller.tempo_range" => call!(crate::set_controller_tempo_range(
            app.clone(),
            app.state(),
            deck(a)?,
            arg(a, "range")?
        )),
        "controller.selection" => call!(crate::set_controller_selection(
            app.state(),
            id(a, "track_id")?
        )),
        "controller.context" => call!(crate::set_controller_browse_context(
            app.state(),
            arg(a, "track_ids")?
        )),
        "controller.browser_active" => call!(crate::set_controller_browser_active(
            app.state(),
            arg(a, "active")?
        )),
        "controller.action" => {
            let action: flx4_midi::ControllerAction =
                serde_json::from_value(a.get("action").cloned().unwrap_or_else(|| a.clone()))
                    .map_err(|e| ControlError::new("invalid_arguments", e.to_string()))?;
            validate_action(&action)?;
            crate::controller::handle(app, action).map_err(failure)?;
            Value::Null
        }
        "mixer.smart_modes" => {
            let control = state.controller.lock().map_err(failure)?;
            let fader = optional(a, "fader")?.unwrap_or(control.smart_fader);
            let cfx = optional(a, "cfx")?.unwrap_or(control.smart_cfx);
            drop(control);
            call!(crate::set_smart_modes(app.state(), fader, cfx))
        }
        "sampler.load" => {
            let slot: usize = arg(a, "slot")?;
            let path: String = arg(a, "path")?;
            serialized(
                state
                    .engine
                    .load_sample(slot, std::path::Path::new(&path))
                    .map_err(failure)?,
            )?
        }
        "sampler.state" => {
            let meta = state.engine.metadata_snapshot();
            let engine = state.engine.transport_snapshot();
            json!({"gain":engine.sampler_gain,"slots":meta.samples.iter().enumerate().map(|(i,t)|json!({"slot":i+1,"title":t.as_ref().map(|t|&t.title),"source":t.as_ref().map(|t|&t.path),"duration_seconds":t.as_ref().map(|t|t.duration_seconds),"position_seconds":engine.sampler_positions_seconds[i],"playing":engine.sampler_positions_seconds[i].is_some()})).collect::<Vec<_>>()})
        }
        "recording.status" => serialized(state.engine.recording_status())?,
        "recording.start" => {
            if !state.engine.transport_snapshot().running {
                return Err(
                    ControlError::new("audio_not_running", "Start audio before recording")
                        .with_detail("example", "pulsedeckctl audio start --json"),
                );
            }
            let path: String = arg(a, "path")?;
            if std::path::Path::new(&path).exists()
                && !optional::<bool>(a, "overwrite")?.unwrap_or(false)
            {
                return Err(ControlError::new(
                    "file_exists",
                    "Recording destination already exists",
                ));
            }
            serialized(
                state
                    .engine
                    .start_recording_with_policy(
                        std::path::Path::new(&path),
                        optional::<bool>(a, "overwrite")?.unwrap_or(false),
                    )
                    .map_err(failure)?,
            )?
        }
        "recording.stop" => asynchronous!(crate::stop_recording(app.clone())),
        "spotify.status" => asynchronous!(crate::spotify::spotify_status(app.state())),
        "spotify.configure" => asynchronous!(crate::spotify::spotify_configure(
            app.clone(),
            app.state(),
            arg(a, "client_id")?
        )),
        "spotify.login" => asynchronous!(crate::spotify::spotify_begin_login(
            app.state(),
            optional(a, "playback")?
        )),
        "spotify.disconnect" => {
            asynchronous!(crate::spotify::spotify_disconnect(app.clone(), app.state()))
        }
        "spotify.native.status" => {
            asynchronous!(crate::spotify::spotify_native_status(app.state()))
        }
        "spotify.native.login" => {
            asynchronous!(crate::spotify::spotify_native_begin_login(app.state()))
        }
        "spotify.native.disconnect" => asynchronous!(crate::spotify::spotify_native_disconnect(
            app.clone(),
            app.state()
        )),
        "spotify.playlists" => asynchronous!(crate::spotify::spotify_playlists(
            app.state(),
            optional(a, "offset")?.unwrap_or(0)
        )),
        "spotify.liked" => asynchronous!(crate::spotify::spotify_liked_tracks(
            app.state(),
            optional(a, "offset")?.unwrap_or(0)
        )),
        "spotify.playlist_tracks" => asynchronous!(crate::spotify::spotify_playlist_tracks(
            app.state(),
            arg(a, "playlist_id")?,
            optional(a, "offset")?.unwrap_or(0)
        )),
        "spotify.album_tracks" => asynchronous!(crate::spotify::spotify_album_tracks(
            app.state(),
            arg(a, "album_id")?,
            optional(a, "album_name")?.unwrap_or_default(),
            optional(a, "offset")?.unwrap_or(0)
        )),
        "spotify.devices" => asynchronous!(crate::spotify::spotify_devices(app.state())),
        "spotify.play" => asynchronous!(crate::spotify::spotify_play(
            app.state(),
            arg(a, "uri")?,
            optional(a, "device_id")?
        )),
        "spotify.pause" => asynchronous!(crate::spotify::spotify_pause(
            app.state(),
            optional(a, "device_id")?
        )),
        "spotify.analysis.get" => call!(crate::spotify_analysis::spotify_cached_analysis(
            app.clone(),
            arg(a, "uris")?
        )),
        "spotify.analysis.start" => call!(crate::spotify_analysis::analyze_spotify_page(
            app.clone(),
            arg(a, "uris")?
        )),
        "spotify.analysis.status" => serialized(crate::spotify_analysis::spotify_analysis_status(
            app.clone(),
        ))?,
        "spotify.analysis.cancel" => {
            crate::spotify_analysis::cancel_spotify_analysis(app.clone());
            Value::Null
        }
        "ui.state" => serialized(&*app.state::<Control>().ui.lock().map_err(failure)?)?,
        "ui.call" => {
            let target: String = arg(a, "target")?;
            let action: String = arg(a, "action")?;
            let control = app.state::<Control>();
            let ui = control.ui.lock().map_err(failure)?;
            let mounted = ui.get(&target).ok_or_else(|| {
                ControlError::new(
                    "ui_unavailable",
                    format!("UI target {target} is not mounted"),
                )
                .with_detail("valid_targets", json!(ui.keys().collect::<Vec<_>>()))
            })?;
            if !mounted["commands"]
                .as_array()
                .is_some_and(|c| c.contains(&Value::String(action.clone())))
            {
                return Err(ControlError::new(
                    "command_unsupported",
                    "Action is not registered on this UI target",
                ).with_detail("target",target.clone()).with_detail("action",action.clone()).with_detail("valid_actions",mounted["commands"].clone()).with_detail("hint",format!("Inspect pulsedeckctl ui state --json, then use pulsedeckctl ui {target} ACTION VALUE.")));
            }
            drop(ui);
            let (sender, receiver) = std::sync::mpsc::channel();
            control
                .pending
                .lock()
                .map_err(failure)?
                .insert(request.request_id.clone(), sender);
            app.emit("product-control",json!({"request_id":request.request_id,"target":target,"action":action,"value":a.get("value").unwrap_or(&Value::Null)})).map_err(failure)?;
            let result = receiver.recv_timeout(Duration::from_millis(request.timeout_ms));
            control
                .pending
                .lock()
                .map_err(failure)?
                .remove(&request.request_id);
            return Ok((
                "rendered",
                result.map_err(|_| {
                    ControlError::new("timeout", "View has not acknowledged this UI action")
                })??,
            ));
        }
        "events.get" => {
            let control = app.state::<Control>();
            let since = optional::<u64>(a, "since")?.unwrap_or(0);
            let events = control.events.lock().map_err(failure)?;
            let gap = events
                .front()
                .and_then(|e| e["sequence"].as_u64())
                .is_some_and(|s| since > 0 && since.saturating_add(1) < s);
            json!({"events":events.iter().filter(|e|e["sequence"].as_u64().unwrap_or(0)>since).cloned().collect::<Vec<_>>(),"resync_required":gap,"sequence":control.event_sequence.load(Ordering::Acquire)})
        }
        "batch.validate" => {
            let steps: Vec<Request> = arg(a, "steps")?;
            if steps.len() > 256 {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "At most 256 batch steps",
                ));
            }
            for step in &steps {
                step.validate()?;
                validate_step(step)?;
                if !catalog().iter().any(|v| v["command"] == step.command) {
                    return Err(ControlError::new(
                        "command_unsupported",
                        format!("Unknown command {}", step.command),
                    ));
                }
            }
            json!({"valid":true,"steps":steps.len()})
        }
        "batch.run" => {
            return start_batch(app, request);
        }
        "jobs.list" => serialized(
            app.state::<Control>()
                .jobs
                .lock()
                .map_err(failure)?
                .iter()
                .map(|(id, j)| json!({"id":id,"status":j.status}))
                .collect::<Vec<_>>(),
        )?,
        "jobs.get" => {
            let id: String = arg(a, "id")?;
            app.state::<Control>()
                .jobs
                .lock()
                .map_err(failure)?
                .get(&id)
                .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?
                .status
                .clone()
        }
        "jobs.cancel" | "automation.stop" => {
            let control = app.state::<Control>();
            let jobs = control.jobs.lock().map_err(failure)?;
            if request.command == "automation.stop" {
                for job in jobs.values() {
                    job.cancel.store(true, Ordering::Release);
                }
                state
                    .engine
                    .command(EngineCommand::CancelRamps)
                    .map_err(failure)?;
                for d in 0..2 {
                    state
                        .engine
                        .command(EngineCommand::CuePress {
                            deck: d,
                            pressed: false,
                        })
                        .map_err(failure)?;
                    state
                        .engine
                        .command(EngineCommand::JogTouch {
                            deck: d,
                            touched: false,
                        })
                        .map_err(failure)?;
                }
            } else {
                let id: String = arg(a, "id")?;
                jobs.get(&id)
                    .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?
                    .cancel
                    .store(true, Ordering::Release);
            }
            Value::Null
        }
        "commands.list" | "capabilities" => {
            json!({"protocol_version":control_protocol::VERSION,"commands":catalog(),"engine_commands":engine_catalog(),"ui":&*app.state::<Control>().ui.lock().map_err(failure)?,"unsupported":["mic_input","android_mono","smart_fader_next","spotify_playlist_writes","structural_section_detection","scheduled_transport"]})
        }
        "commands.describe" => {
            let name: String = arg(a, "name")?;
            catalog()
                .into_iter()
                .find(|v| v["command"] == name)
                .ok_or_else(|| ControlError::new("command_unsupported", "Command not found"))?
        }
        _ => {
            return Err(ControlError::new(
                "command_unsupported",
                format!("Unknown command {}", request.command),
            ))
        }
    };
    if !request.no_wait
        && !matches!(
            request.command.as_str(),
            "status"
                | "state.get"
                | "events.get"
                | "ui.state"
                | "commands.list"
                | "capabilities"
                | "commands.describe"
                | "jobs.get"
                | "jobs.list"
                | "engine.transport"
                | "engine.metadata"
                | "engine.snapshot"
                | "app.state"
                | "app.activity"
                | "library.tracks"
                | "library.track"
                | "track.inspect"
                | "playlist.list"
                | "cues.get"
                | "recording.status"
                | "controller.state"
                | "midi.history"
        )
    {
        let fence = state.engine.fence().map_err(failure)?;
        state
            .engine
            .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
            .map_err(failure)?;
    }
    Ok((
        if request.no_wait {
            "accepted"
        } else {
            "applied"
        },
        result,
    ))
}
fn resolve_source(app: &tauri::AppHandle, a: &Value, expected: Option<u64>) -> Result<String> {
    let state = app.state::<AppState>();
    if a.get("deck").is_some() {
        let metadata = state.engine.metadata_snapshot();
        let d = deck(a)?;
        if expected.is_some_and(|r| r != metadata.load_revisions[d]) {
            return Err(ControlError::new(
                "source_changed",
                "Source changed before resolving this cue edit",
            ));
        }
        return metadata.tracks[d]
            .as_ref()
            .map(|t| t.path.clone())
            .ok_or_else(|| ControlError::new("deck_empty", "Load a source first"));
    }
    let source: String = arg(a, "source")?;
    if let Some(id) = source.strip_prefix("local:") {
        let id: i64 = id.parse().map_err(failure)?;
        return state
            .library
            .lock()
            .map_err(failure)?
            .tracks()
            .map_err(failure)?
            .into_iter()
            .find(|t| t.id == id)
            .map(|t| t.path)
            .ok_or_else(|| ControlError::new("track_not_found", "Track not found"));
    }
    Ok(source)
}
fn source_duration(app: &tauri::AppHandle, source: &str) -> Result<f64> {
    let state = app.state::<AppState>();
    if let Some(track) = state
        .engine
        .metadata_snapshot()
        .tracks
        .iter()
        .flatten()
        .find(|t| t.path == source)
    {
        return Ok(track.duration_seconds);
    }
    let lib = state.library.lock().map_err(failure)?;
    let duration = if source.starts_with("spotify:") {
        lib.spotify_analysis(source)
            .map_err(failure)?
            .map(|t| t.duration_seconds)
    } else {
        lib.track_by_path(source)
            .map_err(failure)?
            .map(|t| t.duration_seconds)
    };
    duration.ok_or_else(|| {
        ControlError::new(
            "track_not_found",
            "Import or analyze source before adding cues",
        )
    })
}
fn metadata_summary(engine: &deck_engine::EngineHandle) -> Value {
    let metadata = engine.metadata_snapshot();
    json!({"revision":metadata.revision,"load_revisions":metadata.load_revisions,"tracks":metadata.tracks.iter().map(|t|t.as_ref().map(|t|json!({"id":t.id,"path":t.path,"title":t.title,"artist":t.artist,"duration_seconds":t.duration_seconds,"bpm":t.bpm,"key":t.key,"recording_allowed":t.recording_allowed}))).collect::<Vec<_>>(),"samples":metadata.samples.iter().map(|t|t.as_ref().map(|t|json!({"id":t.id,"title":t.title,"path":t.path,"duration_seconds":t.duration_seconds}))).collect::<Vec<_>>()})
}
fn validate_action(action: &flx4_midi::ControllerAction) -> Result<()> {
    let value = serialized(action)?;
    if value
        .get("deck")
        .and_then(Value::as_u64)
        .is_some_and(|d| d > 1)
        || value
            .get("pad")
            .and_then(Value::as_u64)
            .is_some_and(|p| p >= 8)
    {
        return Err(ControlError::new(
            "invalid_arguments",
            "Invalid deck or pad",
        ));
    }
    if value
        .get("value")
        .and_then(Value::as_f64)
        .is_some_and(|v| !(0.0..=1.0).contains(&v))
    {
        return Err(ControlError::new(
            "invalid_arguments",
            "Knob value must be 0..1",
        ));
    }
    if (value["shifted"] == true
        && matches!(value["button"].as_str(), Some("loop_in" | "loop_out")))
        || matches!(value["knob"].as_str(), Some("mic_gain"))
        || matches!(
            value["button"].as_str(),
            Some("android_mono" | "smart_fader_next")
        )
    {
        return Err(ControlError::new(
            "command_unsupported",
            "This control has no software implementation",
        ));
    }
    Ok(())
}
fn start_batch(app: &tauri::AppHandle, request: &Request) -> Result<(&'static str, Value)> {
    let steps: Vec<Request> = arg(&request.args, "steps")?;
    if steps.is_empty() || steps.len() > 256 {
        return Err(ControlError::new(
            "invalid_arguments",
            "Batch needs 1..256 steps",
        ));
    }
    for (index, step) in steps.iter().enumerate() {
        step.validate()?;
        validate_step(step).map_err(|e| {
            e.with_detail("step_index", index as u64)
                .with_detail("step_command", step.command.clone())
        })?;
        if step.command == "batch.run" {
            return Err(ControlError::new(
                "invalid_arguments",
                "Nested batches are not supported",
            ));
        }
    }
    let id = format!(
        "job-{}-{}",
        std::process::id(),
        app.state::<Control>()
            .job_sequence
            .fetch_add(1, Ordering::Relaxed)
    );
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = app.state::<Control>();
    if control.jobs.lock().map_err(failure)?.len() >= 256 {
        return Err(ControlError::new(
            "busy",
            "Job history is full; restart the runtime to clear it",
        ));
    }
    control.jobs.lock().map_err(failure)?.insert(
        id.clone(),
        Job {
            cancel: cancel.clone(),
            status: json!({"state":"running","completed":0,"total":steps.len(),"results":[]}),
        },
    );
    let runtime = app.clone();
    let job_id = id.clone();
    std::thread::spawn(move || {
        let mut results = Vec::new();
        let mut final_state = "completed";
        for mut step in steps {
            if cancel.load(Ordering::Acquire) {
                final_state = "cancelled";
                break;
            }
            step.request_id = format!("{}:{}", job_id, results.len());
            let response = execute(&runtime, step);
            let ok = response.ok;
            results.push(response);
            if let Some(job) = runtime
                .state::<Control>()
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(&job_id)
            {
                job.status["completed"] = json!(results.len());
                job.status["results"] = json!(results);
            }
            if !ok {
                final_state = if cancel.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "failed"
                };
                break;
            }
        }
        if let Some(job) = runtime
            .state::<Control>()
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&job_id)
        {
            job.status["state"] = json!(final_state);
        }
        event(
            &runtime,
            "job.completed",
            json!({"id":job_id,"state":final_state}),
        );
    });
    Ok(("accepted", json!({"job_id":id})))
}
fn job_cancelled(app: &tauri::AppHandle, request: &Request) -> bool {
    request.request_id.rsplit_once(':').is_some_and(|(job, _)| {
        app.state::<Control>()
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job)
            .is_some_and(|j| j.cancel.load(Ordering::Acquire))
    })
}

fn validate_step(request: &Request) -> Result<()> {
    control_protocol::validate_arguments(request)?;
    if request.command == "engine.command" {
        let value = request
            .args
            .get("command")
            .cloned()
            .unwrap_or_else(|| request.args.clone());
        let command = parse_engine(value)?;
        command
            .validate()
            .map_err(|e| ControlError::new("invalid_arguments", e))?;
    }
    if request.command.starts_with("batch.") {
        return Err(ControlError::new(
            "invalid_arguments",
            "Nested batches are not supported; flatten the steps",
        ));
    }
    Ok(())
}
fn parse_engine(mut value: Value) -> Result<EngineCommand> {
    if value.get("deck").is_some() {
        value["deck"] = json!(deck(&value)?);
    }
    let examples: Vec<Value> = serde_json::from_str(include_str!("engine-catalog.json")).unwrap();
    let name = value.get("type").and_then(Value::as_str).ok_or_else(|| {
        ControlError::new(
            "invalid_arguments",
            "An engine command requires a string 'type'",
        )
        .with_detail("argument", "type")
    })?;
    let descriptor = examples.iter().find(|e| e["type"] == name).ok_or_else(|| {
        let suggestions = control_protocol::suggestions(
            name,
            examples
                .iter()
                .map(|e| e["type"].as_str().unwrap().to_string()),
        );
        let mut error = ControlError::new(
            "invalid_arguments",
            format!("Unknown engine action '{name}'"),
        )
        .with_detail("argument", "type")
        .with_detail(
            "valid_values",
            json!(examples.iter().map(|e| &e["type"]).collect::<Vec<_>>()),
        )
        .with_detail("suggestions", json!(suggestions));
        if let Some(suggestion) = suggestions
            .first()
            .and_then(|name| examples.iter().find(|e| e["type"] == *name))
        {
            error = error.with_detail(
                "example",
                format!(
                    "pulsedeckctl call engine.command --args '{}' --json",
                    suggestion["example"]
                ),
            );
        }
        error
    })?;
    let valid_fields = descriptor["example"].as_object().unwrap();
    if let Some(key) = value
        .as_object()
        .and_then(|o| o.keys().find(|key| !valid_fields.contains_key(*key)))
    {
        return Err(ControlError::new(
            "invalid_arguments",
            format!("Unknown argument '{key}' for engine action '{name}'"),
        )
        .with_detail("argument", key.to_string())
        .with_detail(
            "valid_arguments",
            json!(valid_fields.keys().collect::<Vec<_>>()),
        )
        .with_detail(
            "suggestions",
            json!(control_protocol::suggestions(
                key,
                valid_fields.keys().cloned()
            )),
        )
        .with_detail(
            "example",
            format!(
                "pulsedeckctl call engine.command --args '{}' --json",
                descriptor["example"]
            ),
        ));
    }
    let example = format!(
        "pulsedeckctl call engine.command --args '{}' --json",
        descriptor["example"]
    );
    let command: EngineCommand = serde_json::from_value(value).map_err(|e| {
        ControlError::new("invalid_arguments", e.to_string())
            .with_detail("example", example.clone())
    })?;
    command
        .validate()
        .map_err(|e| ControlError::new("invalid_arguments", e).with_detail("example", example))?;
    Ok(command)
}

fn catalog() -> Vec<Value> {
    let mut catalog = control_protocol::catalog();
    if let Some(command) = catalog
        .iter_mut()
        .find(|c| c["command"] == "engine.command")
    {
        command["native_input_schema"] =
            serde_json::to_value(schemars::schema_for!(EngineCommand)).unwrap();
    }
    if let Some(command) = catalog
        .iter_mut()
        .find(|c| c["command"] == "controller.action")
    {
        command["native_input_schema"] =
            serde_json::to_value(schemars::schema_for!(flx4_midi::ControllerAction)).unwrap();
    }
    catalog
}
fn engine_catalog() -> Value {
    let mut examples: Value = include_str!("engine-catalog.json")
        .parse()
        .expect("checked-in engine catalog");
    let schema = serde_json::to_value(schemars::schema_for!(EngineCommand)).unwrap();
    for entry in examples.as_array_mut().unwrap() {
        let variant = schema["oneOf"].as_array().and_then(|items| {
            items.iter().find(|v| {
                v["properties"]["type"]["enum"]
                    .as_array()
                    .is_some_and(|values| values.contains(&entry["type"]))
            })
        });
        let mut typed = variant.unwrap_or(&schema).clone();
        typed["definitions"] = schema["definitions"].clone();
        entry["input_schema"] = typed;
    }
    examples
}
