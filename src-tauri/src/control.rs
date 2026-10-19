//! Local product control: the desktop process remains the sole runtime owner.
use crate::AppState;
use control_protocol::{ControlError, Request, Response};
use deck_engine::EngineCommand;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
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
    in_flight: Mutex<HashMap<String, Arc<RequestFlight>>>,
    scheduled_submission: Mutex<()>,
    mix_preparing: Mutex<()>,
    mix_prepared: Mutex<HashMap<String, PreparedMix>>,
    mix_session: Mutex<Option<MixSession>>,
    mix_queue: Mutex<Option<SuccessorQueue>>,
    mix_takeover_revision: AtomicU64,
    events: Mutex<VecDeque<Value>>,
    event_ready: Condvar,
    event_sequence: AtomicU64,
    jobs: Mutex<HashMap<String, Job>>,
    journal: Mutex<Option<std::path::PathBuf>>,
    journal_writer: Mutex<()>,
    performance_baseline: Mutex<Option<Value>>,
    job_sequence: AtomicU64,
    _server: Mutex<Option<control_protocol::ipc::Server>>,
}
#[derive(Clone)]
struct Job {
    cancel: Arc<std::sync::atomic::AtomicBool>,
    status: Value,
    native_id: Option<u64>,
    fingerprint: Option<String>,
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
fn event(app: &tauri::AppHandle, kind: &str, mut data: Value) {
    let control = app.state::<Control>();
    if let Ok(mut events) = control.events.lock() {
        let sequence = control.event_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        if kind == "mix.updated" || kind == "mix.queue.updated" {
            data["event_sequence"] = json!(sequence);
        }
        let feedback = if kind == "mix.updated" {
            mix_event_feedback(&data)
        } else if kind == "mix.queue.updated" {
            queue_event_feedback(&data)
        } else {
            data.clone()
        };
        events.push_back(json!({"sequence":sequence,"type":kind,"data":feedback}));
        while events.len() > 512 {
            events.pop_front();
        }
        control.event_ready.notify_all();
        drop(events);
        // The UI receives full options with the same cursor as the compact agent stream.
        if kind == "mix.updated" {
            let _ = app.emit("mix-updated", &data);
        } else if kind == "mix.queue.updated" {
            let _ = app.emit("mix-queue-updated", &data);
        } else if kind == "mix.queue.notice" {
            let _ = app.emit("mix-queue-notice", &data);
        }
    };
}

pub fn start(
    app: &tauri::AppHandle,
    server: control_protocol::ipc::Server,
) -> std::result::Result<(), String> {
    app.manage(Control::default());
    let journal = app
        .state::<AppState>()
        .data_dir
        .join("control-journal.json");
    if let Err(error) = restore_control(app, journal) {
        eprintln!("Control journal: {error}");
    }
    let observer = app.clone();
    std::thread::spawn(move || loop {
        if let Err(error) = poll_mix_session(&observer) {
            eprintln!("Mix session: {error}");
        }
        if let Err(error) = poll_mix_previews(&observer) {
            eprintln!("Mix audition: {error}");
        }
        if let Err(error) = poll_successor_preparation(&observer) {
            eprintln!("Queue preparation: {error}");
        }
        let session_active = observer
            .state::<Control>()
            .mix_session
            .lock()
            .map(|session| session.as_ref().is_some_and(MixSession::active))
            .unwrap_or(false);
        let active = session_active
            || observer
                .state::<Control>()
                .jobs
                .lock()
                .map(|jobs| {
                    jobs.values()
                        .any(|job| job.native_id.is_some() && !terminal_job(job))
                })
                .unwrap_or(false);
        if active {
            if let Err(error) = refresh_scheduled_jobs(&observer) {
                eprintln!("Control journal: {error}");
            }
        }
        if let Err(error) = poll_mix_operations(&observer) {
            eprintln!("Mix feedback: {error}");
        }
        let control = observer.state::<Control>();
        if let Ok(events) = control.events.lock() {
            let _guard = control
                .event_ready
                .wait_timeout(events, Duration::from_millis(if active { 50 } else { 500 }));
        };
    });
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
pub fn show_main_window(app: &tauri::AppHandle) -> Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| ControlError::new("ui_unavailable", "Main window is unavailable"))?;
    window.show().map_err(failure)?;
    window.unminimize().map_err(failure)?;
    window.set_focus().map_err(failure)?;
    Ok(())
}

pub fn prepare_exit(app: &tauri::AppHandle) -> Result<()> {
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    // Finalize the WAV header before the process or its audio callback can disappear.
    if state.engine.recording_status().path.is_some() {
        state.engine.stop_recording().map_err(failure)?;
    }
    if app.try_state::<Control>().is_some() {
        for deck in 0..2 {
            persist_deck(app, deck, None)?;
        }
        persist_control(app)?;
    }
    Ok(())
}

