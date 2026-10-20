use control_protocol::{ControlError, Request};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
pub mod mix;
pub mod session;

pub const HELP: &str = "PulseDeck control CLI (pulsedeckctl)\n\nOpen PulseDeck, then use:\n  pulsedeckctl status --json\n  pulsedeckctl deck A load --path /music/song.flac\n  pulsedeckctl deck A play|pause|cue|seek --at 32s\n  pulsedeckctl deck B launch --clock A --clock-revision 1 --at 64s\n  pulsedeckctl deck A grid set --offset 0.125s\n  pulsedeckctl deck A hotcue A set --at 32s\n  pulsedeckctl deck A tempo set --bpm 124\n  pulsedeckctl deck A tempo set --percent -2\n  tempo set changes playback; deck A bpm set BPM corrects source analysis/grid.\n  Absolute --bpm uses current four-beat local BPM at apply; it is not a continuing BPM lock.\n  Verify target_playback_bpm and actual_playback_bpm; source bpm remains analysis/grid reference.\n  pulsedeckctl deck A eq set --low 0.7\n  pulsedeckctl mixer crossfader set -0.5\n  pulsedeckctl library tracks --search ambient\n  pulsedeckctl library suggest --deck A --energy keep\n  pulsedeckctl library analysis update --no-wait\n  pulsedeckctl playlist create --name Work\n  pulsedeckctl playlist add local:7 --track local:42\n  pulsedeckctl ui theme set graphite\n  pulsedeckctl commands list --json   # concise; --detail opts into all schemas\n  pulsedeckctl call mix.openers --args '{\"playlist\":\"spotify:liked\",\"provider\":\"spotify\"}' --json\n  pulsedeckctl call METHOD --args '{\"key\":\"value\"}'\n  pulsedeckctl command --file request.json\n  pulsedeckctl batch validate|run --file plan.json\n  pulsedeckctl batch append|invalidate|seal --id JOB --patch-id ID --expected-revision N ...\n  Native queues capture omitted guards; open:true schedules accept later actions.\n  pulsedeckctl mix execute --option-id OPTION --review-confirmed --follow --json\n  pulsedeckctl mix queue start --playlist spotify:liked --provider spotify --duration-seconds 3600 --review-confirmed --replace-prepared\n  pulsedeckctl mix queue start --sources '[\"/music/next.flac\",\"/music/last.flac\"]' --review-confirmed\n  pulsedeckctl mix queue status|stop --queue-id QUEUE\n  Queue start returns immediately: the app owns selection/preparation, timing and duration.\n  --wait-ready waits only until the first successor is armed or startup fails.\n  Finish after readiness, not acceptance. Healthy queues need no polling or model wakeups.\n  mix.queue.notice reports actionable outcomes to event subscribers and the UI.\n  One action owns preparation, short fallback, native timing and lifecycle feedback.\n  --follow streams events until terminal; disconnect does not cancel the move.\n  pulsedeckctl events wait --since 0 --operation-id MIX --timeout 60s\n  pulsedeckctl status --view resume --fields /engine,/jobs\n  pulsedeckctl commands list --prefix batch. --names-only\n  pulsedeckctl state watch [--count 10] [--interval 250ms]\n  pulsedeckctl session start --stdio\n  Queue load: {\"id\":\"load-b\",\"command\":\"deck.load\",\"args\":{\"deck\":\"B\",\"uri\":\"spotify:track:TRACK_ID\"}}\n  Queue input: {\"id\":\"play-b\",\"command\":\"engine.command\",\"args\":{\"type\":\"play\",\"deck\":\"B\"},\"when\":{\"after\":\"load-b\"}}\n  Atomic add: {\"op\":\"add\",\"items\":[{\"id\":\"one\",\"command\":\"state.get\",\"args\":{}},{\"id\":\"two\",\"command\":\"state.get\",\"args\":{},\"when\":{\"after\":\"one\"}}],\"request_id\":\"plan-once\"}\n  Atomic registration accepts all items or zero; each keeps its own timing.\n  Rejected ID reuse blocks ambiguous dependencies; select known when.after_version explicitly.\n  Edit input: {\"op\":\"update\",\"id\":\"play-b\",\"command\":\"engine.command\",\"args\":{\"type\":\"play\",\"deck\":\"B\"}}\n  Timing: omitted=now; {\"after\":\"load-b\"} waits for completion.\n  Relative when: {\"type\":\"after_beats\",\"clock\":\"A\",\"beats\":16,\"quantize\":4}\n                 {\"type\":\"after_seconds\",\"seconds\":2}\n  Absolute when: {\"type\":\"at_seconds\",\"clock\":\"A\",\"seconds\":160} (source audio time).\n                 {\"type\":\"at_event\",\"clock\":\"A\",\"event\":64} (clocks A/B).\n  Commands are independent unless linked with when.after.\n  Parallel input: {\"id\":\"blend\",\"commands\":[{\"command\":\"automation.ramp\",\"args\":{\"target\":{\"type\":\"channel_fader\",\"deck\":\"B\"},\"value\":1,\"seconds\":4}},{\"command\":\"automation.ramp\",\"args\":{\"target\":{\"type\":\"eq_high\",\"deck\":\"B\"},\"value\":1,\"seconds\":2}}],\"when\":{\"after\":\"play-b\"}}\n  Groups contain 2..64 engine.command/automation.ramp members with disjoint targets.\n  One shared trigger starts all members on the same audio frame; durations can differ.\n  Put quantization on shared when; per-ramp quantize is rejected in groups.\n  A group executes only when all members finish; when.after a group waits for all.\n  Tempo ramp target: {\"type\":\"tempo_percent\",\"deck\":\"B\"}; value is absolute source-relative percent.\n  Tempo ramps take manual tempo ownership and disable follower Sync at start.\n  Remove: {\"op\":\"remove\",\"id\":\"play-b\"}; inspect: {\"op\":\"status\"}; stop: {\"op\":\"stop\"}\n  Read immediately: {\"op\":\"call\",\"command\":\"state.get\",\"args\":{},\"view\":\"compact\"}\n  stdout streams registrations and job.action completion/failure events.\n  Keep stdin open; EOF/stop cancels this queue's pending items and leaves playback running.\n  Include request_id to retry an edit without executing it twice.\n  Update replaces a pending item/group; repeat command/args or commands and new timing.\n  Legacy direct requests: pulsedeckctl session --stdio\n\nGlobal: --json --socket PATH --timeout 30s --no-wait\n        --expected-load-revision N --request-id ID\nRead-only: --fields /engine/decks,/metadata/tracks,... --view handover|resume|compact\nNative slots are zero-based in JSON; named hot cues A-H / samples 1-16.\nUse commands list/describe for the complete callable catalog.\n";
#[derive(Debug)]
pub struct Invocation {
    pub request: Request,
    pub socket: PathBuf,
    pub json: bool,
    pub watch: bool,
    pub follow: bool,
    pub wait_ready: bool,
    pub count: Option<usize>,
    pub interval_ms: u64,
}
fn invalid(message: impl ToString) -> ControlError {
    ControlError::new("invalid_arguments", message.to_string())
}
pub fn number(s: &str) -> Result<f64, ControlError> {
    let raw = ["beats", "ms", "s", "%"]
        .iter()
        .find_map(|suffix| s.strip_suffix(suffix))
        .unwrap_or(s);
    let value = raw
        .parse::<f64>()
        .map_err(|_| invalid(format!("Invalid number: {s}")))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid("Numbers must be finite"))
    }
}
fn time_ms(s: &str) -> Result<u64, ControlError> {
    let n = number(s)? * if s.ends_with("ms") { 1.0 } else { 1000.0 };
    if !(1.0..=180_000.0).contains(&n) {
        return Err(invalid("Duration must be 1ms..180s"));
    }
    Ok(n as u64)
}
fn value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
}
fn seconds(s: &str) -> Result<f64, ControlError> {
    Ok(number(s)? / if s.ends_with("ms") { 1000.0 } else { 1.0 })
}
fn required<'a>(flags: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ControlError> {
    flags
        .get(key)
        .ok_or_else(|| invalid(format!("Missing --{}", key.replace('_', "-"))))
}
fn text(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
fn scalar(flags: &Map<String, Value>, key: &str) -> Result<f64, ControlError> {
    number(&text(required(flags, key)?))
}
fn index(s: &str) -> Result<usize, ControlError> {
    if s.len() == 1 && ('A'..='H').contains(&s.to_ascii_uppercase().chars().next().unwrap()) {
        Ok(s.to_ascii_uppercase().as_bytes()[0] as usize - b'A' as usize)
    } else {
        let n = s
            .parse::<usize>()
            .map_err(|_| invalid("Hot cue must be A-H"))?;
        if (1..=8).contains(&n) {
            Ok(n - 1)
        } else {
            Err(invalid("Hot cue must be A-H or 1-8"))
        }
    }
}
fn deck_example(tokens: &[String]) -> &'static str {
    match tokens.get(2).map(String::as_str) {
        Some("tempo") => "pulsedeckctl deck A tempo set --percent -2 --json",
        Some("pitch") => "pulsedeckctl deck A pitch set --semitones 1 --json",
        Some("eq") => "pulsedeckctl deck A eq set --low 0.7 --json",
        Some("load") => "pulsedeckctl deck A load --path /music/song.wav --json",
        Some("seek") => "pulsedeckctl deck A seek --at 32s --json",
        Some("play" | "pause" | "toggle-play") => "pulsedeckctl deck A play --json",
        Some("cue") => "pulsedeckctl deck A cue set --at 32s --json",
        Some("loop") => "pulsedeckctl deck A loop set --beats 4 --json",
        Some("grid") => "pulsedeckctl deck A grid set --offset 0.1s --json",
        _ => "pulsedeckctl deck A hotcue A set --at 32s --json",
    }
}

