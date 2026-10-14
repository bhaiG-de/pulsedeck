use control_protocol::{ControlError, Request};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub const HELP: &str = "PulseDeck control CLI (pulsedeckctl)\n\nOpen PulseDeck, then use:\n  pulsedeckctl status --json\n  pulsedeckctl deck A load --path /music/song.flac\n  pulsedeckctl deck A play|pause|cue|seek --at 32s\n  pulsedeckctl deck A hotcue A set --at 32s\n  pulsedeckctl deck A tempo set --percent -2\n  pulsedeckctl deck A eq set --low 0.7\n  pulsedeckctl mixer crossfader set -0.5\n  pulsedeckctl library tracks --search ambient\n  pulsedeckctl playlist create --name Work\n  pulsedeckctl playlist add local:7 --track local:42\n  pulsedeckctl ui theme set anatomy-dark\n  pulsedeckctl commands list --json\n  pulsedeckctl call METHOD --args '{\"key\":\"value\"}'\n  pulsedeckctl command --file request.json\n  pulsedeckctl batch run --file plan.json\n  pulsedeckctl state watch [--count 10] [--interval 250ms]\n  pulsedeckctl session --stdio\n\nGlobal: --json --socket PATH --timeout 30s --no-wait\n        --expected-load-revision N --request-id ID\nNative slots are zero-based in JSON; named hot cues A-H / samples 1-16.\nUse commands list/describe for the complete callable catalog.\n";
#[derive(Debug)]
pub struct Invocation {
    pub request: Request,
    pub socket: PathBuf,
    pub json: bool,
    pub watch: bool,
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

pub fn parse_session(tokens: &[String]) -> Result<PathBuf, ControlError> {
    let mut socket = control_protocol::socket_path();
    let mut stdio = false;
    let mut seen_socket = false;
    let mut i = 1;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "--stdio" if !stdio => stdio = true,
            "--json" => {}
            "--socket" if !seen_socket => {
                i += 1;
                let path = tokens
                    .get(i)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| invalid("Missing path for --socket"))?;
                socket = PathBuf::from(path);
                seen_socket = true;
            }
            token => {
                return Err(invalid(format!(
                    "Unknown, duplicate or inapplicable session option {token}"
                ))
                .with_detail("valid_options", json!(["--stdio", "--socket PATH"]))
                .with_detail(
                    "example",
                    "pulsedeckctl session --stdio --socket /tmp/control.sock",
                ))
            }
        }
        i += 1;
    }
    if !stdio {
        return Err(invalid("Session mode requires --stdio")
            .with_detail("example", "pulsedeckctl session --stdio"));
    }
    Ok(socket)
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
        if error.message.contains("Unknown command") {error=error.with_detail("suggestions",json!(control_protocol::suggestions(topic,["status","state","deck","mixer","sampler","library","playlist","audio","midi","recording","spotify","ui","commands","call","command","batch","jobs","automation"].map(str::to_string))));}
        error
    })
}
fn parse_inner(tokens: &[String]) -> Result<Invocation, ControlError> {
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
                    | "no-wait"
                    | "demo"
                    | "replace-playing"
                    | "overwrite"
                    | "waveform"
                    | "current"
                    | "scratch"
                    | "shift"
                    | "ramps"
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
        ["status"] => Request::new("status", json!({})),
        ["state", "get"] | ["state"] => Request::new("state.get", json!(flags)),
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
        ["commands", "list"] | ["capabilities"] => Request::new("commands.list", json!({})),
        ["commands", "describe", name] => Request::new("commands.describe", json!({"name":name})),
        ["batch", action] if matches!(*action, "run" | "validate") => {
            let file = text(required(&flags, "file")?);
            let plan: Value =
                serde_json::from_str(&std::fs::read_to_string(file).map_err(invalid)?)
                    .map_err(invalid)?;
            Request::new(
                format!("batch.{action}"),
                if plan.is_array() {
                    json!({"steps":plan})
                } else {
                    plan
                },
            )
        }
        ["jobs", action] if *action == "list" => Request::new("jobs.list", json!({})),
        ["jobs", action, id] if matches!(*action, "get" | "cancel" | "wait") => {
            watch = *action == "wait";
            Request::new(
                if watch {
                    "jobs.get".into()
                } else {
                    format!("jobs.{action}")
                },
                json!({"id":id}),
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
        ["library", "import"] => {
            let paths = if let Some(p) = flags.remove("paths") {
                p
            } else {
                json!([text(required(&flags, "path")?)])
            };
            Request::new("library.import", json!({"paths":paths}))
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
                    return Err(invalid("Use ui theme set THEME; valid themes: hardware-dark, anatomy-light, anatomy-dark"));
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
        if let Some(paths) = request.args["paths"].as_array_mut() {
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
    }
    Ok(Invocation {
        request,
        socket,
        json: json_output,
        watch,
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
    ]
    .map(str::to_string)
    .to_vec();
    let extra: &[&str] = match words {
        ["call", method] => {
            let mut extra = vec!["args".to_string()];
            if let Some(desc) = control_protocol::catalog()
                .iter()
                .find(|d| d["command"] == *method)
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
        ["command"] | ["batch", _] => &["file"],
        ["state", "watch"] => &["count", "interval"],
        ["events", "watch"] => &["count", "interval", "since"],
        ["jobs", "wait", _] => &["count", "interval"],
        ["deck", _, "load"] => &["path", "track", "uri", "demo", "replace_playing"],
        ["deck", _, "cue", "set"] | ["deck", _, "hotcue", _, "set"] => &["at", "current"],
        ["deck", _, "seek"] => &["at", "snap"],
        ["deck", _, "jump"] | ["deck", _, "loop", "set"] => &["beats"],
        ["deck", _, "tempo", "set"] => &["percent"],
        ["deck", _, "pitch", "set"] => &["semitones"],
        ["deck", _, "grid", "set"] => &["offset"],
        ["deck", _, "eq", "set"] => &["low", "mid", "high"],
        ["deck", _, "fx", "set"] => &["echo", "reverb"],
        ["deck", _, "jog"] => &["delta", "scratch"],
        ["deck", _, "scrub", "to"] => &["at", "duration"],
        ["deck", _, "scrub", "end"] => &["at"],
        ["deck", _, "wait"] => &["ramps"],
        ["mixer", _, "ramp"] => &["to", "over", "clock", "quantize"],
        ["mixer", _, "set"] => &["position", "gain", "mix"],
        ["sampler", "slot", _, "load"] => &["path"],
        ["library", "import"] => &["path", "paths"],
        ["library", "track", "tags", _] => &["tags"],
        ["library", "tracks"] | ["library", "tracks", "list"] => {
            &["search", "sort", "playlist", "offset", "limit", "waveform"]
        }
        ["library", "import-rekordbox"] => &["path"],
        ["library", "export-rekordbox"] => &["path", "overwrite"],
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
        ["recording", "start"] => &["path", "overwrite"],
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
        ["tempo", "set"] => {
            c["percent"] = json!(scalar(&flags, "percent")?);
            "set_tempo"
        }
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
}