pub fn execute(app: &tauri::AppHandle, request: Request) -> Response {
    if let Err(e) = request.validate() {
        return Response::failure(request.request_id, e);
    }
    if let Err(e) = control_protocol::validate_arguments(&request) {
        return Response::failure(request.request_id, e);
    }
    let mut identity = json!([
        request.command,
        request.args,
        request.expected_load_revision,
        request.no_wait
    ]);
    if let Some(view) = &request.view {
        identity.as_array_mut().unwrap().push(json!({"view": view}));
    }
    let fingerprint = identity.to_string();
    let control = app.state::<Control>();
    let mutation = request.command != "batch.validate"
        && !control_protocol::selection::is_read(&request.command);
    let reservation = match control.reserve(&request, &fingerprint, mutation) {
        Ok(RequestClaim::Owner(owner)) => owner,
        Ok(RequestClaim::Receipt(response)) => {
            return control_protocol::selection::apply(&request, response)
        }
        Ok(RequestClaim::Pending(flight)) => {
            return control_protocol::selection::apply(&request, flight.wait(&request))
        }
        Err(error) => return Response::failure(request.request_id.clone(), error),
    };
    // Only competing scheduled submissions share an ownership lock. Provider,
    // dialog and UI waits never block unrelated native controls or their receipts.
    let _scheduled = if matches!(
        request.command.as_str(),
        "batch.run"
            | "mix.arm"
            | "mix.session.start"
            | "mix.session.stop"
            | "mix.queue.start"
            | "automation.stop"
    ) || matches!(
        request.command.as_str(),
        "batch.append" | "batch.invalidate" | "batch.seal"
    ) {
        Some(
            control
                .scheduled_submission
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )
    } else {
        None
    };
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
    reservation.finish(response.clone());
    if mutation {
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
            let kind = if request.command.contains(".analysis.") {
                "analysis"
            } else if request.command.starts_with("cues.") {
                "cues"
            } else if request.command.starts_with("playlist.") {
                "playlists"
            } else {
                "library"
            };
            let _ = app.emit("library-updated", json!({"command":request.command,"kind":kind,"source":response.result.as_ref().and_then(|v|v.get("source")).or_else(||request.args.get("source")),"cue_revision":response.result.as_ref().and_then(|v|v.get("cue_revision").or_else(||v.get("revision"))),"metadata_revision":state_metadata_revision(app)}));
        }
    }
    if mutation {
        if let Err(error) = persist_control(app) {
            eprintln!("Control journal: {error}");
        }
    }
    let response = control_protocol::selection::apply(&request, response);
    if request.command == "app.stop" && response.ok {
        // Journal persistence can be slower than the response-flush grace period.
        // Start shutdown only after the complete acknowledgement is ready.
        let runtime = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            runtime.exit(0);
        });
    }
    response
}
fn validate_playback_bpm_reference(reference: Option<f64>, bpm: f64, deck: usize) -> Result<()> {
    deck_engine::playback_tempo_percent(reference, bpm)
        .map(|_| ())
        .map_err(|reason| {
            let code = if reference.is_none_or(|value| !value.is_finite() || value <= 0.) {
                "grid_unknown"
            } else {
                "invalid_arguments"
            };
            ControlError::new(code, reason)
                .with_detail("deck", if deck == 0 { "A" } else { "B" })
                .with_detail("reference_bpm", json!(reference))
                .with_detail("requested_playback_bpm", bpm)
        })
}
fn apply(app: &tauri::AppHandle, request: &Request, command: EngineCommand) -> Result<Value> {
    apply_with_control_guard(app, request, command, None)
}
fn apply_with_control_guard(
    app: &tauri::AppHandle,
    request: &Request,
    mut command: EngineCommand,
    expected_control_revision: Option<u64>,
) -> Result<Value> {
    command.validate().map_err(|e| {
        ControlError::new("invalid_arguments", e).with_detail(
            "expected",
            serde_json::to_value(schemars::schema_for!(EngineCommand)).unwrap(),
        )
    })?;
    let state = app.state::<AppState>();
    let first_playback = matches!(command, EngineCommand::Play { .. })
        && state
            .engine
            .transport_snapshot()
            .decks
            .iter()
            .all(|d| !d.playing);
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
        let snap = if matches!(command, EngineCommand::Seek { .. })
            && request.args.get("snap").and_then(Value::as_str) == Some("beat")
        {
            state.engine.snapshot()
        } else {
            state.engine.transport_snapshot()
        };
        if let EngineCommand::SetPlaybackBpm { bpm, .. } = command {
            validate_playback_bpm_reference(snap.decks[d].local_bpm, bpm, d)?;
        }
        if let EngineCommand::Seek { seconds, .. } = &mut command {
            if request.args.get("snap").and_then(Value::as_str) == Some("beat") {
                let d = &snap.decks[d];
                if !d.beatgrid_valid {
                    return Err(ControlError::new(
                        "grid_unknown",
                        "Align or analyze this deck's beatgrid before snapping",
                    ));
                }
                *seconds = d
                    .snapped_grid_time(*seconds)
                    .ok_or_else(|| ControlError::new("grid_unknown", "No usable beat events"))?
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
        ) && state.engine.track_duration(d).is_none()
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
    state.engine.ensure_prepared(&command).map_err(failure)?;
    if expected_control_revision.is_none() && manual_transport_takeover(&command) {
        if let Some(deck) = d {
            if app.state::<Control>().has_pending_load(deck) {
                crate::spotify_decks::cancel_spotify_load(app.clone(), deck).map_err(failure)?;
            }
        }
    }
    if let Some(revision) = expected_control_revision {
        // Recipes apply literal controls; smart controller macros belong to manual gestures.
        let receipt = state
            .engine
            .command_for_control_revision(command.clone(), request.expected_load_revision, revision)
            .map_err(|error| ControlError::new("manual_takeover", error))?;
        state
            .engine
            .wait_for_control_receipt(receipt, Duration::from_millis(request.timeout_ms))
            .map_err(|error| ControlError::new("manual_takeover", error))?;
        state
            .controller
            .lock()
            .map_err(failure)?
            .observe(&command, false);
    } else {
        crate::engine_command_inner(
            app.clone(),
            app.state(),
            command,
            true,
            request.expected_load_revision,
        )
        .map_err(failure)?;
    }
    if request.no_wait {
        return Ok(json!({"accepted":true}));
    }
    let fence = state.engine.fence().map_err(failure)?;
    state
        .engine
        .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
        .map_err(failure)?;
    if first_playback {
        let snapshot = state.engine.transport_snapshot();
        if snapshot.decks.iter().any(|d| d.playing) {
            *app.state::<Control>()
                .performance_baseline
                .lock()
                .map_err(failure)? = Some(
                json!({"instance":std::process::id(),"unix_ms":unix_ms(),"engine_epoch":snapshot.engine_epoch,"rendered_frames":snapshot.rendered_frames,"sample_rate":snapshot.sample_rate}),
            );
        }
    }
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
                | "library.analysis.update"
                | "library.import_xml"
                | "library.write_metadata"
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
        "mix.execute" => return execute_mix(app, request),
        "mix.prepare" => return prepare_mix(app, request),
        "mix.arm" => return arm_mix(app, request),
        "mix.preview" => return preview_mix(app, request),
        "mix.status" => return mix_status(app, request),
        "mix.cancel" => return cancel_mix(app, request),
        "mix.session.start" => return start_mix_session(app, request),
        "mix.session.status" => return status_mix_session(app, request),
        "mix.session.stop" => return stop_mix_session(app, request),
        "mix.queue.start" => return start_successor_queue(app, request),
        "mix.queue.status" => return status_successor_queue(app, request),
        "mix.queue.edit" => return edit_successor_queue(app, request),
        "mix.queue.stop" => return stop_successor_queue(app, request),
        "app.show" => {
            show_main_window(app)?;
            Value::Null
        }
        "skills.welcome" => {
            json!({"show": crate::agent_skills::welcome(&state.data_dir, optional::<bool>(a, "dismiss")?.unwrap_or(false)).map_err(failure)?})
        }
        "skills.install" => {
            let agents = arg::<Vec<crate::agent_skills::Agent>>(a, "agents")?;
            let source = app
                .path()
                .resource_dir()
                .map_err(failure)?
                .join("dj-skills");
            let home = app.path().home_dir().map_err(failure)?;
            serialized(crate::agent_skills::install(&source, &home, agents).map_err(failure)?)?
        }
        "app.hide" => {
            app.get_webview_window("main")
                .ok_or_else(|| ControlError::new("ui_unavailable", "Main window is unavailable"))?
                .hide()
                .map_err(failure)?;
            Value::Null
        }
        "app.stop" => {
            prepare_exit(app)?;
            json!({"shutting_down":true})
        }
        "status" | "state.get" => {
            refresh_scheduled_jobs(app)?;
            let (engine, metadata) = compact_engine_state(&state.engine);
            let mut result = json!({"protocol_version":control_protocol::VERSION,"instance":std::process::id(),"isolated":std::env::var_os("PULSEDECK_ISOLATED").is_some_and(|v|v=="1"),"engine":engine,"metadata":metadata,"controller":crate::get_controller_surface(app.state()),"audio":state.current_audio_status().map_err(failure)?,"midi":state.midi.lock().map_err(failure)?.status(),"recording":state.engine.recording_status(),"source_jobs":state.source_jobs.snapshot(),"analysis":state.analysis.lock().map_err(failure)?.clone(),"performance_baseline":app.state::<Control>().performance_baseline.lock().map_err(failure)?.clone(),"jobs":app.state::<Control>().jobs.lock().map_err(failure)?.iter().map(|(id,j)|json!({"id":id,"state":j.status["state"],"reason":j.status["reason"],"mode":j.status["mode"],"operation_id":j.status["operation_id"]})).collect::<Vec<_>>()});
            if request.view.as_deref() == Some("resume") {
                let mut jobs: Vec<_> = app
                    .state::<Control>()
                    .jobs
                    .lock()
                    .map_err(failure)?
                    .iter()
                    .filter(|(_, j)| j.status["mode"] == "scheduled")
                    .map(|(id, j)| json!({"id":id,"status":job_status(&j.status, a["detail"] == true)}))
                    .collect();
                jobs.sort_by_key(|j| j["status"]["created_unix_ms"].as_u64().unwrap_or(0));
                result["jobs"] = json!(jobs.into_iter().rev().take(1).collect::<Vec<_>>());
                result["journal"] = json!({"durable":true,"replay":false});
            }
            if matches!(request.view.as_deref(), Some("handover" | "compact")) {
                for key in ["controller", "midi", "recording", "analysis"] {
                    result.as_object_mut().unwrap().remove(key);
                }
            }
            result
        }
        "app.state" => call!(crate::get_app_state(
            app.state(),
            optional::<bool>(a, "library")?.unwrap_or(true)
        )),
        "app.activity" => call!(crate::get_activity_state(app.state())),
        "engine.snapshot" => serialized(state.engine.snapshot())?,
        "engine.transport" => {
            let (mut transport, metadata) = compact_engine_state(&state.engine);
            transport["metadata_revision"] = metadata["revision"].clone();
            transport
        }
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
            let spotify_uri = spotify_load_uri(a)?;
            let d = deck(a)?;
            if state.engine.transport_snapshot().decks[d].playing
                && !optional::<bool>(a, "replace_playing")?.unwrap_or(false)
            {
                return Err(ControlError::new(
                    "deck_playing",
                    "Pause the deck or explicitly set replace_playing=true",
                ));
            }
            let mut result = if optional::<bool>(a, "demo")?.unwrap_or(false) {
                asynchronous!(crate::load_demo(app.clone(), d))
            } else if let Some(uri) = spotify_uri {
                asynchronous!(crate::spotify_decks::load_spotify_track(
                    app.clone(),
                    d,
                    uri,
                    Some(request.request_id.clone())
                ))
            } else {
                let path = if let Some(track) = a.get("track") {
                    let track_id = id(&json!({"track":track}), "track")?;
                    state
                        .library
                        .lock()
                        .map_err(failure)?
                        .path_for_id(track_id)
                        .map_err(|_| ControlError::new("track_not_found", "Track not found"))?
                } else {
                    arg(a, "path")?
                };
                asynchronous!(crate::load_track_owned(
                    app.clone(),
                    d,
                    path,
                    request.request_id.clone()
                ))
            };
            let fence = state.engine.fence().map_err(failure)?;
            state
                .engine
                .wait_for_fence(fence, Duration::from_millis(request.timeout_ms))
                .map_err(failure)?;
            if !optional::<bool>(a, "detail")?.unwrap_or(false) {
                compact_control_result(&mut result);
            }
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
        "library.analysis.update" => asynchronous!(crate::update_library_analysis(app.clone())),
        "library.analysis.cancel" => {
            crate::cancel_library_analysis(app.state());
            Value::Null
        }
        "mix.options" => {
            let mut args = a.clone();
            let detail = optional::<bool>(a, "detail")?.unwrap_or(false);
            args.as_object_mut()
                .ok_or_else(|| failure("Expected argument object"))?
                .remove("detail");
            if !detail && args.get("limit").is_none() {
                args["limit"] = json!(5);
            }
            if args.get("deck").is_some() {
                args["deck"] = json!(deck(a)?);
            }
            let request = serde_json::from_value(args).map_err(failure)?;
            let value = asynchronous!(crate::next_move::options(app.clone(), request));
            if detail {
                value
            } else {
                compact_options(value)
            }
        }
        "mix.openers" => opener_candidates(app, a)?,
        "mix.index_scope" => {
            let mut coverage = asynchronous!(crate::spotify::index_mix_scope(
                app.clone(),
                arg(a, "scope")?
            ));
            if !optional::<bool>(a, "detail")?.unwrap_or(false) {
                compact_coverage(&mut coverage);
            }
            coverage
        }
        "mix.index_scope.cancel" => {
            crate::spotify::cancel_mix_scope_index();
            Value::Null
        }
        "library.suggest_next" => asynchronous!(crate::suggest::suggest_next(
            app.clone(),
            if a.get("deck").is_some() {
                Some(deck(a)?)
            } else {
                None
            },
            optional(a, "energy_direction")?,
            Some(crate::suggest::SuggestionOptions {
                sources: optional(a, "sources")?,
                provider: optional(a, "provider")?,
                playlist: optional(a, "playlist")?,
                max_stretch_percent: optional(a, "max_stretch_percent")?,
                allow_half_double: optional(a, "allow_half_double")?,
                limit: optional(a, "limit")?
            })
        )),
        "library.history.clear" => call!(crate::suggest::clear_play_history(app.clone())),
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
            let launches = optional::<bool>(a, "launches")?.unwrap_or(false);
            while d.is_some_and(|d| state.engine.transport_snapshot().decks[d].preparation_pending)
                || ramps
                    && state
                        .engine
                        .transport_snapshot()
                        .active_ramps
                        .iter()
                        .any(|r| *r)
                || launches
                    && state
                        .engine
                        .transport_snapshot()
                        .decks
                        .iter()
                        .any(|d| d.launch_pending)
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
            let source = if let Some(d) = d {
                let snapshot = state.engine.scheduled_snapshot().map_err(failure)?;
                Some(ready_source(&snapshot, d, revision.unwrap())?)
            } else {
                None
            };
            json!({"sequence":fence,"load_revision":revision,"source":source})
        }
        "library.import" => {
            let dialog = dialog_selection(a, &["paths", "folder"])?;
            let kind = optional::<String>(a, "kind")?.unwrap_or_else(|| "files".into());
            let paths = if dialog {
                match kind.as_str() {
                    "files" => tauri::async_runtime::block_on(crate::choose_tracks(app.clone()))
                        .map_err(failure)?,
                    "folder" => tauri::async_runtime::block_on(crate::choose_folder(app.clone()))
                        .map_err(failure)?,
                    _ => {
                        return Err(ControlError::new(
                            "invalid_arguments",
                            "Unknown import kind",
                        ))
                    }
                }
            } else if let Some(folder) = a.get("folder") {
                if kind != "files" {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "kind applies only to dialog imports",
                    ));
                }
                let folder = folder.as_str().ok_or_else(|| {
                    ControlError::new("invalid_arguments", "folder must be a path string")
                })?;
                crate::import_scan::audio_paths(std::path::Path::new(folder)).map_err(failure)?
            } else {
                if kind != "files" {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "kind applies only to dialog imports",
                    ));
                }
                arg(a, "paths")?
            };
            asynchronous!(crate::import_tracks(
                app.clone(),
                paths,
                optional::<bool>(a, "summary")?
            ))
        }
        "library.tracks" => {
            let query = crate::library::BrowserQuery {
                search: optional::<String>(a, "search")?.unwrap_or_default(),
                sort: optional::<String>(a, "sort")?.unwrap_or_else(|| "title".into()),
                playlist: a
                    .get("playlist")
                    .map(|p| id(&json!({"id":p}), "id"))
                    .transpose()?,
                bpm_min: optional(a, "bpm_min")?,
                bpm_max: optional(a, "bpm_max")?,
                offset: optional(a, "offset")?.unwrap_or(0),
                limit: optional::<usize>(a, "limit")?.unwrap_or(100).min(1000),
                filters: optional(a, "filters")?.unwrap_or_default(),
            };
            let library = state.library.lock().map_err(failure)?;
            let mut page = library.browser_page(&query).map_err(failure)?;
            if optional::<bool>(a, "waveform")?.unwrap_or(false) {
                for track in page["tracks"].as_array_mut().unwrap() {
                    let path = track["path"]
                        .as_str()
                        .ok_or_else(|| failure("Track path missing"))?;
                    let full = library
                        .track_by_path(path)
                        .map_err(failure)?
                        .ok_or_else(|| failure("Track no longer exists"))?;
                    let rating = track["rating"].clone();
                    let color = track["color"].clone();
                    *track = serialized(full)?;
                    track["rating"] = rating;
                    track["color"] = color;
                }
            }
            return Ok(("applied", page));
        }
        "library.track" => {
            let library = state.library.lock().map_err(failure)?;
            let path = library.path_for_id(id(a, "id")?).map_err(failure)?;
            serialized(
                library
                    .track_by_path(&path)
                    .map_err(failure)?
                    .ok_or_else(|| failure("Track not found"))?,
            )?
        }
        "library.write_metadata" => asynchronous!(crate::write_library_metadata(
            app.clone(),
            id(a, "id")?,
            arg(a, "revision")?
        )),
        "library.details" => serialized(
            state
                .library
                .lock()
                .map_err(failure)?
                .track_details(id(a, "id")?)
                .map_err(failure)?,
        )?,
        "library.edit_details" => {
            let patch: crate::library::DetailsPatch = arg(a, "patch")?;
            let details = state
                .library
                .lock()
                .map_err(failure)?
                .edit_details(id(a, "id")?, patch)
                .map_err(failure)?;
            let path = state
                .library
                .lock()
                .map_err(failure)?
                .path_for_id(details.id)
                .map_err(failure)?;
            crate::apply_library_names(&state.engine, &path, &details.title, &details.artist);
            serialized(details)?
        }
        "library.tag_categories" => serialized(
            state
                .library
                .lock()
                .map_err(failure)?
                .tag_categories()
                .map_err(failure)?,
        )?,
        "library.edit_taxonomy" => {
            let categories = state
                .library
                .lock()
                .map_err(failure)?
                .edit_taxonomy(
                    &arg::<String>(a, "kind")?,
                    optional(a, "id")?,
                    optional(a, "category")?,
                    &optional::<String>(a, "name")?.unwrap_or_default(),
                    optional(a, "position")?,
                    optional(a, "delete")?.unwrap_or(false),
                )
                .map_err(failure)?;
            serialized(categories)?
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
            compact_neural_summary(&mut track);
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
            let mut track = if let Some(d) = d {
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
            if let Some(d) = d {
                let transport = state.engine.transport_snapshot();
                if transport.load_revisions[d] != metadata.load_revisions[d] {
                    return Err(ControlError::new(
                        "source_changed",
                        "Deck source changed while reading its effective grid",
                    ));
                }
                let effective = &transport.decks[d];
                track["grid_revision"] = json!(effective.grid_revision);
                track["bpm"] = json!(effective.bpm);
                track["beatgrid_offset_seconds"] = json!(effective
                    .beatgrid_valid
                    .then_some(effective.beatgrid_offset_seconds));
                track["beatgrid_valid"] = json!(effective.beatgrid_valid);
                track["beatgrid_downbeat"] = json!(effective.beatgrid_downbeat);
                track["beatgrid_manual"] = json!(effective.beatgrid_manual);
                track["beatgrid_uniform"] = json!(effective.beatgrid_uniform);
                track["beatgrid_anchor_index"] = json!(effective.beatgrid_anchor_index);
                track["beat_position"] = json!(effective.beat_position);
                if effective.beatgrid_manual {
                    track["beatgrid_source"] = json!("manual");
                    track["beatgrid_confidence"] = Value::Null;
                }
            }
            let (cue_revision, cues) = state
                .library
                .lock()
                .map_err(failure)?
                .source_cues(&source)
                .map_err(failure)?;
            track["cue_points"] = json!(cues.points);
            let mut result = crate::track_inspection::view(
                track,
                optional(a, "start_seconds")?.unwrap_or(0.0),
                optional(a, "end_seconds")?,
                optional(a, "points")?.unwrap_or(64),
            )?;
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
            result["grid_revision"] = result["metadata"]
                .get("grid_revision")
                .cloned()
                .unwrap_or(Value::Null);
            result["beat_events"]["grid_revision"] = result["grid_revision"].clone();
            result["cues"] = json!({"revision":cue_revision,"cues":cues});
            result
        }
        "library.remove" => call!(crate::delete_track(app.state(), id(a, "id")?)),
        "library.import_xml" => {
            let dialog = dialog_selection(a, &["path"])?;
            asynchronous!(crate::import_rekordbox_xml(
                app.clone(),
                if dialog { None } else { Some(arg(a, "path")?) },
                optional(a, "summary")?
            ))
        }
        "library.export_xml" => {
            let dialog = dialog_selection(a, &["path"])?;
            let path = tauri::async_runtime::block_on(crate::export_rekordbox_xml_to(
                app.clone(),
                if dialog { None } else { Some(arg(a, "path")?) },
                optional::<bool>(a, "overwrite")?.unwrap_or(false),
            ))
            .map_err(failure)?;
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
                } else if let Some(value) = a.get("point") {
                    let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
                    let mut point = if let Some(id) = id {
                        cues.points
                            .iter()
                            .find(|p| p.id == id)
                            .cloned()
                            .ok_or_else(|| {
                                ControlError::new("invalid_arguments", "Unknown cue point ID")
                            })?
                    } else {
                        let id = format!("user:{}", cues.next_id);
                        cues.next_id += 1;
                        audio_analysis::CuePoint::manual(id, 0., String::new())
                    };
                    let mut merged = serialized(&point)?;
                    let values = value.as_object().ok_or_else(|| {
                        ControlError::new("invalid_arguments", "point must be an object")
                    })?;
                    for (key, value) in values {
                        if ![
                            "id",
                            "start_seconds",
                            "end_seconds",
                            "name",
                            "role",
                            "confirmed",
                        ]
                        .contains(&key.as_str())
                        {
                            return Err(ControlError::new("invalid_arguments","Cue edits accept identity, timing, name, role and confirmation only"));
                        }
                        merged[key] = value.clone();
                    }
                    point = serde_json::from_value(merged).map_err(failure)?;
                    point.modified = true;
                    point.confirmed = true;
                    if let Some(old) = cues.points.iter_mut().find(|p| p.id == point.id) {
                        *old = point;
                    } else {
                        cues.points.push(point);
                    }
                    cues.project();
                } else if let Some(id) = a.get("delete_id").and_then(Value::as_str) {
                    cues.delete(id);
                } else if let Some(bind) = a.get("bind") {
                    let kind: String = arg(bind, "kind")?;
                    let id: Option<String> = optional(bind, "id")?;
                    if id
                        .as_ref()
                        .is_some_and(|id| !cues.points.iter().any(|p| &p.id == id))
                    {
                        return Err(ControlError::new(
                            "invalid_arguments",
                            "Binding requires an existing cue point ID",
                        ));
                    }
                    match kind.as_str() {
                        "main" => cues.bindings.main = id,
                        "hot" => {
                            let i: usize = arg(bind, "index")?;
                            if i >= 8 {
                                return Err(ControlError::new(
                                    "invalid_arguments",
                                    "Hot cue index must be 0..7",
                                ));
                            }
                            cues.bindings.hot[i] = id;
                        }
                        "memory" => {
                            let i: usize = arg(bind, "index")?;
                            if i >= 10 {
                                return Err(ControlError::new(
                                    "invalid_arguments",
                                    "Memory cue index must be 0..9",
                                ));
                            }
                            cues.bindings.memory[i] = id;
                        }
                        _ => {
                            return Err(ControlError::new(
                                "invalid_arguments",
                                "Cue binding kind must be main, hot or memory",
                            ))
                        }
                    };
                    cues.project();
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
                cues.reconcile_legacy(
                    cues.hot_cues,
                    cues.memory_cues,
                    cues.cue_seconds,
                    &cues.annotations.clone(),
                );
                cues.validate(duration).map_err(failure)?;
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
                let metadata = state.engine.metadata_snapshot();
                if a.get("deck").is_some() {
                    let d = deck(a)?;
                    if request
                        .expected_load_revision
                        .is_some_and(|expected| metadata.load_revisions[d] != expected)
                        || metadata.tracks[d].as_ref().is_none_or(|t| t.path != source)
                    {
                        return Err(ControlError::new(
                            "source_changed",
                            "Loaded source changed before cue commit",
                        ));
                    }
                }
                let revision = lib
                    .save_source_cues(&source, &cues, optional(a, "expected_revision")?)
                    .map_err(failure)?;
                // Keep the persistence lock until queued updates apply, preventing
                // the legacy observer from writing older deck values over this edit.
                for (d, t) in metadata.tracks.iter().enumerate() {
                    if t.as_ref().is_some_and(|t| t.path == source) {
                        state
                            .engine
                            .update_loaded_analysis_for_revision(
                                d,
                                &t.as_ref().unwrap().id,
                                Some(metadata.load_revisions[d]),
                                |a| {
                                    a.cue_points = cues.points.clone();
                                },
                            )
                            .map_err(failure)?;
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
            arg(a, "track_ids")?,
            optional(a, "selected_track_id")?
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
            let dialog = dialog_selection(a, &["path"])?;
            asynchronous!(crate::load_sample_at(
                app.clone(),
                slot,
                if dialog { None } else { Some(arg(a, "path")?) }
            ))
        }
        "sampler.state" => {
            let meta = state.engine.metadata_snapshot();
            let engine = state.engine.transport_snapshot();
            json!({"gain":engine.sampler_gain,"slots":meta.samples.iter().enumerate().map(|(i,t)|json!({"slot":i+1,"title":t.as_ref().map(|t|&t.title),"source":t.as_ref().map(|t|&t.path),"duration_seconds":t.as_ref().map(|t|t.duration_seconds),"position_seconds":engine.sampler_positions_seconds[i],"playing":engine.sampler_positions_seconds[i].is_some()})).collect::<Vec<_>>()})
        }
        "recording.status" => serialized(state.engine.recording_status())?,
        "recording.start" => {
            if !state.engine.transport_snapshot().running {
                return Err(ControlError::new(
                    "audio_not_running",
                    "Start audio before recording",
                ));
            }
            let dialog = dialog_selection(a, &["path"])?;
            asynchronous!(crate::start_recording_at(
                app.clone(),
                if dialog { None } else { Some(arg(a, "path")?) },
                optional::<bool>(a, "overwrite")?.unwrap_or(false)
            ))
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
            app.clone(),
            app.state(),
            optional(a, "offset")?.unwrap_or(0)
        )),
        "spotify.playlist_tracks" => asynchronous!(crate::spotify::spotify_playlist_tracks(
            app.clone(),
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
        "events.get" => read_events(app, request)?,
        "batch.validate" => {
            if a.get("schedule").is_some() {
                let snapshot = state
                    .engine
                    .scheduled_snapshot()
                    .map_err(scheduled_failure)?;
                let (schedule, commands) = scheduled_commands_at(request, Some(&snapshot))?;
                let count = commands.len();
                state
                    .engine
                    .validate_scheduled_batch(&schedule, commands)
                    .map_err(scheduled_failure)?;
                return Ok((
                    "applied",
                    json!({"valid":true,"mode":"scheduled","steps":count,"operation_id":schedule.operation_id}),
                ));
            }
            let actions = command_job_actions(&a["steps"], &[], "validate")?;
            if actions.is_empty() && a["open"] != true {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "Closed batches need at least one step",
                ));
            }
            json!({"valid":true,"steps":actions.len(),"mode":"commands"})
        }
        "batch.run" => {
            return start_batch(app, request);
        }
        "batch.append" | "batch.invalidate" | "batch.seal" => {
            return patch_scheduled_batch(app, request);
        }
        "jobs.list" => {
            refresh_scheduled_jobs(app)?;
            serialized(
                app.state::<Control>()
                    .jobs
                    .lock()
                    .map_err(failure)?
                    .iter()
                    .map(|(id, j)| json!({"id":id,"status":job_status(&j.status, a["detail"] == true)}))
                    .collect::<Vec<_>>(),
            )?
        }
        "jobs.get" => {
            refresh_scheduled_jobs(app)?;
            let id: String = arg(a, "id")?;
            let control = app.state::<Control>();
            let jobs = control.jobs.lock().map_err(failure)?;
            let job = jobs
                .get(&id)
                .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?;
            let mut result = job_status(&job.status, a["detail"] == true);
            result["event_sequence"] = json!(control.event_sequence.load(Ordering::Acquire));
            result
        }
        "jobs.cancel" | "automation.stop" => {
            let control = app.state::<Control>();
            if request.command == "automation.stop" {
                let mut stored = control.mix_prepared.lock().map_err(failure)?;
                for p in stored.values_mut().filter(|p| !mix_terminal(&p.state)) {
                    p.cancelled.store(true, Ordering::Release);
                    p.state = "cancelled".into();
                    for deck in 0..2 {
                        state
                            .source_jobs
                            .cancel_owner(deck, &format!("{}:load", p.id));
                    }
                    event(app, "mix.updated", mix_summary(p));
                }
            }
            let jobs = control.jobs.lock().map_err(failure)?;
            if request.command == "automation.stop" {
                manual_mix_session_takeover(app, &EngineCommand::CancelRamps);
                let scheduled = state.engine.transport_snapshot().scheduled_batch;
                if scheduled.state.active() {
                    state
                        .engine
                        .cancel_scheduled_batch(scheduled.id, false)
                        .map_err(failure)?;
                }
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
                        .command(EngineCommand::CancelLaunch { deck: d })
                        .map_err(failure)?;
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
                if let Some(job) = jobs.get(&id) {
                    if let Some(native_id) = job.native_id {
                        state
                            .engine
                            .cancel_scheduled_batch(
                                native_id,
                                optional::<String>(a, "mode")?.as_deref() == Some("stabilize"),
                            )
                            .map_err(failure)?;
                    }
                }
                jobs.get(&id)
                    .ok_or_else(|| ControlError::new("job_not_found", "Job not found"))?
                    .cancel
                    .store(true, Ordering::Release);
            }
            control.event_ready.notify_all();
            Value::Null
        }
        "commands.list" | "capabilities" => {
            let mut result = discovery_catalog(a)?;
            if optional::<bool>(a, "detail")?.unwrap_or(false) {
                result["ui"] = json!(&*app.state::<Control>().ui.lock().map_err(failure)?);
                result["unsupported"] = json!([
                    "mic_input",
                    "android_mono",
                    "smart_fader_next",
                    "spotify_playlist_writes"
                ]);
            }
            result
        }
        "commands.describe" => {
            let name: String = arg(a, "name")?;
            if let Some(action) = optional::<String>(a, "action")? {
                if name != "engine.command" {
                    return Err(ControlError::new(
                        "invalid_arguments",
                        "action is only valid for engine.command",
                    ));
                }
                return Ok((
                    "applied",
                    engine_catalog()
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|entry| entry["type"] == action)
                        .cloned()
                        .ok_or_else(|| {
                            ControlError::new("command_unsupported", "Engine action not found")
                        })?,
                ));
            }
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
                | "library.import"
                | "library.edit_details"
                | "library.edit_taxonomy"
                | "library.write_metadata"
                | "library.import_xml"
                | "library.export_xml"
                | "library.details"
                | "library.tag_categories"
                | "track.inspect"
                | "library.suggest_next"
                | "mix.options"
                | "mix.openers"
                | "mix.index_scope"
                | "mix.index_scope.cancel"
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
fn compact_neural_summary(track: &mut Value) {
    if let Some(rhythm) = track
        .get_mut("neural_rhythm")
        .and_then(Value::as_object_mut)
    {
        for (events, count) in [
            ("beats_seconds", "beat_count"),
            ("downbeats_seconds", "downbeat_count"),
        ] {
            if let Some(times) = rhythm.remove(events) {
                rhythm.insert(count.into(), json!(times.as_array().map_or(0, Vec::len)));
            }
        }
        rhythm.insert("scores_calibrated".into(), json!(false));
    }
}
/// Compact read receipts use the same neural summary as library reads while
/// retaining identities, source guards, stage outcomes and command errors.
fn compact_control_result(value: &mut Value) {
    compact_neural_summary(value);
    match value {
        Value::Object(object) => {
            for key in [
                "waveform",
                "waveform_bands",
                "beat_logits",
                "downbeat_logits",
                "beats_seconds",
                "downbeats_seconds",
                "bar_evidence",
            ] {
                if let Some(payload) = object.remove(key) {
                    if let Some(array) = payload.as_array() {
                        object.insert(format!("{key}_count"), json!(array.len()));
                    }
                }
            }
            for child in object.values_mut() {
                compact_control_result(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                compact_control_result(child);
            }
        }
        _ => {}
    }
}
fn ready_source(
    snapshot: &deck_engine::EngineSnapshot,
    deck: usize,
    revision: u64,
) -> Result<String> {
    if snapshot.load_revisions[deck] != revision {
        return Err(ControlError::new(
            "source_changed",
            "Deck source changed before readiness was verified",
        ));
    }
    if snapshot.decks[deck].preparation_pending {
        return Err(ControlError::new(
            "preparation_pending",
            "Deck preparation is still pending",
        ));
    }
    snapshot.decks[deck]
        .track
        .as_ref()
        .map(|track| track.path.clone())
        .ok_or_else(|| ControlError::new("deck_empty", "No source is loaded on the requested deck"))
}
/// Join identities only across the same source epoch. The real-time transport
/// intentionally contains no TrackAnalysis, which must not serialize as an empty deck.
fn compact_engine_state(engine: &deck_engine::EngineHandle) -> (Value, Value) {
    let metadata = engine.metadata_summary_snapshot();
    let transport = engine.transport_snapshot();
    compact_engine_snapshots(&transport, &metadata)
}
fn metadata_summary(engine: &deck_engine::EngineHandle) -> Value {
    compact_engine_state(engine).1
}
fn compact_engine_snapshots(
    transport: &deck_engine::EngineSnapshot,
    metadata: &deck_engine::EngineMetadataSnapshot,
) -> (Value, Value) {
    let mut engine = json!(transport);
    let mut summary = json!({"revision":metadata.revision,"load_revisions":metadata.load_revisions,"tracks":metadata.tracks.iter().map(|t|t.as_ref().map(|t|json!({"id":t.id,"path":t.path,"title":t.title,"artist":t.artist,"duration_seconds":t.duration_seconds,"bpm":t.bpm,"key":t.key,"recording_allowed":t.recording_allowed}))).collect::<Vec<_>>(),"samples":metadata.samples.iter().map(|t|t.as_ref().map(|t|json!({"id":t.id,"title":t.title,"path":t.path,"duration_seconds":t.duration_seconds}))).collect::<Vec<_>>()});
    for d in 0..2 {
        let deck = &mut engine["decks"][d];
        if transport.load_revisions[d] != metadata.load_revisions[d] {
            // Never attach another release's metadata during a source commit.
            deck.as_object_mut().unwrap().remove("track");
            deck["source_state"] = json!("metadata_pending");
            summary["tracks"][d] = Value::Null;
        } else {
            deck["track"] = summary["tracks"][d].clone();
            deck["source_state"] = json!(if metadata.tracks[d].is_none() {
                "empty"
            } else if transport.decks[d].preparation_pending {
                "preparing"
            } else {
                "ready"
            });
        }
    }
    (engine, summary)
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
    if request.args.get("schedule").is_some() {
        return start_scheduled_batch(app, request);
    }
    start_command_batch(app, request)
}

fn job_cancelled(app: &tauri::AppHandle, request: &Request) -> bool {
    request_owner_cancelled(app, &request.request_id)
}
pub(crate) fn request_owner_cancelled(app: &tauri::AppHandle, request_id: &str) -> bool {
    request_id.split_once(':').is_some_and(|(job, _)| {
        if job.starts_with("mix-queue-") {
            return successor_queue_owner_cancelled(app, job, request_id);
        }
        let control = app.state::<Control>();
        if control
            .mix_prepared
            .lock()
            .map(|mixes| {
                mixes
                    .get(job)
                    .is_some_and(|p| p.cancelled.load(Ordering::Acquire))
            })
            .unwrap_or(false)
        {
            return true;
        }
        app.state::<Control>()
            .jobs
            .lock()
            .map(|jobs| {
                jobs.get(job)
                    .is_some_and(|job| job.cancel.load(Ordering::Acquire))
            })
            .unwrap_or(false)
    })
}

fn validate_control_track_uri(uri: &str) -> Result<()> {
    spotify_playback::validate_track_uri(uri).map_err(|reason| {
        ControlError::new("invalid_track_uri", reason)
            .with_detail("argument", "uri")
            .with_detail("uri", uri)
            .with_detail("hint", "Copy the complete URI exactly from spotify.liked or spotify.playlist results; do not reconstruct or shorten the identifier.")
    })
}
fn validate_step(request: &Request) -> Result<()> {
    control_protocol::validate_arguments(request)?;
    if request.command == "deck.load" {
        spotify_load_uri(&request.args)?;
    }
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
    if value.get("clock").is_some() && value["type"] == "play_quantized" {
        value["clock"] = json!(deck(&json!({"deck":value["clock"]}))?);
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
    for entry in &mut catalog {
        if matches!(
            entry["command"].as_str(),
            Some("batch.run" | "batch.validate")
        ) {
            let mut schedule_schema =
                serde_json::to_value(schemars::schema_for!(deck_engine::BatchSchedule)).unwrap();
            if let Some(required) = schedule_schema["required"].as_array_mut() {
                required.retain(|key| {
                    !matches!(
                        key.as_str(),
                        Some(
                            "expected_load_revisions"
                                | "expected_position_revisions"
                                | "expected_grid_revisions"
                                | "expected_engine_epoch"
                                | "expected_control_revision"
                                | "arm_before_event"
                        )
                    )
                });
            }
            schedule_schema["description"]=json!("Omitted guards capture one current coherent source generation. Native execution rejects any subsequent source/position/grid/control change. Omitted arm_before_event uses the first action event.");
            entry["input_schema"]["properties"]["schedule"] = schedule_schema.clone();
            entry["schedule_schema"] = schedule_schema.clone();
            entry["prerequisite_schema"] =
                serde_json::to_value(schemars::schema_for!(deck_engine::BatchPrerequisite))
                    .unwrap();
            let step_schema =
                serde_json::to_value(schemars::schema_for!(ScheduledRequest)).unwrap();
            entry["scheduled_step_schema"] = step_schema.clone();
            let mut definitions = serde_json::Map::new();
            for schema in [&schedule_schema, &step_schema] {
                if let Some(defined) = schema["definitions"].as_object() {
                    definitions.extend(defined.clone());
                }
            }
            entry["input_schema"]["definitions"] = json!(definitions);
            entry["input_schema"]["properties"]["steps"]["items"] =
                json!({"anyOf":[command_step_schema(),step_schema]});
        }
    }
    if let Some(entry) = catalog
        .iter_mut()
        .find(|entry| entry["command"] == "batch.append")
    {
        let schema = serde_json::to_value(schemars::schema_for!(ScheduledRequest)).unwrap();
        entry["input_schema"]["definitions"] = schema["definitions"].clone();
        entry["input_schema"]["properties"]["steps"]["items"] =
            json!({"anyOf":[schema,command_step_schema()]});
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

fn state_metadata_revision(app: &tauri::AppHandle) -> u64 {
    app.state::<AppState>().engine.metadata_revision()
}

fn dialog_selection(args: &Value, paths: &[&str]) -> Result<bool> {
    let dialog = optional::<bool>(args, "dialog")?.unwrap_or(false);
    let supplied = paths
        .iter()
        .any(|p| args.get(*p).is_some_and(|v| !v.is_null()));
    if dialog == supplied {
        return Err(ControlError::new(
            "invalid_arguments",
            "Supply an explicit path/paths or dialog:true, exclusively",
        ));
    }
    Ok(dialog)
}

// Request-ID reservation is independent of the audio engine. No service or
// provider work runs under either map/cache lock.
struct RequestFlight {
    fingerprint: String,
    load_deck: Option<usize>,
    response: Mutex<Option<Response>>,
    ready: Condvar,
}
enum RequestClaim<'a> {
    Owner(RequestOwner<'a>),
    Receipt(Response),
    Pending(Arc<RequestFlight>),
}
struct RequestOwner<'a> {
    control: &'a Control,
    id: String,
    flight: Arc<RequestFlight>,
    retain: bool,
    done: bool,
}
impl RequestFlight {
    fn wait(&self, request: &Request) -> Response {
        let mut response = self.response.lock().unwrap_or_else(|e| e.into_inner());
        let started = Instant::now();
        while response.is_none() {
            let Some(remaining) =
                Duration::from_millis(request.timeout_ms).checked_sub(started.elapsed())
            else {
                return Response::failure(request.request_id.clone(),ControlError::new("timeout","Original request remains in flight; inspect state/job or retry this same ID"));
            };
            let (next, _) = self
                .ready
                .wait_timeout(response, remaining)
                .unwrap_or_else(|e| e.into_inner());
            response = next;
        }
        response.as_ref().unwrap().clone()
    }
}
impl Control {
    fn reserve<'a>(
        &'a self,
        request: &Request,
        fingerprint: &str,
        retain: bool,
    ) -> Result<RequestClaim<'a>> {
        let mut flights = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        let receipts = self.completed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, previous, response)) =
            receipts.iter().find(|(id, _, _)| id == &request.request_id)
        {
            return if previous == fingerprint {
                Ok(RequestClaim::Receipt(response.clone()))
            } else {
                Err(ControlError::new(
                    "request_id_conflict",
                    "This request ID was used for different arguments",
                ))
            };
        }
        drop(receipts);
        if let Some(flight) = flights.get(&request.request_id) {
            return if flight.fingerprint == fingerprint {
                Ok(RequestClaim::Pending(flight.clone()))
            } else {
                Err(ControlError::new(
                    "request_id_conflict",
                    "This request ID is already in flight with different arguments",
                ))
            };
        }
        if flights.len() >= 256 {
            return Err(ControlError::new(
                "busy",
                "At most 256 requests may be in flight",
            ));
        }
        let load_deck = if matches!(request.command.as_str(), "deck.load" | "deck.demo") {
            deck(&request.args).ok()
        } else {
            None
        };
        let flight = Arc::new(RequestFlight {
            fingerprint: fingerprint.into(),
            load_deck,
            response: Mutex::new(None),
            ready: Condvar::new(),
        });
        flights.insert(request.request_id.clone(), flight.clone());
        Ok(RequestClaim::Owner(RequestOwner {
            control: self,
            id: request.request_id.clone(),
            flight,
            retain,
            done: false,
        }))
    }
    fn has_pending_load(&self, deck: usize) -> bool {
        self.in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|f| f.load_deck == Some(deck))
    }
}
impl RequestOwner<'_> {
    fn finish(mut self, response: Response) {
        self.publish(response);
        self.done = true;
    }
    fn publish(&self, response: Response) {
        let mut flights = self
            .control
            .in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if self.retain {
            let mut receipts = self
                .control
                .completed
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            receipts.push_back((
                self.id.clone(),
                self.flight.fingerprint.clone(),
                response.clone(),
            ));
            while receipts.len() > 256 {
                receipts.pop_front();
            }
        }
        *self
            .flight
            .response
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(response);
        flights.remove(&self.id);
        drop(flights);
        self.flight.ready.notify_all();
    }
}
impl Drop for RequestOwner<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.publish(Response::failure(
                self.id.clone(),
                ControlError::new(
                    "execution_interrupted",
                    "Request execution was interrupted; inspect current state before retrying",
                ),
            ));
        }
    }
}
fn manual_transport_takeover(command: &EngineCommand) -> bool {
    matches!(
        command,
        EngineCommand::Play { .. }
            | EngineCommand::TogglePlay { .. }
            | EngineCommand::Pause { .. }
            | EngineCommand::Seek { .. }
            | EngineCommand::ScrubTo { .. }
            | EngineCommand::ScrubEnd { .. }
            | EngineCommand::Cue { .. }
            | EngineCommand::CuePress { .. }
            | EngineCommand::TriggerHotCue { .. }
            | EngineCommand::Jog { .. }
            | EngineCommand::JogTouch { .. }
    )
}
include!("control_scheduled.rs");
include!("mix_execution.rs");
include!("mix_session.rs");
include!("mix_queue.rs");
include!("control_discovery.rs");
include!("control_command_queue.rs");

