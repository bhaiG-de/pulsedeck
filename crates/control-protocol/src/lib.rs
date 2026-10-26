//! Versioned, bounded local control transport. No audio engine lives in a client.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod commands;
pub mod forecast;
pub mod performance;
pub mod selection;
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EnergyDirection {
    #[default]
    Keep,
    #[serde(alias = "up")]
    Raise,
    #[serde(alias = "down")]
    Lower,
}
impl EnergyDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Raise => "raise",
            Self::Lower => "lower",
        }
    }
}
pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub fn catalog() -> Vec<Value> {
    {
        let mut entries: Vec<Value> =
            serde_json::from_str(include_str!("catalog.json")).expect("checked-in catalog");
        entries.extend(performance::descriptors());
        entries
    }
}
pub fn suggestions(input: &str, candidates: impl IntoIterator<Item = String>) -> Vec<String> {
    fn distance(a: &str, b: &str) -> usize {
        let mut row: Vec<usize> = (0..=b.chars().count()).collect();
        for (i, ac) in a.chars().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, bc) in b.chars().enumerate() {
                let old = row[j + 1];
                row[j + 1] = (row[j + 1] + 1)
                    .min(row[j] + 1)
                    .min(prev + usize::from(ac != bc));
                prev = old;
            }
        }
        *row.last().unwrap()
    }
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .map(|c| (distance(&input.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(d, _)| *d <= 2.max(input.len() / 3))
        .collect();
    ranked.sort();
    ranked.into_iter().take(3).map(|(_, c)| c).collect()
}
pub fn validate_arguments(request: &Request) -> Result<(), ControlError> {
    let descriptors = catalog();
    let Some(descriptor) = descriptors.iter().find(|d| d["command"] == request.command) else {
        let choices = suggestions(
            &request.command,
            descriptors
                .iter()
                .filter_map(|d| d["command"].as_str().map(str::to_owned)),
        );
        return Err(ControlError::new("command_unsupported",format!("Unknown command: {}",request.command)).with_detail("command",request.command.clone()).with_detail("suggestions",serde_json::json!(choices)).with_detail("hint","Use one of the suggested exact method names, or run pulsedeckctl commands list --json."));
    };
    if request.command == "cues.set" {
        let forms = ["cues", "point", "delete_id", "bind", "kind"]
            .into_iter()
            .filter(|key| request.args.get(*key).is_some())
            .collect::<Vec<_>>();
        if forms.len() != 1 {
            return Err(ControlError::new(
                "invalid_arguments",
                "Choose exactly one cue edit form: cues, point, delete_id, bind, or legacy kind",
            ));
        }
        if forms[0] != "kind"
            && ["index", "seconds", "label"]
                .into_iter()
                .any(|key| request.args.get(key).is_some())
        {
            return Err(ControlError::new(
                "invalid_arguments",
                "Legacy index/seconds/label belong only to the kind edit form",
            ));
        }
    }
    if matches!(request.command.as_str(), "cues.get" | "cues.set")
        && request.args.get("source").is_some() == request.args.get("deck").is_some()
    {
        return Err(ControlError::new(
            "invalid_arguments",
            "Cue operations require exactly one of source or deck",
        ));
    }
    let schema = &descriptor["input_schema"];
    let properties = schema["properties"]
        .as_object()
        .expect("catalog object schema");
    let fail = |key: &str, message: String| {
        ControlError::new("invalid_arguments", message)
            .with_detail("command", request.command.clone())
            .with_detail("argument", key.to_string())
            .with_detail(
                "expected",
                properties.get(key).cloned().unwrap_or(Value::Null),
            )
            .with_detail("example", descriptor["example_cli"].clone())
    };
    for key in schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if request.args.get(key).is_none() {
            return Err(fail(key, format!("Missing required argument '{key}'")));
        }
    }
    for (key, value) in request
        .args
        .as_object()
        .ok_or_else(|| ControlError::new("invalid_arguments", "Arguments must be a JSON object"))?
    {
        let Some(property) = properties.get(key) else {
            if schema["additionalProperties"] == false {
                return Err(fail(key, format!("Unknown argument '{key}'"))
                    .with_detail(
                        "valid_arguments",
                        serde_json::json!(properties.keys().collect::<Vec<_>>()),
                    )
                    .with_detail(
                        "suggestions",
                        serde_json::json!(suggestions(key, properties.keys().cloned())),
                    ));
            } else {
                continue;
            }
        };
        let matches_type = |kind: &str| match kind {
            "null" => value.is_null(),
            "boolean" => value.is_boolean(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        };
        let correct = match &property["type"] {
            Value::String(t) => matches_type(t),
            Value::Array(types) => types.iter().filter_map(Value::as_str).any(matches_type),
            _ => true,
        };
        if !correct {
            return Err(fail(
                key,
                format!(
                    "Argument '{key}' has the wrong type; expected {}",
                    property["type"]
                ),
            ));
        }
        if let Some(options) = property["enum"].as_array() {
            let aliased = value
                .as_str()
                .and_then(|s| property["aliases"].get(s))
                .is_some_and(|v| options.contains(v));
            if !options.contains(value) && !aliased {
                return Err(fail(
                    key,
                    format!("Argument '{key}' must be one of {}", property["enum"]),
                ));
            }
        }
        if let Some(n) = value.as_f64() {
            if property["minimum"].as_f64().is_some_and(|min| n < min)
                || property["maximum"].as_f64().is_some_and(|max| n > max)
            {
                return Err(fail(
                    key,
                    format!("Argument '{key}' is outside its allowed range"),
                ));
            }
        }
        if let Some(values) = value.as_array() {
            if property["minItems"]
                .as_u64()
                .is_some_and(|min| values.len() < (min as usize))
                || property["maxItems"]
                    .as_u64()
                    .is_some_and(|max| values.len() > (max as usize))
            {
                return Err(fail(
                    key,
                    format!("Argument '{key}' has an invalid item count"),
                ));
            }
        }
    }
    Ok(())
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Set only by the desktop bridge, never trusted from the wire.
    #[serde(skip)]
    pub from_ui: bool,
    #[serde(default = "version")]
    pub protocol_version: u32,
    #[serde(default = "request_id")]
    pub request_id: String,
    pub command: String,
    #[serde(default = "object")]
    pub args: Value,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub expected_load_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_state: Option<commands::DirectStateGuard>,
    #[serde(default)]
    pub no_wait: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
}
fn version() -> u32 {
    VERSION
}
fn object() -> Value {
    serde_json::json!({})
}
fn timeout() -> u64 {
    30_000
}
fn request_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}
impl Request {
    pub fn new(command: impl Into<String>, args: Value) -> Self {
        Self {
            from_ui: false,
            protocol_version: VERSION,
            request_id: request_id(),
            command: command.into(),
            args,
            timeout_ms: timeout(),
            expected_load_revision: None,
            expected_state: None,
            no_wait: false,
            fields: None,
            view: None,
        }
    }
    pub fn validate(&self) -> Result<(), ControlError> {
        if let Some(guard) = self.expected_state {
            guard
                .validate()
                .map_err(|e| ControlError::new("invalid_arguments", e))?;
            if self.command != "engine.command" || self.no_wait {
                return Err(ControlError::new(
                    "invalid_arguments",
                    "expected_state requires a waiting direct engine.command",
                ));
            }
        }
        selection::validate(self)?;
        if self.protocol_version != VERSION {
            return Err(ControlError::new(
                "protocol_mismatch",
                "Unsupported protocol version",
            ));
        }
        if self.command.is_empty()
            || self.command.len() > 128
            || self.request_id.len() > 128
            || !self.args.is_object()
            || self.args.to_string().len() > 256_000
            || !(1..=180_000).contains(&self.timeout_ms)
        {
            return Err(ControlError::new(
                "invalid_request",
                "Use a command, JSON object arguments up to 256KB and timeout 1..180000ms",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ControlError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default)]
    pub details: Value,
}
impl ControlError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        let hint = match code.as_str() {
            "app_not_running" => "Open PulseDeck, then retry with the same --socket path.",
            "deck_empty" => "Load a track: pulsedeckctl deck A load --path /music/song.flac",
            "deck_playing" => "Pause the deck first, or use --replace-playing for an intentional replacement.",
            "cue_empty" => "Set this slot first: pulsedeckctl deck A hotcue A set --at 32s",
            "source_changed" | "revision_conflict" => "Read status/cues.get, verify the current source and retry with its latest revision.",
            "auth_required" => "Sign in to the appropriate account with spotify login or spotify native login.",
            "source_unavailable" => "Preserve the current carrier and inspect the source error/source_jobs. A provider failure after successful native sign-in does not establish that another login is needed.",
            "provider_cooldown" => "Wait for the stated provider cooldown before another load; keep an already-loaded source available. Do not repeat login or retry across many tracks.",
            "configuration_required" => "Configure your registered client: pulsedeckctl spotify configure --client-id YOUR_32_CHARACTER_CLIENT_ID --json; then run pulsedeckctl spotify login.",
            "audio_not_running" => "Start the audio renderer with pulsedeckctl audio start --json, then retry.",
            "ui_unavailable" => "Open the relevant UI panel, then inspect pulsedeckctl ui state --json for mounted targets.",
            "timeout" => "The action may still be pending. Inspect its job or retry the same request ID; do not blindly duplicate it.",
            "queue_full" | "busy" => "Inspect current jobs, wait for capacity and retry the same request ID.",
            "file_exists" => "Choose a new destination, or pass --overwrite to replace the existing output.",
            _ => "Use pulsedeckctl commands list --json or commands describe METHOD --json for valid syntax and arguments.",
        };
        Self {
            retryable: matches!(code.as_str(), "queue_full" | "busy"),
            code,
            message: message.into(),
            details: serde_json::json!({"hint":hint}),
        }
    }
    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        if !self.details.is_object() {
            self.details = serde_json::json!({});
        }
        self.details[key] = value.into();
        self
    }
    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "invalid_request" | "invalid_arguments" | "protocol_mismatch" => 2,
            "app_not_running" | "transport_error" => 3,
            "timeout" => 6,
            "cancelled" => 7,
            "operation_failed" | "persistence_failed" => 5,
            _ => 4,
        }
    }
}
impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(hint) = self.details.get("hint").and_then(Value::as_str) {
            write!(f, "\n{hint}")?;
        }
        if let Some(example) = self.details.get("example").and_then(Value::as_str) {
            write!(f, "\nExample: {example}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ControlError {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Response {
    pub protocol_version: u32,
    pub request_id: String,
    pub ok: bool,
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlError>,
}
impl Response {
    pub fn success(req: &Request, stage: &str, result: Value) -> Self {
        Self {
            protocol_version: VERSION,
            request_id: req.request_id.clone(),
            ok: true,
            stage: stage.into(),
            result: Some(result),
            error: None,
        }
    }
    pub fn failure(id: impl Into<String>, error: ControlError) -> Self {
        Self {
            protocol_version: VERSION,
            request_id: id.into(),
            ok: false,
            stage: "failed".into(),
            result: None,
            error: Some(error),
        }
    }
}

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PULSEDECK_SOCKET") {
        return path.into();
    }
    // SAFETY: getuid has no pointer arguments or side effects.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/pulsedeck-{uid}/control.sock"))
}