#[derive(Debug)]
pub struct SessionInvocation {
    pub socket: PathBuf,
    pub start: bool,
}
/// Discover the command through the same option tokenizer used by ordinary calls.
pub fn is_session(tokens: &[String]) -> bool {
    split_tokens(tokens).is_ok_and(|(words, _)| words.first().is_some_and(|word| word == "session"))
}
pub fn parse_session(tokens: &[String]) -> Result<SessionInvocation, ControlError> {
    let (words, mut flags) = split_tokens(tokens)?;
    let start = match words
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["session"] => false,
        ["session", "start"] => true,
        _ => return Err(invalid("Use session [start] --stdio [--socket PATH]")),
    };
    if let Some(key) = flags
        .keys()
        .find(|key| !["stdio", "socket", "json"].contains(&key.as_str()))
    {
        return Err(invalid(format!(
            "Unknown or inapplicable session option --{}",
            key.replace('_', "-")
        ))
        .with_detail(
            "valid_options",
            json!(["--stdio", "--socket PATH", "--json"]),
        ));
    }
    if flags.remove("stdio") != Some(json!(true)) {
        return Err(invalid("Session mode requires --stdio").with_detail(
            "example",
            "pulsedeckctl session start --stdio --socket /tmp/control.sock",
        ));
    }
    let socket = flags
        .remove("socket")
        .map(|value| PathBuf::from(text(&value)))
        .unwrap_or_else(control_protocol::socket_path);
    Ok(SessionInvocation { socket, start })
}

