//! Bounded MCP stdio adapter over the shared typed local control registry.
use control_protocol::{ControlError, Request, Response};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

pub const PROTOCOL_VERSION: &str = "2025-11-25";
const TOOLS: &[&str] = &[
    "performance.observe",
    "performance.validate",
    "performance.commit",
    "performance.receipts",
    "performance.cancel",
    "engine.command",
    "deck.load",
    "commands.list",
    "commands.describe",
    "library.tracks",
    "track.inspect",
    "status",
    "engine.transport",
    "engine.metadata",
    "deck.unload",
    "deck.instant_double",
    "deck.load.cancel",
    "engine.wait",
    "cues.get",
    "library.track",
    "library.suggest_next",
    "spotify.liked",
    "spotify.playlists",
    "spotify.playlist_tracks",
    "spotify.album_tracks",
    "spotify.native.status",
    "ytmusic.status",
    "ytmusic.playlists",
    "ytmusic.tracks",
    "ytmusic.match",
    "mix.index_scope",
    "mix.openers",
    "mix.options",
    "mix.session.start",
    "mix.session.status",
    "mix.session.stop",
    "mix.session.successor",
    "jobs.get",
    "jobs.list",
    "events.get",
    "automation.stop",
    "sampler.load",
    "sampler.state",
    "mixer.smart_modes",
];
fn tool_entries() -> Vec<Value> {
    let catalog = control_protocol::catalog();
    TOOLS
        .iter()
        .filter_map(|name| catalog.iter().find(|e| e["command"] == *name).cloned())
        .collect()
}
fn deck_bounds(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                for name in ["deck", "clock", "carrier"] {
                    if let Some(field) = properties.get_mut(name) {
                        if field["type"] == "integer"
                            || field["type"]
                                .as_array()
                                .is_some_and(|a| a.iter().any(|v| v == "integer"))
                        {
                            field["minimum"] = json!(0);
                            field["maximum"] = json!(1);
                        }
                    }
                }
            }
            for value in object.values_mut() {
                deck_bounds(value);
            }
        }
        Value::Array(array) => {
            for value in array {
                deck_bounds(value);
            }
        }
        _ => {}
    }
}
fn request_metadata_schema(schema: &mut Value) {
    schema["properties"]["request_id"] = json!({"type":"string","minLength":1,"maxLength":128,
        "description":"Optional native retry identity. Reuse this ID and identical arguments after an uncertain acknowledgement; choose a new ID for a new action. JSON-RPC IDs only correlate responses."});
    schema["properties"]["expected_load_revision"] = json!({"type":"integer","minimum":0,
        "description":"Optional observed target-deck load revision. The native source guard rejects a control or load if that deck now contains a different source."});
    if let Some(variants) = schema.get_mut("oneOf").and_then(Value::as_array_mut) {
        for variant in variants {
            request_metadata_schema(variant);
        }
    }
}
fn descriptor(entry: &Value) -> Value {
    let command = entry["command"].as_str().unwrap();
    let (name, mut input) = if command == "engine.command" {
        (
            "deck_control".into(),
            serde_json::to_value(schemars::schema_for!(
                control_protocol::commands::EngineCommand
            ))
            .unwrap(),
        )
    } else {
        (command.replace('.', "_"), entry["input_schema"].clone())
    };
    if command == "engine.command" {
        fn state_guard_schema(schema: &mut Value) {
            schema["properties"]["expected_state"] = serde_json::to_value(schemars::schema_for!(
                control_protocol::commands::DirectStateGuard
            ))
            .unwrap();
            if let Some(variants) = schema.get_mut("oneOf").and_then(Value::as_array_mut) {
                for variant in variants {
                    state_guard_schema(variant);
                }
            }
        }
        state_guard_schema(&mut input);
    }
    input["type"] = json!("object");
    deck_bounds(&mut input);
    request_metadata_schema(&mut input);
    let mut output = entry
        .get("output_schema")
        .filter(|s| s.is_object())
        .cloned()
        .unwrap_or(json!({"type":"object"}));
    if command == "engine.command" {
        let mut receipt = serde_json::to_value(schemars::schema_for!(
            control_protocol::commands::DirectControlReceipt
        ))
        .unwrap();
        if let Some(definitions) = receipt.as_object_mut().unwrap().remove("definitions") {
            output["definitions"] = definitions;
        }
        receipt.as_object_mut().unwrap().remove("$schema");
        output["properties"]["receipt"] = json!({"anyOf":[receipt,{"type":"null"}]});
    }
    deck_bounds(&mut output);
    json!({"name":name,"description":entry["description"],"inputSchema":input,"outputSchema":output,
        "annotations":{"readOnlyHint":control_protocol::selection::is_read(command),"idempotentHint":control_protocol::selection::is_read(command)}})
}
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn tool_error(mut error: ControlError, request_id: Option<&str>, command: &str) -> Value {
    // The shared CLI diagnostics include full schemas and shell examples. MCP
    // already carries schemas in discovery; keep runtime causes and recovery
    // coordinates without repeating that transport's guidance on every retry.
    if let Some(details) = error.details.as_object_mut() {
        for key in [
            "example",
            "expected_arguments",
            "input_schema",
            "output_schema",
        ] {
            details.remove(key);
        }
        if details.get("expected").is_some_and(|value| {
            [
                "$schema",
                "$defs",
                "definitions",
                "properties",
                "oneOf",
                "anyOf",
            ]
            .iter()
            .any(|key| value.get(key).is_some())
        }) {
            details.remove("expected");
        }
        if let Some(hint) = details.get("hint").and_then(Value::as_str) {
            if hint.contains("pulsedeckctl") || hint.contains("--") || hint.contains("login") {
                details.remove("hint");
            } else {
                details.insert(
                    "hint".into(),
                    json!(hint
                        .replace("spotify.liked", "spotify_liked")
                        .replace("spotify.playlist", "spotify_playlist_tracks")
                        .replace("jobs.get", "jobs_get")
                        .replace("cues.get", "cues_get")),
                );
            }
        }
        details.insert("schema_discovery".into(), json!({"method":"tools/list","tool":
            if command == "engine.command" { "deck_control".to_owned() } else { command.replace('.', "_") }}));
    }
    let mut result = json!({"content":[{"type":"text","text":serde_json::to_string(&error).unwrap()}],"isError":true});
    if let Some(id) = request_id {
        result["_meta"] = json!({"pulsedeck/request_id":id});
    }
    result
}
fn call_tool(
    entry: &Value,
    mut args: Value,
    call: &mut impl FnMut(Request) -> Result<Response, ControlError>,
) -> Value {
    let command = entry["command"].as_str().unwrap();
    if command == "commands.describe" {
        if let Some(name) = args["name"].as_str() {
            if let Some(canonical) = TOOLS.iter().find(|candidate| {
                (if **candidate == "engine.command" {
                    "deck_control".to_owned()
                } else {
                    candidate.replace('.', "_")
                }) == name
            }) {
                args["name"] = json!(canonical);
            }
        }
    }
    let mut native = Request::new(command, args);
    let result = (|| {
        if let Some(key) = native
            .args
            .as_object_mut()
            .and_then(|o| o.remove("request_id"))
        {
            let key = key
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or_else(|| {
                    ControlError::new(
                        "invalid_arguments",
                        "request_id must be a nonempty string of at most 128 bytes",
                    )
                })?;
            native.request_id = key.to_owned();
        }
        if let Some(revision) = native
            .args
            .as_object_mut()
            .and_then(|o| o.remove("expected_load_revision"))
        {
            native.expected_load_revision = Some(revision.as_u64().ok_or_else(|| {
                ControlError::new(
                    "invalid_arguments",
                    "expected_load_revision must be a nonnegative integer",
                )
            })?);
        }
        if command == "engine.command" {
            if let Some(guard) = native
                .args
                .as_object_mut()
                .and_then(|o| o.remove("expected_state"))
            {
                native.expected_state = Some(
                    serde_json::from_value(guard)
                        .map_err(|e| ControlError::new("invalid_arguments", e.to_string()))?,
                );
            }
            let typed: control_protocol::commands::EngineCommand =
                serde_json::from_value(native.args.clone())
                    .map_err(|e| ControlError::new("invalid_arguments", e.to_string()))?;
            typed
                .validate()
                .map_err(|e| ControlError::new("invalid_arguments", e))?;
            native.args = json!({"command":typed});
        }
        native.validate()?;
        control_protocol::validate_arguments(&native)?;
        call(native.clone())
    })();
    match result {
        Ok(response) if response.ok => {
            let mut data = response.result.unwrap_or(json!({}));
            if command == "engine.command" && data["receipt"].is_object() {
                // The CLI retains the detailed engine packet. Native typed
                // receipts are sufficient for the MCP action acknowledgement;
                // deliberate observations use the dedicated read tools.
                data.as_object_mut().unwrap().remove("engine");
            }
            let data = if data.is_object() {
                data
            } else {
                json!({"value":data})
            };
            json!({"content":[{"type":"text","text":data.to_string()}],"structuredContent":data,"isError":false,
                "_meta":{"pulsedeck/request_id":response.request_id}})
        }
        Ok(response) => tool_error(
            response.error.unwrap_or_else(|| {
                ControlError::new("operation_failed", "Control returned no error")
            }),
            Some(&response.request_id),
            command,
        ),
        Err(error) => tool_error(error, Some(&native.request_id), command),
    }
}
fn server_instructions(root: Option<&std::path::Path>) -> Result<String, &'static str> {
    let mut instructions = "Artist chooses the music and arrangement. Observe, validate, inspect the transition forecast, commit, then compare actual receipts with the intended musical course. Tool pages contain DJ performance/source capabilities. For uncertain direct-action acknowledgements reuse arguments.request_id and identical arguments; without that key do not blindly retry toggles, jumps or other nonidempotent actions. Native request IDs are returned in result._meta; performance plans also retain operation_id.".to_owned();
    if let Some(root) = root {
        let root = root
            .canonicalize()
            .map_err(|_| "PulseDeck plugin skill directory is unavailable; reinstall the plugin")?;
        let mut paths = serde_json::Map::new();
        for (name, relative) in [
            ("art", "djing-art/SKILL.md"),
            (
                "waveform_reasoning",
                "djing-art/references/data-to-decisions.md",
            ),
            ("library", "pulsedeck-library/SKILL.md"),
            ("control", "pulsedeck-control/SKILL.md"),
        ] {
            let path = root.join(relative);
            if !path.is_file() {
                return Err("PulseDeck plugin skill reference is missing; reinstall the plugin");
            }
            paths.insert(name.into(), json!(path));
        }
        instructions.push_str(&format!("\nInstalled PulseDeck skill files, resolved from this plugin installation: {}. Read the art and waveform_reasoning files for musical planning. Resolve other relative references from the SKILL.md file that links them.", Value::Object(paths)));
    }
    Ok(instructions)
}
fn initialize(params: &Value) -> Result<Value, &'static str> {
    if !params["protocolVersion"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
        || !params["capabilities"].is_object()
        || !params["clientInfo"]["name"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        || !params["clientInfo"]["version"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        return Err(
            "initialize requires protocolVersion, capabilities and clientInfo.name/version",
        );
    }
    // Only this revision is implemented. Older/newer clients may accept this
    // supported counteroffer, or disconnect; never echo an unsupported version.
    let root = std::env::var_os("PULSEDECK_AGENT_SKILLS_DIR");
    let instructions = server_instructions(root.as_deref().map(std::path::Path::new))?;
    Ok(
        json!({"protocolVersion":PROTOCOL_VERSION,"capabilities":{"tools":{"listChanged":false}},
        "serverInfo":{"name":"pulsedeck","version":env!("CARGO_PKG_VERSION")},
        "instructions":instructions}),
    )
}
fn dispatch(
    request: &Value,
    initialized: &mut bool,
    call: &mut impl FnMut(Request) -> Result<Response, ControlError>,
) -> Value {
    let id = request["id"].clone();
    let method = request["method"].as_str().unwrap();
    let params = request.get("params").cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return rpc_error(id, -32602, "MCP params must be an object");
    }
    let result = if method == "initialize" {
        match initialize(&params) {
            Ok(result) => {
                *initialized = true;
                result
            }
            Err(message) => return rpc_error(id, -32602, message),
        }
    } else if method == "ping" {
        json!({})
    } else if !*initialized {
        return rpc_error(id, -32000, "Initialize before calling tools");
    } else if method == "tools/list" {
        let cursor = match params.get("cursor") {
            None => "0",
            Some(Value::String(s)) => s,
            _ => return rpc_error(id, -32602, "Tool cursor must be a string"),
        };
        let start = match cursor.parse::<usize>() {
            Ok(n) => n,
            Err(_) => return rpc_error(id, -32602, "Invalid tool cursor"),
        };
        let entries = tool_entries();
        let count = if start == 0 { 11 } else { 16 };
        if start >= entries.len() && start != 0 {
            return rpc_error(id, -32602, "Tool cursor outside DJ registry");
        }
        let mut result = json!({"tools":entries.iter().skip(start).take(count).map(descriptor).collect::<Vec<_>>()});
        if start + count < entries.len() {
            result["nextCursor"] = json!((start + count).to_string());
        }
        result
    } else if method == "tools/call" {
        let Some(name) = params["name"].as_str() else {
            return rpc_error(id, -32602, "tools/call requires a tool name");
        };
        let Some(entry) = tool_entries()
            .into_iter()
            .find(|e| descriptor(e)["name"] == name)
        else {
            return rpc_error(id, -32602, "Unknown DJ tool; inspect tools/list");
        };
        call_tool(
            &entry,
            params.get("arguments").cloned().unwrap_or(json!({})),
            call,
        )
    } else {
        return rpc_error(id, -32601, "Method not found");
    };
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
    mut call: impl FnMut(Request) -> Result<Response, ControlError>,
) -> std::io::Result<()> {
    let mut input = input;
    let mut initialized = false;
    loop {
        let mut frame = Vec::new();
        let read = (&mut input)
            .take((control_protocol::MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut frame)?;
        if read == 0 {
            return Ok(());
        }
        if read > control_protocol::MAX_FRAME || frame.last() != Some(&b'\n') {
            writeln!(
                output,
                "{}",
                rpc_error(Value::Null, -32600, "Frame exceeds 4MiB or lacks newline")
            )?;
            output.flush()?;
            return Ok(());
        }
        let response = match serde_json::from_slice::<Value>(&frame) {
            Err(_) => rpc_error(Value::Null, -32700, "Invalid JSON"),
            Ok(request) => {
                let valid = request.is_object()
                    && request["jsonrpc"] == "2.0"
                    && request["method"].is_string()
                    && request
                        .get("id")
                        .is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64());
                if !valid {
                    rpc_error(Value::Null, -32600, "Invalid MCP JSON-RPC request")
                } else if request.get("id").is_none() {
                    continue;
                } else {
                    dispatch(&request, &mut initialized, &mut call)
                }
            }
        };
        writeln!(output, "{response}")?;
        output.flush()?;
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