#[cfg(unix)]
pub mod ipc {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    };
    use std::time::Duration;

    fn io_error(e: std::io::Error) -> ControlError {
        ControlError::new(
            if matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                "timeout"
            } else {
                "transport_error"
            },
            e.to_string(),
        )
    }
    fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>, ControlError> {
        let mut frame = Vec::new();
        BufReader::new(stream)
            .take((MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut frame)
            .map_err(io_error)?;
        if frame.len() > MAX_FRAME || frame.last() != Some(&b'\n') {
            return Err(ControlError::new(
                "invalid_request",
                "Frame missing newline or exceeding 4MiB",
            ));
        }
        Ok(frame)
    }
    fn write_frame(stream: &mut UnixStream, value: &impl Serialize) -> Result<(), ControlError> {
        let mut frame = serde_json::to_vec(value)
            .map_err(|e| ControlError::new("operation_failed", e.to_string()))?;
        frame.push(b'\n');
        if frame.len() > MAX_FRAME {
            return Err(ControlError::new(
                "response_too_large",
                "Use a paginated query",
            ));
        }
        stream.write_all(&frame).map_err(io_error)
    }
    pub fn call(path: &Path, request: &Request) -> Result<Response, ControlError> {
        request.validate()?;
        let mut stream = UnixStream::connect(path).map_err(|e| {
            ControlError::new(
                "app_not_running",
                format!("Open PulseDeck first ({}): {e}", path.display()),
            )
        })?;
        let duration = Duration::from_millis(request.timeout_ms + 2000);
        stream.set_read_timeout(Some(duration)).map_err(io_error)?;
        stream.set_write_timeout(Some(duration)).map_err(io_error)?;
        write_frame(&mut stream, request)?;
        let response: Response = serde_json::from_slice(&read_frame(&mut stream)?)
            .map_err(|e| ControlError::new("transport_error", e.to_string()))?;
        if response.protocol_version != VERSION || response.request_id != request.request_id {
            return Err(ControlError::new(
                "protocol_mismatch",
                "Response does not match this request",
            ));
        }
        Ok(response)
    }
    pub struct Server {
        path: PathBuf,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
        _lock: std::fs::File,
    }
    impl Server {
        pub fn start(
            path: PathBuf,
            handler: impl Fn(Request) -> Response + Send + Sync + 'static,
        ) -> Result<Self, ControlError> {
            let parent = path.parent().ok_or_else(|| {
                ControlError::new("invalid_arguments", "Socket needs a parent directory")
            })?;
            // SAFETY: getuid has no pointer arguments or side effects.
            let uid = unsafe { libc::getuid() };
            match std::fs::DirBuilder::new().mode(0o700).create(parent) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io_error(e)),
            }
            let metadata = std::fs::symlink_metadata(parent).map_err(io_error)?;
            if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
                return Err(ControlError::new(
                    "unsafe_socket_path",
                    "Socket directory must be owned by you and mode 0700",
                ));
            }
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path.with_extension("lock"))
                .map_err(io_error)?;
            // SAFETY: flock operates on this valid, owned descriptor. Keeping it
            // open gives the runtime exclusive socket ownership through cleanup.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(ControlError::new(
                    "instance_running",
                    "Another runtime owns this socket lock",
                ));
            }
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if !metadata.file_type().is_socket() || metadata.uid() != uid {
                    return Err(ControlError::new(
                        "unsafe_socket_path",
                        "Refusing to replace non-owned socket",
                    ));
                }
                match UnixStream::connect(&path) {
                    Ok(_) => {
                        return Err(ControlError::new(
                            "instance_running",
                            "Another PulseDeck instance owns this socket",
                        ))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                        std::fs::remove_file(&path).map_err(io_error)?
                    }
                    Err(e) => return Err(io_error(e)),
                }
            }
            let listener = UnixListener::bind(&path).map_err(io_error)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(io_error)?;
            listener.set_nonblocking(true).map_err(io_error)?;
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let handler = Arc::new(handler);
            let active = Arc::new(AtomicUsize::new(0));
            let thread = std::thread::Builder::new()
                .name("pulsedeck-control".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Acquire) {
                        let mut ready = libc::pollfd {
                            fd: listener.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        };
                        // The listener outlives poll; wake on input without a fixed request delay.
                        if unsafe { libc::poll(&mut ready, 1, 50) } <= 0 {
                            continue;
                        }
                        match listener.accept() {
                            Ok((mut stream, _)) => {
                                // macOS accepted sockets inherit the listener's
                                // nonblocking flag; handlers use bounded blocking IO.
                                if stream.set_nonblocking(false).is_err() {
                                    continue;
                                }
                                if active.load(Ordering::Acquire) >= 16 {
                                    continue;
                                }
                                active.fetch_add(1, Ordering::AcqRel);
                                let active = active.clone();
                                let handler = handler.clone();
                                std::thread::spawn(move || {
                                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                                    let response = match read_frame(&mut stream).and_then(|v| {
                                        serde_json::from_slice::<Request>(&v).map_err(|e| {
                                            ControlError::new("invalid_request", e.to_string())
                                        })
                                    }) {
                                        Ok(request) => match request.validate() {
                                            Ok(()) => handler(request),
                                            Err(e) => Response::failure(request.request_id, e),
                                        },
                                        Err(e) => Response::failure("", e),
                                    };
                                    if let Err(e) = write_frame(&mut stream, &response) {
                                        // A transport failure may follow a partial
                                        // frame. Appending another JSON response would
                                        // corrupt the stream. Oversize validation runs
                                        // before any bytes have been written.
                                        if e.code == "response_too_large" {
                                            let _ = write_frame(
                                                &mut stream,
                                                &Response::failure(response.request_id, e),
                                            );
                                        }
                                    }
                                    active.fetch_sub(1, Ordering::AcqRel);
                                });
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => break,
                        }
                    }
                })
                .map_err(io_error)?;
            Ok(Self {
                path,
                stop,
                thread: Some(thread),
                _lock: lock,
            })
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_origin_is_not_a_wire_claim() {
        let mut request = Request::new("status", serde_json::json!({}));
        request.from_ui = true;
        let encoded = serde_json::to_value(&request).unwrap();
        assert!(encoded.get("from_ui").is_none());
        assert!(!serde_json::from_value::<Request>(encoded).unwrap().from_ui);
        assert!(serde_json::from_value::<Request>(
            serde_json::json!({"command":"status","from_ui":true})
        )
        .is_err());
    }
    #[test]
    fn incremental_queue_methods_are_discoverable_and_require_transaction_identity() {
        for command in ["batch.append", "batch.invalidate", "batch.seal"] {
            assert!(catalog()
                .iter()
                .any(|descriptor| descriptor["command"] == command));
            let error = validate_arguments(&Request::new(command, serde_json::json!({"id":"job"})))
                .unwrap_err();
            assert_eq!(error.details["argument"], "patch_id");
        }
        let wait = Request::new(
            "events.get",
            serde_json::json!({"since":42,"wait_ms":60000,"job_id":"job"}),
        );
        validate_arguments(&wait).unwrap();
        assert!(validate_arguments(&Request::new(
            "batch.seal",
            serde_json::json!({"id":"job","patch_id":"p","expected_revision":1,"execute":"shell"})
        ))
        .is_err());
    }
    #[test]
    fn actionable_errors_report_exact_argument_schema_example_and_suggestions() {
        let error = validate_arguments(&Request::new("deck.laod", serde_json::json!({"deck":"A"})))
            .unwrap_err();
        assert!(error.details["suggestions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("deck.load")));
        let error = validate_arguments(&Request::new("deck.load", serde_json::json!({"dekk":"A"})))
            .unwrap_err();
        assert_eq!(error.details["argument"], "deck");
        assert!(error.details["example"]
            .as_str()
            .unwrap()
            .contains("deck.load"));
        let error = validate_arguments(&Request::new(
            "library.tracks",
            serde_json::json!({"sort":"tempo"}),
        ))
        .unwrap_err();
        assert_eq!(
            error.details["expected"]["enum"],
            serde_json::json!(["title", "bpm", "duration"])
        );
        for descriptor in catalog() {
            let r = Request::new(
                descriptor["command"].as_str().unwrap(),
                descriptor["example_args"].clone(),
            );
            validate_arguments(&r).unwrap_or_else(|e| panic!("{}: {e}", r.command));
        }
    }
    #[test]
    fn request_validation_and_error_contract() {
        let mut r = Request::new("status", object());
        assert!(r.validate().is_ok());
        r.protocol_version = 8;
        assert_eq!(r.validate().unwrap_err().code, "protocol_mismatch");
        r.protocol_version = VERSION;
        r.args = Value::Null;
        assert!(r.validate().is_err());
        assert_eq!(ControlError::new("timeout", "pending").exit_code(), 6);
    }
    #[cfg(unix)]
    #[test]
    fn live_socket_roundtrip_exclusive_owner_and_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "pd-ipc-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("test.sock");
        let server = ipc::Server::start(path.clone(), |r| {
            let result = match r.command.as_str() {
                "large" => serde_json::json!({"payload":"x".repeat(1_000_000)}),
                "oversize" => serde_json::json!({"payload":"x".repeat(MAX_FRAME)}),
                _ => r.args.clone(),
            };
            Response::success(&r, "applied", result)
        })
        .unwrap();
        let request = Request::new("test", serde_json::json!({"deck":"A"}));
        let response = ipc::call(&path, &request).unwrap();
        assert_eq!(response.result, Some(request.args));
        let response = ipc::call(&path, &Request::new("large", serde_json::json!({}))).unwrap();
        assert_eq!(
            response.result.unwrap()["payload"].as_str().unwrap().len(),
            1_000_000
        );
        let response = ipc::call(&path, &Request::new("oversize", serde_json::json!({}))).unwrap();
        assert_eq!(response.error.unwrap().code, "response_too_large");
        assert!(ipc::Server::start(path.clone(), |r| Response::success(
            &r,
            "applied",
            Value::Null
        ))
        .is_err());
        drop(server);
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn refuses_public_directory_and_arbitrary_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("pd-unsafe-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(ipc::Server::start(root.join("s"), |r| Response::success(
            &r,
            "applied",
            Value::Null
        ))
        .is_err());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(root.join("s"), "keep").unwrap();
        assert!(ipc::Server::start(root.join("s"), |r| Response::success(
            &r,
            "applied",
            Value::Null
        ))
        .is_err());
        assert_eq!(std::fs::read_to_string(root.join("s")).unwrap(), "keep");
        std::fs::remove_dir_all(root).unwrap();
    }
}