#[cfg(test)]
mod request_reservation_tests {
    use super::*;
    #[test]
    fn compact_sources_distinguish_ready_empty_pending_and_replaced_epochs() {
        let engine = deck_engine::EngineHandle::new().unwrap();
        let track = engine.load_demo(0).unwrap();
        let fence = engine.fence().unwrap();
        engine
            .wait_for_fence(fence, Duration::from_secs(2))
            .unwrap();
        let mut transport = engine.transport_snapshot();
        let metadata = engine.metadata_snapshot();
        let (view, summary) = compact_engine_snapshots(&transport, &metadata);
        let compact = engine.metadata_summary_snapshot();
        assert!(compact.tracks[0].as_ref().unwrap().waveform.is_empty());
        assert_eq!(
            (view.clone(), summary.clone()),
            compact_engine_snapshots(&transport, &compact)
        );

        assert_eq!(view["decks"][0]["track"]["path"], track.path);
        assert_eq!(view["decks"][0]["source_state"], "ready");
        assert!(view["decks"][0]["track"].get("waveform").is_none());
        assert_eq!(view["decks"][1]["source_state"], "empty");
        assert!(view["decks"][1]["track"].is_null());
        assert_eq!(view["decks"][0]["track"], summary["tracks"][0]);
        transport.decks[0].preparation_pending = true;
        assert_eq!(
            compact_engine_snapshots(&transport, &metadata).0["decks"][0]["source_state"],
            "preparing"
        );
        transport.load_revisions[0] += 1;
        let (view, summary) = compact_engine_snapshots(&transport, &metadata);
        assert_eq!(view["decks"][0]["source_state"], "metadata_pending");
        assert!(view["decks"][0].get("track").is_none());
        assert!(summary["tracks"][0].is_null());
    }
    #[test]
    fn readiness_requires_a_loaded_source_and_current_epoch() {
        let engine = deck_engine::EngineHandle::new().unwrap();
        let mut snapshot = engine.snapshot();
        assert_eq!(
            ready_source(&snapshot, 0, 0).unwrap_err().code,
            "deck_empty"
        );
        let track = engine.load_demo(0).unwrap();
        let fence = engine.fence().unwrap();
        engine
            .wait_for_fence(fence, Duration::from_secs(2))
            .unwrap();
        snapshot = engine.snapshot();
        let rev = snapshot.load_revisions[0];
        assert_eq!(ready_source(&snapshot, 0, rev).unwrap(), track.path);
        assert_eq!(
            ready_source(&snapshot, 0, rev + 1).unwrap_err().code,
            "source_changed"
        );
        snapshot.decks[0].preparation_pending = true;
        assert_eq!(
            ready_source(&snapshot, 0, rev).unwrap_err().code,
            "preparation_pending"
        );
    }
    fn request(id: &str, command: &str) -> Request {
        let mut request = Request::new(command, json!({"deck":"B"}));
        request.request_id = id.into();
        request
    }
    fn owner<'a>(control: &'a Control, r: &Request, fingerprint: &str) -> RequestOwner<'a> {
        match control.reserve(r, fingerprint, true).unwrap() {
            RequestClaim::Owner(owner) => owner,
            _ => panic!("expected owner"),
        }
    }
    #[test]
    fn held_provider_or_dialog_does_not_block_manual_dispatch() {
        let control = Control::default();
        let load = request("provider", "deck.load");
        let pending = owner(&control, &load, "provider");
        assert!(control.has_pending_load(1));
        assert!(!control.has_pending_load(0));
        let dialog = request("dialog", "sampler.load");
        let dialog_pending = owner(&control, &dialog, "dialog");
        let started = Instant::now();
        let manual = request("manual", "engine.command");
        let manual_owner = owner(&control, &manual, "fader");
        manual_owner.finish(Response::success(
            &manual,
            "applied",
            json!({"manual_takeover":true}),
        ));
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(control.has_pending_load(1));
        pending.finish(Response::failure(
            load.request_id.clone(),
            ControlError::new("cancelled", "manual takeover"),
        ));
        dialog_pending.finish(Response::success(&dialog, "applied", Value::Null));
    }
    #[test]
    fn concurrent_identical_id_executes_once_and_returns_actual_receipt() {
        let control = Arc::new(Control::default());
        let r = request("same", "engine.command");
        let first = owner(&control, &r, "same-payload");
        let flight = match control.reserve(&r, "same-payload", true).unwrap() {
            RequestClaim::Pending(flight) => flight,
            _ => panic!("must join existing execution"),
        };
        let duplicate = r.clone();
        let waiter = std::thread::spawn(move || flight.wait(&duplicate));
        let actual = Response::success(&r, "applied", json!({"sequence":19,"manual_won":true}));
        first.finish(actual.clone());
        let joined = waiter.join().unwrap();
        assert_eq!(joined.result, actual.result);
        assert_eq!(joined.stage, "applied");
        assert_eq!(control.completed.lock().unwrap().len(), 1);
        assert!(matches!(
            control.reserve(&r, "same-payload", true).unwrap(),
            RequestClaim::Receipt(_)
        ));
    }
    #[test]
    fn conflicting_id_rejected_during_and_after_execution() {
        let control = Control::default();
        let r = request("same", "deck.load");
        let first = owner(&control, &r, "original");
        assert!(
            matches!(control.reserve(&r,"changed",true),Err(e)if e.code=="request_id_conflict")
        );
        first.finish(Response::success(&r, "ready", json!({"load_revision":2})));
        assert!(
            matches!(control.reserve(&r,"changed",true),Err(e)if e.code=="request_id_conflict")
        );
    }
    #[test]
    fn duplicate_timeout_does_not_replace_owner_or_create_retry_execution() {
        let control = Control::default();
        let mut r = request("wait", "deck.load");
        r.timeout_ms = 1;
        let first = owner(&control, &r, "same");
        let flight = match control.reserve(&r, "same", true).unwrap() {
            RequestClaim::Pending(f) => f,
            _ => panic!(),
        };
        assert_eq!(flight.wait(&r).error.unwrap().code, "timeout");
        assert!(control.has_pending_load(1));
        first.finish(Response::success(&r, "ready", json!({"load_revision":3})));
        assert!(matches!(control.reserve(&r,"same",true).unwrap(),RequestClaim::Receipt(r)if r.ok));
    }
    #[test]
    fn abandoned_execution_wakes_waiters_with_honest_failure() {
        let control = Control::default();
        let r = request("abandoned", "deck.load");
        let first = owner(&control, &r, "same");
        let flight = match control.reserve(&r, "same", true).unwrap() {
            RequestClaim::Pending(f) => f,
            _ => panic!(),
        };
        drop(first);
        assert_eq!(flight.wait(&r).error.unwrap().code, "execution_interrupted");
        assert!(!control.has_pending_load(1));
    }
    #[test]
    fn provider_takeover_is_source_specific_and_knobs_do_not_cancel_loading() {
        assert!(manual_transport_takeover(&EngineCommand::Play { deck: 1 }));
        assert!(manual_transport_takeover(&EngineCommand::ScrubTo {
            deck: 1,
            seconds: 1.,
            duration_ms: 8.
        }));
        assert!(!manual_transport_takeover(
            &EngineCommand::SetChannelFader {
                deck: 0,
                value: 0.2
            }
        ));
    }
}