pub fn parse(tokens: &[String]) -> Result<Invocation, ControlError> {
    parse_inner(tokens).map_err(|mut error| {
        let topic=tokens.first().map(String::as_str).unwrap_or("");
        let (usage,example)=match topic {
            "deck"=>("deck A|B ACTION [arguments]", deck_example(tokens)),
            "mixer"=>("mixer CONTROL set VALUE | ramp --to VALUE --over DURATION", "pulsedeckctl mixer crossfader set -0.5 --json"),
            "sampler"=>("sampler slot 1..16 load|trigger|stop", "pulsedeckctl sampler slot 9 load --path /music/kick.wav --json"),
            "playlist"=>("playlist create --name NAME | add ID --track ID", "pulsedeckctl playlist add local:7 --track local:42 --json"),
            "ui"=>("ui TARGET ACTION VALUE (inspect ui state for mounted targets)", "pulsedeckctl ui app zoom 64 --json"),
            "call"=>("call METHOD --args JSON_OBJECT", "pulsedeckctl call engine.command --args '{\"type\":\"play\",\"deck\":\"A\"}' --json"),
            _=>("pulsedeckctl COMMAND [arguments] --json", "pulsedeckctl commands list --json"),
        };
        error=error.with_detail("usage",usage).with_detail("command",topic);
        if error.details.get("example").is_none(){error=error.with_detail("example",example);}
        if error.message.contains("Unknown command") {error=error.with_detail("suggestions",json!(control_protocol::suggestions(topic,["status","state","deck","mixer","sampler","library","playlist","analysis","track","audio","midi","recording","spotify","ui","commands","call","command","batch","jobs","automation"].map(str::to_string))));}
        error
    })
}
fn split_tokens(tokens: &[String]) -> Result<(Vec<String>, Map<String, Value>), ControlError> {
    let mut words = Vec::new();
    let mut flags = Map::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if let Some(flag) = token.strip_prefix("--") {
            let (name, inline) = flag
                .split_once('=')
                .map_or((flag, None), |(k, v)| (k, Some(v)));
            let key = name.replace('-', "_");
            let bool_flag = matches!(
                name,
                "json"
                    | "stdio"
                    | "no-wait"
                    | "demo"
                    | "replace-playing"
                    | "overwrite"
                    | "waveform"
                    | "current"
                    | "scratch"
                    | "shift"
                    | "ramps"
                    | "names-only"
                    | "dialog"
                    | "allow-half-double"
                    | "detail"
                    | "follow"
                    | "wait-ready"
                    | "review-confirmed"
                    | "replace-prepared"
                    | "allow-analysis"
                    | "allow-fallback"
            );
            let val = if let Some(v) = inline {
                value(v)
            } else if bool_flag {
                json!(true)
            } else {
                i += 1;
                let next = tokens
                    .get(i)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| {
                        invalid(format!("Missing value for --{name}")).with_detail("argument", name)
                    })?;
                value(next)
            };
            if bool_flag && !val.is_boolean() {
                return Err(invalid(format!("--{name} requires true or false"))
                    .with_detail("argument", name)
                    .with_detail("expected", json!([true, false])));
            }
            if flags.insert(key, val).is_some() {
                return Err(invalid(format!("Duplicate --{name}")));
            }
        } else {
            words.push(token.clone());
        }
        i += 1;
    }
    Ok((words, flags))
}
fn parse_inner(tokens: &[String]) -> Result<Invocation, ControlError> {
    let (words, mut flags) = split_tokens(tokens)?;
    let word_refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let valid_flags = allowed_flags(&word_refs);
    if let Some(key) = flags
        .keys()
        .find(|key| !valid_flags.iter().any(|v| v == *key))
    {
        return Err(invalid(format!(
            "Unknown or inapplicable option --{} for this command",
            key.replace('_', "-")
        ))
        .with_detail("argument", format!("--{}", key.replace('_', "-")))
        .with_detail(
            "valid_options",
            json!(valid_flags
                .iter()
                .map(|v| format!("--{}", v.replace('_', "-")))
                .collect::<Vec<_>>()),
        )
        .with_detail(
            "suggestions",
            json!(control_protocol::suggestions(
                key,
                valid_flags.iter().map(|v| v.to_string())
            )),
        ));
    }
    let socket = flags
        .remove("socket")
        .map(|v| PathBuf::from(text(&v)))
        .unwrap_or_else(control_protocol::socket_path);
    let json_output = flags
        .remove("json")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let timeout = flags
        .remove("timeout")
        .map(|v| time_ms(&text(&v)))
        .transpose()?
        .unwrap_or(30_000);
    let no_wait = flags
        .remove("no_wait")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let revision = flags
        .remove("expected_load_revision")
        .map(|v| {
            v.as_u64()
                .ok_or_else(|| invalid("Revision must be an integer"))
        })
        .transpose()?;
    let request_id = flags.remove("request_id").map(|v| text(&v));
    let fields = flags
        .remove("fields")
        .map(|v| text(&v).split(',').map(str::to_owned).collect::<Vec<_>>());
    let view = flags.remove("view").map(|v| text(&v));
    let count = flags
        .remove("count")
        .map(|v| {
            v.as_u64()
                .map(|v| v as usize)
                .ok_or_else(|| invalid("Count must be an integer"))
        })
        .transpose()?;
    let interval = flags
        .remove("interval")
        .map(|v| time_ms(&text(&v)))
        .transpose()?
        .unwrap_or(250)
        .max(20);
    let follow = flags
        .remove("follow")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let wait_ready = flags
        .remove("wait_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut watch = false;
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut request = match w.as_slice() {
        ["call", method] => {
            let args = if let Some(v) = flags.remove("args") {
                if !flags.is_empty() {
                    return Err(invalid("Use either --args JSON or named argument options; combining them is ambiguous"));
                }
                if let Some(raw) = v.as_str() {
                    serde_json::from_str(raw).map_err(|e| {
                        invalid(format!("Invalid --args JSON: {e}"))
                            .with_detail("argument", "--args")
                            .with_detail("expected", "A JSON object")
                    })?
                } else {
                    v
                }
            } else {
                json!(flags)
            };
            Request::new(*method, args)
        }
        ["command"] => {
            let file = text(required(&flags, "file")?);
            serde_json::from_str::<Request>(&std::fs::read_to_string(file).map_err(invalid)?)
                .map_err(invalid)?
        }
        ["mix", "execute"] => Request::new("mix.execute", json!(flags)),
        ["mix", "queue", action @ ("start" | "status" | "edit" | "stop")] => {
            Request::new(format!("mix.queue.{action}"), json!(flags))
        }
        ["status"] => Request::new("status", json!({})),
        ["state", "get"] | ["state"] => Request::new("state.get", json!(flags)),
        ["events", "wait"] => {
            flags
                .entry("wait_ms")
                .or_insert(json!(timeout.saturating_sub(100).min(60000)));
            Request::new("events.get", json!(flags))
        }
        ["state", "watch"] | ["events", "watch"] => {
            watch = true;
            Request::new(
                if w[0] == "events" {
                    "events.get"
                } else {
                    "state.get"
                },
                json!(flags),
            )
        }
        ["commands", "list"] | ["capabilities"] => Request::new("commands.list", json!(flags)),
        ["commands", "describe", name] => {
            flags.insert("name".into(), json!(name));
            Request::new("commands.describe", json!(flags))
        }
        ["batch", action]
            if matches!(
                *action,
                "run" | "validate" | "append" | "invalidate" | "seal"
            ) =>
        {
            let plan: Value = if let Some(file) = flags.remove("file") {
                if !flags.is_empty() {
                    return Err(invalid("Use --file or inline batch arguments, not both"));
                }
                serde_json::from_str(&std::fs::read_to_string(text(&file)).map_err(invalid)?)
                    .map_err(invalid)?
            } else {
                json!(flags)
            };
            Request::new(
                format!("batch.{action}"),
                if *action == "validate" && plan.get("items").is_some() {
                    session::validation_args(&plan)?
                } else if plan.is_array() {
                    json!({"steps":plan})
                } else {
                    plan
                },
            )
        }
        ["jobs", action] if *action == "list" => Request::new("jobs.list", json!(flags)),
        ["jobs", action, id] if matches!(*action, "get" | "cancel" | "wait") => {
            watch = *action == "wait";
            Request::new(
                if watch {
                    "jobs.get".into()
                } else {
                    format!("jobs.{action}")
                },
                {
                    let mut args = json!({"id":id});
                    for key in ["mode", "detail"] {
                        if let Some(value) = flags.remove(key) {
                            args[key] = value;
                        }
                    }
                    args
                },
            )
        }
        ["automation", "stop"] => Request::new("automation.stop", json!({})),
        ["app", action] if matches!(*action, "show" | "hide" | "stop") => {
            Request::new(format!("app.{action}"), json!({}))
        }
        ["deck", d, rest @ ..] => deck_request(d, rest, flags)?,
        ["mixer", name, rest @ ..] => mixer_request(name, rest, flags)?,
        ["sampler", "slot", slot, action] => {
            let n: usize = slot.parse().map_err(|_| invalid("Slot must be 1..16"))?;
            if !(1..=16).contains(&n) {
                return Err(invalid("Slot must be 1..16"));
            }
            if *action == "load" {
                flags.insert("slot".into(), json!(n - 1));
                Request::new("sampler.load", json!(flags))
            } else {
                Request::new(
                    "engine.command",
                    json!({"type":match *action{"trigger"=>"trigger_sample","stop"=>"stop_sample",_=>return Err(invalid("Sampler action load, trigger or stop"))},"slot":n-1}),
                )
            }
        }
        ["sampler", "gain", "set", gain] => Request::new(
            "engine.command",
            json!({"type":"set_sampler_gain","gain":number(gain)?}),
        ),
        ["sampler", "list"] | ["sampler", "state"] => Request::new("sampler.state", json!({})),
        ["library", "analysis", action] if matches!(*action, "update" | "cancel") => {
            Request::new(format!("library.analysis.{action}"), json!({}))
        }
        ["library", "suggest"] => {
            if let Some(v) = flags.remove("energy") {
                let direction:control_protocol::EnergyDirection=serde_json::from_value(v).map_err(|_|invalid("Energy direction must be keep, raise or lower (up/down aliases accepted)"))?;
                flags.insert("energy_direction".into(), json!(direction.as_str()));
            }
            if let Some(v) = flags.get_mut("deck") {
                *v = json!(text(v).to_ascii_uppercase());
            }
            Request::new("library.suggest_next", json!(flags))
        }
        ["library", "history", "clear"] => Request::new("library.history.clear", json!({})),
        ["library", "import"] => {
            if flags.get("dialog").and_then(Value::as_bool) == Some(true) {
                if flags.contains_key("path") || flags.contains_key("paths") {
                    return Err(invalid("Use dialog:true or path/paths exclusively"));
                }
                Request::new("library.import", json!(flags))
            } else {
                let paths = if let Some(p) = flags.remove("paths") {
                    p
                } else {
                    json!([text(required(&flags, "path")?)])
                };
                Request::new("library.import", json!({"paths":paths}))
            }
        }
        ["library", "tracks"] | ["library", "tracks", "list"] => {
            Request::new("library.tracks", json!(flags))
        }
        ["library", "track", "tags", id] => Request::new(
            "library.set_tags",
            json!({"id":id,"tags":required(&flags,"tags")?}),
        ),
        ["library", "track", action, id] if matches!(*action, "get" | "remove") => Request::new(
            if *action == "get" {
                "library.track"
            } else {
                "library.remove"
            },
            json!({"id":id}),
        ),
        ["library", action] if matches!(*action, "import-rekordbox" | "export-rekordbox") => {
            Request::new(
                if *action == "import-rekordbox" {
                    "library.import_xml"
                } else {
                    "library.export_xml"
                },
                json!(flags),
            )
        }
        ["playlist", "create"] => Request::new("playlist.create", json!(flags)),
        ["playlist", "list"] => Request::new("playlist.list", json!({})),
        ["playlist", "add", id] => {
            flags.insert("playlist".into(), json!(id));
            Request::new("playlist.add", json!(flags))
        }
        ["playlist", action, id] => {
            flags.insert("playlist".into(), json!(id));
            flags.insert(
                "action".into(),
                json!(if *action == "reorder" {
                    "replace"
                } else {
                    action
                }),
            );
            Request::new("playlist.edit", json!(flags))
        }
        ["audio", "devices"] | ["audio", "devices", "list"] => {
            Request::new("audio.devices", json!({}))
        }
        ["audio", action] if matches!(*action, "start" | "stop") => {
            if let Some(v) = flags.remove("device") {
                flags.insert("device_name".into(), v);
            }
            if let Some(v) = flags.remove("buffer") {
                flags.insert("buffer_frames".into(), v);
            }
            Request::new(format!("audio.{action}"), json!(flags))
        }
        ["midi", action] => {
            if let Some(v) = flags.remove("input") {
                flags.insert("input_name".into(), v);
            }
            if let Some(v) = flags.remove("output") {
                flags.insert("output_name".into(), v);
            }
            Request::new(format!("midi.{action}"), json!(flags))
        }
        ["recording", action] => Request::new(format!("recording.{action}"), json!(flags)),
        ["spotify", rest @ ..] if !rest.is_empty() => {
            Request::new(format!("spotify.{}", rest.join(".")), json!(flags))
        }
        ["ui", "state", "get"] | ["ui", "state"] => Request::new("ui.state", json!({})),
        ["ui", target, action, rest @ ..] => {
            let (target, action, value) = if *target == "theme" {
                if *action != "set" || rest.len() != 1 {
                    return Err(invalid(
                        "Use ui theme set THEME; valid themes: aluminium, ivory, graphite",
                    ));
                }
                (
                    "app".to_string(),
                    "theme".to_string(),
                    json!(rest.first().ok_or_else(|| invalid("ui theme set THEME"))?),
                )
            } else {
                if rest.len() > 1 {
                    return Err(invalid("UI commands take one VALUE (a JSON object for multiple fields); use ui TARGET ACTION VALUE"));
                }
                (
                    target.to_string(),
                    action.to_string(),
                    rest.first()
                        .map(|v| value(v))
                        .or_else(|| flags.remove("value"))
                        .unwrap_or_else(|| json!(flags)),
                )
            };
            Request::new(
                "ui.call",
                json!({"target":target,"action":action,"value":value}),
            )
        }
        _ => {
            return Err(invalid(
                "Unknown command; use --help or call METHOD --args JSON",
            ))
        }
    };
    if tokens
        .iter()
        .any(|t| t == "--timeout" || t.starts_with("--timeout="))
    {
        request.timeout_ms = timeout;
    }
    if tokens
        .iter()
        .any(|t| t == "--no-wait" || t.starts_with("--no-wait="))
    {
        request.no_wait = no_wait;
    }
    if revision.is_some() {
        request.expected_load_revision = revision;
    }
    if let Some(id) = request_id {
        request.request_id = id;
    }
    request.fields = fields.or(request.fields);
    request.view = view.or(request.view);
    request.validate()?;
    control_protocol::validate_arguments(&request)?;
    if matches!(
        request.command.as_str(),
        "deck.load"
            | "sampler.load"
            | "library.import_xml"
            | "library.export_xml"
            | "recording.start"
    ) {
        if let Some(path) = request.args.get_mut("path") {
            let input = PathBuf::from(text(path));
            if !input.is_absolute() {
                *path = json!(std::env::current_dir()
                    .map_err(invalid)?
                    .join(input)
                    .to_string_lossy());
            }
        }
    }
    if request.command == "library.import" {
        if let Some(paths) = request.args.get_mut("paths").and_then(Value::as_array_mut) {
            for path in paths {
                let input = PathBuf::from(text(path));
                if !input.is_absolute() {
                    *path = json!(std::env::current_dir()
                        .map_err(invalid)?
                        .join(input)
                        .to_string_lossy());
                }
            }
        }
        if let Some(folder) = request.args.get_mut("folder") {
            let input = PathBuf::from(text(folder));
            if !input.is_absolute() {
                *folder = json!(std::env::current_dir()
                    .map_err(invalid)?
                    .join(input)
                    .to_string_lossy());
            }
        }
    }
    if follow && !matches!(request.command.as_str(), "mix.execute" | "mix.queue.start") {
        return Err(invalid(
            "--follow is supported for mix.execute and mix.queue.start",
        ));
    }
    if wait_ready && (request.command != "mix.queue.start" || follow) {
        return Err(invalid(
            "--wait-ready is only supported for mix queue start, without --follow",
        ));
    }
    Ok(Invocation {
        request,
        socket,
        json: json_output,
        watch,
        follow,
        wait_ready,
        count,
        interval_ms: interval,
    })
}
fn allowed_flags(words: &[&str]) -> Vec<String> {
    let mut flags: Vec<String> = [
        "json",
        "socket",
        "timeout",
        "no_wait",
        "expected_load_revision",
        "request_id",
        "fields",
        "view",
        "follow",
        "wait_ready",
    ]
    .map(str::to_string)
    .to_vec();
    let extra: &[&str] = match words {
        ["mix", "queue", action @ ("start" | "status" | "edit" | "stop")] => {
            if let Some(desc) = control_protocol::catalog()
                .iter()
                .find(|d| d["command"] == format!("mix.queue.{action}"))
            {
                flags.extend(
                    desc["input_schema"]["properties"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .cloned(),
                );
            }
            return flags;
        }
        ["call", method] | ["mix", method @ "execute"] => {
            let method = if words[0] == "mix" {
                "mix.execute"
            } else {
                *method
            };
            let mut extra = vec!["args".to_string()];
            if let Some(desc) = control_protocol::catalog()
                .iter()
                .find(|d| d["command"] == method)
            {
                extra.extend(
                    desc["input_schema"]["properties"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .cloned(),
                );
            }
            flags.extend(extra);
            return flags;
        }
        ["command"] => &["file"],
        ["batch", "run" | "validate"] => &["file", "steps", "schedule"],
        ["batch", "append"] => &[
            "file",
            "id",
            "patch_id",
            "expected_revision",
            "steps",
            "invalidate_action_ids",
        ],
        ["batch", "invalidate"] => &["file", "id", "patch_id", "expected_revision", "action_ids"],
        ["batch", "seal"] => &[
            "file",
            "id",
            "patch_id",
            "expected_revision",
            "completion_event",
        ],
        ["jobs", "get", _] | ["jobs", "list"] => &["detail"],
        ["events", "wait"] => &[
            "since",
            "wait_ms",
            "job_id",
            "operation_id",
            "queue_id",
            "types",
        ],
        ["commands", "list"] | ["capabilities"] => &["prefix", "names_only", "detail", "search"],
        ["commands", "describe", _] => &["action"],
        ["jobs", "cancel", _] => &["mode"],
        ["state", "watch"] => &["count", "interval"],
        ["events", "watch"] => &["count", "interval", "since"],
        ["jobs", "wait", _] => &["count", "interval"],
        ["deck", _, "load"] => &["path", "track", "uri", "demo", "replace_playing", "detail"],
        ["deck", _, "cue", "set"] | ["deck", _, "hotcue", _, "set"] => &["at", "current"],
        ["deck", _, "seek"] => &["at", "snap"],
        ["deck", _, "launch"] => &["clock", "clock_revision", "at", "quantize"],
        ["deck", _, "jump"] | ["deck", _, "loop", "set"] => &["beats"],
        ["deck", _, "tempo", "set"] => &["percent", "bpm"],
        ["deck", _, "pitch", "set"] => &["semitones"],
        ["deck", _, "grid", "set"] => &["offset"],
        ["deck", _, "eq", "set"] => &["low", "mid", "high"],
        ["deck", _, "fx", "set"] => &["echo", "reverb"],
        ["deck", _, "jog"] => &["delta", "scratch"],
        ["deck", _, "scrub", "to"] => &["at", "duration"],
        ["deck", _, "scrub", "end"] => &["at"],
        ["deck", _, "wait"] => &["ramps", "launches"],
        ["mixer", _, "ramp"] => &["to", "over", "clock", "quantize"],
        ["mixer", _, "set"] => &["position", "gain", "mix"],
        ["sampler", "slot", _, "load"] => &["path", "dialog"],
        ["library", "suggest"] => &[
            "deck",
            "energy",
            "sources",
            "provider",
            "playlist",
            "max_stretch_percent",
            "allow_half_double",
            "limit",
        ],
        ["library", "import"] => &["path", "paths", "dialog"],
        ["library", "track", "tags", _] => &["tags"],
        ["library", "tracks"] | ["library", "tracks", "list"] => &[
            "search", "sort", "playlist", "offset", "limit", "waveform", "bpm_min", "bpm_max",
            "filters",
        ],
        ["library", "import-rekordbox"] => &["path", "dialog"],
        ["library", "export-rekordbox"] => &["path", "overwrite", "dialog"],
        ["playlist", "create"] => &["name", "source"],
        ["playlist", "add", _] => &["track"],
        ["playlist", _, _] => &["tracks", "name"],
        ["audio", "start"] => &[
            "device",
            "device_name",
            "buffer",
            "buffer_frames",
            "routing",
        ],
        ["midi", "connect"] => &["input", "output", "input_name", "output_name"],
        ["recording", "start"] => &["path", "overwrite", "dialog"],
        ["spotify", rest @ ..] => {
            let method = format!("spotify.{}", rest.join("."));
            if let Some(desc) = control_protocol::catalog()
                .iter()
                .find(|d| d["command"] == method)
            {
                flags.extend(
                    desc["input_schema"]["properties"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .cloned(),
                );
            }
            return flags;
        }
        ["ui", _, _, ..] => &["value"],
        _ => &[],
    };
    flags.extend(extra.iter().map(|v| v.to_string()));
    flags
}
fn deck_request(
    d: &str,
    words: &[&str],
    mut flags: Map<String, Value>,
) -> Result<Request, ControlError> {
    if !matches!(d.to_ascii_uppercase().as_str(), "A" | "B") {
        return Err(invalid("Deck must be A or B"));
    }
    let deck = d.to_ascii_uppercase();
    flags.insert("deck".into(), json!(deck));
    let mut c = json!({"deck":deck});
    let kind = match words {
        ["load"] => return Ok(Request::new("deck.load", json!(flags))),
        ["load", "cancel"] => return Ok(Request::new("deck.load.cancel", json!(flags))),
        ["unload"] => return Ok(Request::new("deck.unload", json!(flags))),
        ["instant-double"] => return Ok(Request::new("deck.instant_double", json!(flags))),
        ["analyze"] => return Ok(Request::new("deck.analyze", json!(flags))),
        ["wait"] => return Ok(Request::new("engine.wait", json!(flags))),
        ["play"] => "play",
        ["launch"] => {
            c["clock"] = required(&flags, "clock")?.clone();
            c["clock_revision"] =
                json!(text(required(&flags, "clock_revision")?)
                    .parse::<u64>()
                    .map_err(|_| invalid("Clock revision must be a non-negative integer"))?);
            c["quantize"] = json!(flags
                .get("quantize")
                .map(|v| number(&text(v)))
                .transpose()?
                .unwrap_or(1.0));
            c["at_seconds"] = flags
                .get("at")
                .map(|v| seconds(&text(v)))
                .transpose()?
                .map_or(Value::Null, |s| json!(s));
            "play_quantized"
        }
        ["launch", "cancel"] => "cancel_launch",
        ["pause"] => "pause",
        ["toggle-play"] => "toggle_play",
        ["master"] => "set_master",
        ["cue"] | ["cue", "return"] => "cue",
        ["cue", "set"] => {
            if let Some(v) = flags.get("at") {
                c["seconds"] = json!(seconds(&text(v))?);
                "set_cue_point"
            } else {
                "set_cue"
            }
        }
        ["cue", "press"] | ["cue", "release"] => {
            c["pressed"] = json!(words[1] == "press");
            "cue_press"
        }
        ["seek"] => {
            c["seconds"] = json!(seconds(&text(required(&flags, "at")?))?);
            "seek"
        }
        ["jump"] => {
            c["beats"] = json!(scalar(&flags, "beats")?);
            "beat_jump"
        }
        ["hotcue", slot, action] => {
            c["index"] = json!(index(slot)?);
            match *action {
                "set" => {
                    if let Some(v) = flags.get("at") {
                        c["seconds"] = json!(seconds(&text(v))?);
                        "set_hot_cue_point"
                    } else {
                        "set_hot_cue"
                    }
                }
                "trigger" => "trigger_hot_cue",
                "clear" => "clear_hot_cue",
                _ => return Err(invalid("Hotcue action set, trigger or clear")),
            }
        }
        ["memory", action] => match *action {
            "save" => "memory_cue_save",
            "delete" => "memory_cue_delete",
            "next" => "memory_cue_next",
            "previous" => "memory_cue_previous",
            _ => return Err(invalid("Memory action save/delete/next/previous")),
        },
        ["loop", "set"] => {
            c["beats"] = json!(scalar(&flags, "beats")?);
            "set_loop"
        }
        ["loop", "off"] => {
            c["beats"] = Value::Null;
            "set_loop"
        }
        ["loop", "toggle"] | ["loop", "reloop"] => "toggle_active_loop",
        ["loop", "in"] => "loop_in",
        ["loop", "out"] => "loop_out",
        ["loop", "edit", boundary] => {
            c["boundary"] = if *boundary == "off" {
                Value::Null
            } else {
                json!(boundary)
            };
            "set_loop_edit"
        }
        ["tempo", "set"] => match (flags.get("bpm"), flags.get("percent")) {
            (Some(_), None) => {
                let bpm = text(required(&flags, "bpm")?)
                    .parse::<f64>()
                    .map_err(|_| invalid("--bpm requires a numeric playback BPM"))?;
                if !(30.0..=300.0).contains(&bpm) {
                    return Err(invalid("--bpm must be 30..300 playback BPM")
                        .with_detail("argument", "--bpm"));
                }
                c["bpm"] = json!(bpm);
                "set_playback_bpm"
            }
            (None, Some(_)) => {
                c["percent"] = json!(scalar(&flags, "percent")?);
                "set_tempo"
            }
            _ => {
                return Err(
                    invalid("Use exactly one of --bpm or --percent for tempo set")
                        .with_detail("valid_options", json!(["--bpm 124", "--percent -2"])),
                )
            }
        },
        ["tempo", "reset"] => {
            c["percent"] = json!(0);
            "set_tempo"
        }
        ["pitch", "set"] => {
            c["semitones"] = json!(scalar(&flags, "semitones")?);
            "set_pitch"
        }
        ["bpm", "set", bpm] => {
            c["bpm"] = json!(number(bpm)?);
            "set_bpm"
        }
        ["grid", "set"] => {
            c["offset_seconds"] = json!(seconds(&text(required(&flags, "offset")?))?);
            "set_beatgrid"
        }
        ["sync", "align"] => "sync",
        ["sync", on] => {
            c["enabled"] = json!(onoff(on)?);
            "set_sync"
        }
        ["keylock", on] => {
            c["enabled"] = json!(onoff(on)?);
            "set_keylock"
        }
        ["headphones", on] => {
            c["enabled"] = json!(onoff(on)?);
            "set_headphone_cue"
        }
        ["gain", "set", v] => {
            c["gain"] = json!(number(v)?);
            "set_deck_gain"
        }
        ["fader", "set", v] => {
            c["value"] = json!(number(v)?);
            "set_channel_fader"
        }
        ["filter", "set", v] => {
            c["amount"] = json!(number(v)?);
            "set_filter"
        }
        ["eq", "set"] => {
            let bands: Map<String, Value> = flags
                .iter()
                .filter(|(k, _)| matches!(k.as_str(), "low" | "mid" | "high"))
                .map(|(k, v)| number(&text(v)).map(|n| (k.clone(), json!(n))))
                .collect::<Result<_, _>>()?;
            return Ok(Request::new(
                "engine.command",
                json!({"command":{"type":"set_eq","deck":deck,"low":1,"mid":1,"high":1},"bands":bands}),
            ));
        }
        ["fx", "set"] => {
            c["echo"] = json!(flags
                .get("echo")
                .map(|v| number(&text(v)))
                .transpose()?
                .unwrap_or(0.0));
            c["reverb"] = json!(flags
                .get("reverb")
                .map(|v| number(&text(v)))
                .transpose()?
                .unwrap_or(0.0));
            "set_fx"
        }
        ["fx", "beats", "set", v] => {
            c["beats"] = json!(number(v)?);
            "set_fx_beat"
        }
        ["jog"] => {
            c["delta_seconds"] = json!(seconds(&text(required(&flags, "delta")?))?);
            c["scratch"] = json!(flags
                .get("scratch")
                .and_then(Value::as_bool)
                .unwrap_or(false));
            "jog"
        }
        ["jog", "touch", on] => {
            c["touched"] = json!(onoff(on)?);
            "jog_touch"
        }
        ["scrub", "to"] => {
            c["seconds"] = json!(seconds(&text(required(&flags, "at")?))?);
            c["duration_ms"] = json!(time_ms(&text(required(&flags, "duration")?))?);
            "scrub_to"
        }
        ["scrub", "end"] => {
            c["seconds"] = json!(seconds(&text(required(&flags, "at")?))?);
            "scrub_end"
        }
        _ => {
            return Err(invalid(
                "Unknown deck action; use call engine.command for catalog operations",
            ))
        }
    };
    c["type"] = json!(kind);
    if let Some(snap) = flags.get("snap") {
        return Ok(Request::new(
            "engine.command",
            json!({"command":c,"snap":snap}),
        ));
    }
    Ok(Request::new("engine.command", c))
}
fn onoff(v: &str) -> Result<bool, ControlError> {
    match v {
        "on" | "true" => Ok(true),
        "off" | "false" => Ok(false),
        _ => Err(invalid("Use on or off")),
    }
}
fn mixer_request(
    name: &str,
    rest: &[&str],
    flags: Map<String, Value>,
) -> Result<Request, ControlError> {
    if rest == ["ramp"] {
        let target = name.replace('-', "_");
        let duration = text(required(&flags, "over")?);
        let mut args = json!({"target":{"type":target},"value":scalar(&flags,"to")?,"seconds":if duration.ends_with("beats"){1.0}else{seconds(&duration)?}});
        if duration.ends_with("beats") {
            args["beats"] = json!(number(&duration)?);
        }
        if let Some(clock) = flags.get("clock") {
            args["clock"] = clock.clone();
        }
        if let Some(q) = flags.get("quantize") {
            args["quantize"] = json!(number(&text(q))?);
        }
        return Ok(Request::new("automation.ramp", args));
    }
    if matches!(name, "smart-fader" | "smart-cfx") {
        return Ok(Request::new(
            "mixer.smart_modes",
            json!({(if name=="smart-fader"{"fader"}else{"cfx"}):onoff(rest.first().ok_or_else(||invalid("Use on/off"))?)?}),
        ));
    }
    let parameter = match name {
        "crossfader" => "position",
        "cue-mix" => "mix",
        "master-gain" | "cue-gain" => "gain",
        _ => return Err(invalid("Unknown mixer control")),
    };
    let v = match rest {
        ["set", v] => number(v)?,
        ["reset"] => {
            if name == "master-gain" || name == "cue-gain" {
                1.0
            } else {
                0.0
            }
        }
        ["set"] => scalar(&flags, parameter)?,
        _ => return Err(invalid("Use set VALUE or reset")),
    };
    Ok(Request::new(
        "engine.command",
        json!({"type":format!("set_{}",name.replace('-',"_")),(parameter):v}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successor_queue_cli_keeps_order_and_revision_guards() {
        let tokens = [
            "mix",
            "queue",
            "edit",
            "--queue-id",
            "q",
            "--expected-revision",
            "7",
            "--sources",
            r#"["local:42","local:7"]"#,
        ]
        .map(str::to_owned);
        let request = super::parse(&tokens).unwrap().request;
        assert_eq!(request.command, "mix.queue.edit");
        assert_eq!(request.args["sources"], json!(["local:42", "local:7"]));
        assert_eq!(request.args["expected_revision"], 7);
        assert_eq!(p("mix queue status").request.command, "mix.queue.status");
        assert!(control_protocol::selection::is_read("mix.queue.status"));
    }
    #[test]
    fn live_discovery_and_full_load_detail_are_explicit() {
        let describe = p("commands describe engine.command --action set_sync");
        assert_eq!(
            describe.request.args,
            json!({"name":"engine.command","action":"set_sync"})
        );
        assert_eq!(
            p("deck B load --track local:53 --detail").request.args["detail"],
            true
        );
        assert!(p("deck B load --track local:53")
            .request
            .args
            .get("detail")
            .is_none());
    }
    #[test]
    fn library_search_passes_indexed_filters_and_requires_an_object() {
        for words in [vec!["library", "tracks"], vec!["library", "tracks", "list"]] {
            let mut tokens: Vec<String> = words.into_iter().map(str::to_owned).collect();
            tokens.extend(
                [
                    "--search",
                    "house summer",
                    "--bpm-min",
                    "120",
                    "--bpm-max",
                    "128",
                    "--limit",
                    "20",
                    "--filters",
                    r##"{"my_tags":[1,2],"tag_mode":"and","rating_min":3,"colors":["#FF0000"]}"##,
                ]
                .map(str::to_owned),
            );
            let request = parse(&tokens).unwrap().request;
            assert_eq!(request.command, "library.tracks");
            assert_eq!(
                request.args,
                json!({"search":"house summer","bpm_min":120,
                "bpm_max":128,"limit":20,"filters":{"my_tags":[1,2],"tag_mode":"and",
                "rating_min":3,"colors":["#FF0000"]}})
            );
            for invalid in ["[]", "3", "true"] {
                *tokens.last_mut().unwrap() = invalid.to_owned();
                assert!(
                    parse(&tokens).is_err(),
                    "accepted invalid filters {invalid}"
                );
            }
        }
    }
    #[test]
    fn session_global_options_are_order_independent_and_do_not_become_commands() {
        let variants = [
            "--socket /tmp/pd.sock session start --stdio",
            "session start --stdio --socket /tmp/pd.sock",
            "--json --socket=/tmp/pd.sock session --stdio start",
            "--stdio session --socket /tmp/pd.sock start --json",
        ];
        for variant in variants {
            let tokens: Vec<String> = variant.split_whitespace().map(str::to_owned).collect();
            assert!(is_session(&tokens), "not recognized: {variant}");
            let session = parse_session(&tokens).unwrap();
            assert!(session.start);
            assert_eq!(session.socket, PathBuf::from("/tmp/pd.sock"));
        }
        let tokens = ["--socket", "session", "status"].map(str::to_owned);
        assert!(!is_session(&tokens));
        assert_eq!(parse(&tokens).unwrap().request.command, "status");
        let legacy = ["--socket", "/tmp/pd.sock", "session", "--stdio"].map(str::to_owned);
        assert!(!parse_session(&legacy).unwrap().start);
        for invalid in [
            "--socket /tmp/pd.sock session start --stdio --socket /tmp/other.sock",
            "session start --stdio --stdio",
            "session start --stdio=false",
            "session start --stdio --no-wait",
            "session start --stdio nonsense",
            "--socket session start --stdio",
        ] {
            let tokens: Vec<String> = invalid.split_whitespace().map(str::to_owned).collect();
            assert!(parse_session(&tokens).is_err(), "accepted: {invalid}");
        }
    }

    #[test]
    fn absolute_playback_bpm_is_distinct_from_source_grid_correction() {
        let args = |command: &str| {
            parse(
                &command
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
        };
        let invocation = args("deck A tempo set --bpm 124.25 --expected-load-revision 9").unwrap();
        assert_eq!(invocation.request.command, "engine.command");
        assert_eq!(
            invocation.request.args,
            json!({"type":"set_playback_bpm","deck":"A","bpm":124.25})
        );
        assert_eq!(invocation.request.expected_load_revision, Some(9));
        assert_eq!(
            args("deck A bpm set 124.25").unwrap().request.args["type"],
            "set_bpm"
        );
        assert_eq!(
            args("deck B tempo set --percent -2").unwrap().request.args,
            json!({"type":"set_tempo","deck":"B","percent":-2.0})
        );
        for command in [
            "deck A tempo set",
            "deck A tempo set --bpm 124 --percent 1",
            "deck A tempo set --bpm NaN",
            "deck A tempo set --bpm 0",
            "deck A tempo set --bpm 301",
            "deck A tempo set --bpm nope",
            "deck A tempo set --bpm 124s",
        ] {
            assert!(args(command).is_err(), "accepted {command}");
        }
    }

    #[test]
    fn invalid_commands_are_actionable_and_never_silently_ignore_flags() {
        for (command, argument) in [
            ("deck A tempo set --precent -2", "--precent"),
            ("status --nonsense 1", "--nonsense"),
        ] {
            let e = parse(
                &command
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )
            .unwrap_err();
            assert_eq!(e.details["argument"], argument);
            assert!(e.details["usage"].is_string());
            assert!(e.details["example"].is_string());
            if argument == "--precent" {
                assert!(e.details["example"]
                    .as_str()
                    .unwrap()
                    .contains("tempo set --percent"));
            }
        }
        let e = parse(
            &["call", "library.tracks", "--args", "{\"sort\":\"wrong\"}"].map(str::to_string),
        )
        .unwrap_err();
        assert_eq!(e.details["argument"], "sort");
        assert!(e.details["expected"]["enum"].is_array());
        let e = parse(
            &"dekk A play"
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
        .unwrap_err();
        assert!(e.details["suggestions"]
            .as_array()
            .unwrap()
            .contains(&json!("deck")));
        assert!(parse(
            &"deck A seek --at 32msbeats"
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        )
        .is_err());
    }
    fn p(s: &str) -> Invocation {
        parse(&s.split_whitespace().map(str::to_string).collect::<Vec<_>>()).unwrap()
    }
    #[test]
    fn folder_import_does_not_add_null_paths() {
        let invocation = parse(
            &["call", "library.import", "--args", "{\"folder\":\"music\"}"].map(str::to_owned),
        )
        .unwrap();
        assert_eq!(invocation.request.command, "library.import");
        assert!(invocation.request.args.get("paths").is_none());
        assert!(invocation.request.args["folder"]
            .as_str()
            .unwrap()
            .ends_with("/music"));
    }
    #[test]
    fn units_and_identity() {
        let r = p("deck b hotcue H set --at 500ms --expected-load-revision 7 --json");
        assert_eq!(
            r.request.args,
            json!({"type":"set_hot_cue_point","deck":"B","index":7,"seconds":0.5})
        );
        assert_eq!(r.request.expected_load_revision, Some(7));
        assert!(r.json);
    }
    #[test]
    fn launch_and_grid_commands_keep_units_and_both_source_guards() {
        let r = p("deck B launch --clock A --clock-revision 9 --at 32125ms --quantize 4 --expected-load-revision 7");
        assert_eq!(
            r.request.args,
            json!({"type":"play_quantized","deck":"B","clock":"A","clock_revision":9,"at_seconds":32.125,"quantize":4.0})
        );
        assert_eq!(r.request.expected_load_revision, Some(7));
        assert_eq!(
            p("deck A grid set --offset 125ms").request.args,
            json!({"type":"set_beatgrid","deck":"A","offset_seconds":0.125})
        );
        assert_eq!(
            p("deck B launch cancel").request.args,
            json!({"type":"cancel_launch","deck":"B"})
        );
        assert!(parse(
            &"deck B launch --clock A --clock-revision -1"
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        )
        .is_err());
    }
    #[test]
    fn partial_eq_and_sampler_bank_two() {
        assert_eq!(
            p("deck A eq set --low 0.2").request.args["bands"],
            json!({"low":0.2})
        );
        assert_eq!(p("sampler slot 16 trigger").request.args["slot"], 15);
    }
    #[test]
    fn rejects_invalid_inputs() {
        for s in [
            "deck C play",
            "deck A hotcue Z set",
            "deck A sync maybe",
            "deck A seek --at NaN",
            "sampler slot 17 trigger",
            "status --timeout 0ms",
            "status --json --json",
        ] {
            assert!(
                parse(&s.split_whitespace().map(str::to_string).collect::<Vec<_>>()).is_err(),
                "{s}"
            );
        }
    }
    #[test]
    fn smart_modes_and_mix() {
        assert_eq!(
            p("mixer smart-fader on").request.args,
            json!({"fader":true})
        );
        assert_eq!(
            p("mixer crossfader set -0.5").request.args,
            json!({"type":"set_crossfader","position":-0.5})
        );
    }
    #[test]
    fn new_capabilities_preserve_arguments_units_and_guards() {
        assert_eq!(
            p("library suggest --deck b --energy up").request.args,
            json!({"deck":"B","energy_direction":"raise"})
        );
        let r = p("library analysis update --no-wait");
        assert_eq!(r.request.command, "library.analysis.update");
        assert!(r.request.no_wait);
        for command in [
            "mix guide choose 0",
            "library suggest --energy louder",
            "deck A sections vocabulary rock",
            "analysis settings set",
            "analysis settings set --hot-cues perhaps",
        ] {
            assert!(
                parse(
                    &command
                        .split_whitespace()
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                )
                .is_err(),
                "{command}"
            );
        }
    }
}
