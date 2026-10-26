use super::*;
use std::collections::HashMap;
use std::io::Cursor;

fn init(id: u64, version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn run(
    messages: &[Value],
    call: impl FnMut(Request) -> Result<Response, ControlError>,
) -> Vec<Value> {
    let input = messages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, call).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn no_call(_: Request) -> Result<Response, ControlError> {
    panic!("No app call expected")
}

#[test]
fn plugin_guidance_resolves_relocated_skill_files_and_reports_missing_references() {
    let base = std::env::temp_dir().join(format!(
        "pd-plugin-guidance-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("installed plugin/skills");
    for relative in [
        "djing-art/SKILL.md",
        "djing-art/references/data-to-decisions.md",
        "pulsedeck-library/SKILL.md",
        "pulsedeck-control/SKILL.md",
    ] {
        let file = root.join(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "guide").unwrap();
    }
    let first = server_instructions(Some(&root)).unwrap();
    assert!(first.contains(root.canonicalize().unwrap().to_str().unwrap()));
    std::fs::rename(base.join("installed plugin"), base.join("relocated plugin")).unwrap();
    let moved = base.join("relocated plugin/skills");
    let guidance = server_instructions(Some(&moved)).unwrap();
    assert!(guidance.contains(
        moved
            .canonicalize()
            .unwrap()
            .join("djing-art/references/data-to-decisions.md")
            .to_str()
            .unwrap()
    ));
    assert!(!guidance.contains("installed plugin"));
    assert!(server_instructions(Some(&root)).is_err());
    std::fs::remove_file(moved.join("djing-art/references/data-to-decisions.md")).unwrap();
    assert!(server_instructions(Some(&moved)).is_err());
    assert!(server_instructions(None).is_ok());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn seconds_ramp_accepts_omitted_optional_clock() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_control",
                json!({
                    "type":"ramp","target":{"type":"channel_fader","deck":"B"},
                    "value":0.2,"seconds":8
                }),
            ),
        ],
        |request| {
            assert_eq!(request.args["command"]["clock"], Value::Null);
            Ok(Response::success(
                &request,
                "applied",
                json!({"accepted":true}),
            ))
        },
    );
    assert_eq!(responses[1]["result"]["isError"], false);
}
#[test]
fn direct_expected_state_is_typed_metadata_and_retry_returns_original_native_receipt() {
    let messages = [
        init(1, PROTOCOL_VERSION),
        tool(
            2,
            "deck_control",
            json!({"type":"seek","deck":"B","seconds":4,"request_id":"prep:1","expected_state":{"playing":false,"launch_pending":false,"channel_fader_max":0.002}}),
        ),
        tool(
            3,
            "deck_control",
            json!({"type":"seek","deck":"B","seconds":4,"request_id":"prep:1","expected_state":{"playing":false,"launch_pending":false,"channel_fader_max":0.002}}),
        ),
        tool(
            4,
            "deck_control",
            json!({"type":"seek","deck":"B","seconds":4,"request_id":"prep:1","expected_state":{"playing":true}}),
        ),
    ];
    let mut cached: Option<(Value, Response)> = None;
    let mut executed = 0;
    let responses = run(&messages, |request| {
        assert!(request.args["command"].get("expected_state").is_none());
        let identity = json!([request.args, request.expected_state]);
        if let Some((original, response)) = &cached {
            return if *original == identity {
                Ok(response.clone())
            } else {
                Err(ControlError::new(
                    "idempotency_conflict",
                    "Expected state changed",
                ))
            };
        }
        assert_eq!(request.expected_state.unwrap().playing, Some(false));
        executed += 1;
        let response = Response::success(
            &request,
            "applied",
            json!({"receipt":{"ticket":41,"outcome":"applied","before":{"playing":false},"after":{"playing":false}},"engine":{"large":"packet"}}),
        );
        cached = Some((identity, response.clone()));
        Ok(response)
    });
    assert_eq!(executed, 1);
    assert_eq!(
        responses[1]["result"]["structuredContent"],
        responses[2]["result"]["structuredContent"]
    );
    assert_eq!(
        responses[2]["result"]["structuredContent"]["receipt"]["ticket"],
        41
    );
    assert!(responses[1]["result"]["structuredContent"]
        .get("engine")
        .is_none());
    assert_eq!(responses[3]["result"]["isError"], true);
}
#[test]
fn direct_expected_state_bad_units_and_unknown_predicates_never_reach_app() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_control",
                json!({"type":"seek","deck":"B","seconds":4,"expected_state":{"channel_fader_max":2}}),
            ),
            tool(
                3,
                "deck_control",
                json!({"type":"seek","deck":"B","seconds":4,"expected_state":{"audible":false}}),
            ),
        ],
        no_call,
    );
    assert!(responses[1..]
        .iter()
        .all(|r| r["result"]["isError"] == true));
    let entry = tool_entries()
        .into_iter()
        .find(|e| e["command"] == "engine.command")
        .unwrap();
    let schema = descriptor(&entry);
    assert_eq!(
        schema["inputSchema"]["properties"]["expected_state"]["properties"]["channel_fader_max"]
            ["maximum"],
        1.0
    );
    assert!(schema["outputSchema"]["properties"]["receipt"].is_object());
}

#[test]
fn command_discovery_accepts_advertised_tool_names_and_canonical_names() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(2, "commands_describe", json!({"name":"deck_control"})),
            tool(
                3,
                "commands_describe",
                json!({"name":"performance_observe"}),
            ),
            tool(
                4,
                "commands_describe",
                json!({"name":"performance.observe"}),
            ),
            tool(5, "commands_describe", json!({"name":"not_a_tool"})),
        ],
        |request| {
            Ok(Response::success(
                &request,
                "described",
                request.args.clone(),
            ))
        },
    );
    for (response, expected) in responses[1..].iter().zip([
        "engine.command",
        "performance.observe",
        "performance.observe",
        "not_a_tool",
    ]) {
        assert_eq!(response["result"]["structuredContent"]["name"], expected);
    }
}

#[test]
fn tool_failures_keep_runtime_cause_and_retry_identity_without_cli_schema_dump() {
    let messages = [
        init(1, PROTOCOL_VERSION),
        tool(
            2,
            "deck_control",
            json!({"type":"set_filter","deck":"B","amount":0.2,"request_id":"gesture:1"}),
        ),
        tool(
            3,
            "deck_control",
            json!({"type":"set_filter","deck":"B","amount":0.1,"request_id":"gesture:2"}),
        ),
    ];
    let mut calls = 0;
    let responses = run(&messages, |request| {
        calls += 1;
        if calls == 1 {
            return Ok(Response::failure(
                request.request_id.clone(),
                ControlError::new(
                    "dependency_deadline",
                    "Transfer precedes verified reveal completion",
                )
                .with_detail("action", "transfer")
                .with_detail("dependency", "reveal")
                .with_detail("start_seconds", 15.6)
                .with_detail("completion_seconds", 16.02)
                .with_detail("expected_arguments", json!({"oneOf":[{"large":"schema"}]}))
                .with_detail(
                    "expected",
                    json!({"type":"object","properties":{"large":"schema"}}),
                )
                .with_detail("example", "pulsedeckctl call engine.command --args '{}'"),
            ));
        }
        Ok(Response::success(&request, "applied", json!({"done":true})))
    });
    let result = &responses[1]["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(result["_meta"]["pulsedeck/request_id"], "gesture:1");
    let error: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["code"], "dependency_deadline");
    assert_eq!(error["details"]["action"], "transfer");
    assert_eq!(error["details"]["dependency"], "reveal");
    assert_eq!(error["details"]["completion_seconds"], 16.02);
    assert_eq!(
        error["details"]["schema_discovery"],
        json!({"method":"tools/list","tool":"deck_control"})
    );
    assert!(error["details"].get("expected_arguments").is_none());
    assert!(!result.to_string().contains("pulsedeckctl"));
    assert_eq!(
        responses[2]["result"]["isError"], false,
        "A tool failure does not poison the stdio session"
    );
    assert_eq!(calls, 2);
}

#[test]
fn compact_errors_keep_concrete_expected_state_and_valid_bounds() {
    for expected in [
        json!({"playing":false,"channel_fader_max":0.002}),
        json!({"minimum":0,"maximum":230.5}),
    ] {
        let result = tool_error(
            ControlError::new("state_conflict", "Control state changed")
                .with_detail("expected", expected.clone())
                .with_detail("actual", json!({"playing":true,"channel_fader":1})),
            Some("recue:1"),
            "engine.command",
        );
        let error: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(error["details"]["expected"], expected);
        assert_eq!(error["details"]["actual"]["playing"], true);
    }
}

#[test]
fn compact_errors_retain_actionable_source_guidance_in_mcp_names() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_load",
                json!({"deck":"B","uri":"bad","request_id":"load:1"}),
            ),
        ],
        |_| {
            Err(
                ControlError::new("invalid_track_uri", "Invalid URI").with_detail(
                    "hint",
                    "Copy the complete URI from spotify.liked or spotify.playlist results.",
                ),
            )
        },
    );
    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let error: Value = serde_json::from_str(text).unwrap();
    assert!(error["details"]["hint"]
        .as_str()
        .unwrap()
        .contains("spotify_liked"));
    assert!(error["details"]["hint"]
        .as_str()
        .unwrap()
        .contains("spotify_playlist_tracks"));
    assert_eq!(error["details"]["schema_discovery"]["tool"], "deck_load");
}

#[test]
fn library_playlist_schema_distinguishes_local_ids_from_spotify_collections() {
    let entry = tool_entries()
        .into_iter()
        .find(|e| e["command"] == "library.tracks")
        .unwrap();
    let schema = descriptor(&entry);
    let description = schema["inputSchema"]["properties"]["playlist"]["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("local:ID"));
    assert!(description.contains("spotify_liked"));
    assert!(description.contains("spotify_playlist_tracks"));
}
fn tool(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}

#[test]
fn handshake_negotiates_only_supported_revision_and_notifications_emit_nothing() {
    let responses = run(
        &[
            init(1, "2025-11-25"),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","method":"notifications/unknown","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
            init(3, "2026-07-28"),
        ],
        no_call,
    );
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(responses[1]["result"], json!({}));
    assert_eq!(responses[2]["result"]["protocolVersion"], PROTOCOL_VERSION);
}

#[test]
fn invalid_handshake_does_not_initialize_or_call_app() {
    let responses = run(
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}),
        ],
        no_call,
    );
    assert_eq!(responses[0]["error"]["code"], -32602);
    assert_eq!(responses[1]["error"]["code"], -32000);
    assert_eq!(responses[2]["error"]["code"], -32600);
    assert_eq!(responses[3]["error"]["code"], -32600);
}

#[test]
fn invalid_json_and_unknown_methods_are_valid_jsonrpc_errors() {
    let input = format!(
        "{{bad}}\n{}\n{}\n",
        init(1, PROTOCOL_VERSION),
        json!({"jsonrpc":"2.0","id":"unknown","method":"not/implemented"})
    );
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, no_call).unwrap();
    let responses = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["error"]["code"], -32700);
    assert_eq!(responses[2]["error"]["code"], -32601);
    assert_eq!(responses[2]["id"], "unknown");
}

#[test]
fn all_tool_pages_remain_inside_dj_scope_and_are_typed() {
    let entries = tool_entries();
    assert_eq!(entries.len(), TOOLS.len());
    for entry in &entries {
        let command = entry["command"].as_str().unwrap();
        assert!(TOOLS.contains(&command));
        assert!(!command.starts_with("ui.") && !command.starts_with("app."));
        assert!(!command.contains("login") && !command.contains("import"));
        let descriptor = descriptor(entry);
        assert_eq!(descriptor["inputSchema"]["type"], "object");
        assert_eq!(descriptor["outputSchema"]["type"], "object");
        assert_eq!(
            descriptor["inputSchema"]["properties"]["request_id"]["maxLength"],
            128
        );
    }
    let mut requests = vec![init(1, PROTOCOL_VERSION)];
    for start in [0, 11, 27] {
        requests.push(json!({"jsonrpc":"2.0","id":start+2,"method":"tools/list","params":{"cursor":start.to_string()}}));
    }
    let responses = run(&requests, no_call);
    let names = responses[1..]
        .iter()
        .flat_map(|r| r["result"]["tools"].as_array().unwrap().iter())
        .map(|t| t["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), TOOLS.len());
    assert!(names.contains(&"deck_control") && names.contains(&"performance_commit"));
}

#[test]
fn invalid_tool_params_and_out_of_scope_tool_never_reach_app() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":11}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"cursor":"99999"}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":[]}),
            tool(5, "app_stop", json!({})),
            tool(6, "deck_control", json!({"type":"play","deck":2})),
            tool(
                7,
                "deck_control",
                json!({"type":"play","deck":0,"request_id":""}),
            ),
        ],
        no_call,
    );
    for response in &responses[1..5] {
        assert_eq!(response["error"]["code"], -32602);
    }
    assert_eq!(responses[5]["result"]["isError"], true);
    assert_eq!(responses[6]["result"]["isError"], true);
}

#[test]
fn exact_native_retry_key_is_independent_of_jsonrpc_id_and_not_part_of_command() {
    let messages = [
        init(1, PROTOCOL_VERSION),
        tool(
            2,
            "deck_control",
            json!({"type":"toggle_play","deck":0,"request_id":"set-1:toggle-1"}),
        ),
        tool(
            3,
            "deck_control",
            json!({"type":"toggle_play","deck":0,"request_id":"set-1:toggle-1"}),
        ),
        tool(
            4,
            "deck_control",
            json!({"type":"toggle_play","deck":1,"request_id":"set-1:toggle-1"}),
        ),
    ];
    let mut executed = 0;
    let mut receipts: HashMap<String, (Value, Response)> = HashMap::new();
    let responses = run(&messages, |request| {
        assert_eq!(request.command, "engine.command");
        assert!(request.args["command"].get("request_id").is_none());
        if let Some((body, response)) = receipts.get(&request.request_id) {
            if body != &request.args {
                return Err(ControlError::new(
                    "idempotency_conflict",
                    "Retry body changed",
                ));
            }
            return Ok(response.clone());
        }
        executed += 1;
        let response = Response::success(&request, "applied", json!({"executed":executed}));
        receipts.insert(request.request_id.clone(), (request.args, response.clone()));
        Ok(response)
    });
    assert_eq!(executed, 1);
    assert_eq!(
        responses[1]["result"]["structuredContent"],
        responses[2]["result"]["structuredContent"]
    );
    assert_eq!(
        responses[2]["result"]["_meta"]["pulsedeck/request_id"],
        "set-1:toggle-1"
    );
    assert_eq!(responses[3]["result"]["isError"], true);
    assert_eq!(responses[2]["id"], 3);
}

#[test]
fn requests_without_retry_key_remain_distinct_and_expose_native_identity() {
    let mut ids = Vec::new();
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(2, "deck_control", json!({"type":"play","deck":0})),
            tool(3, "deck_control", json!({"type":"play","deck":0})),
        ],
        |request| {
            ids.push(request.request_id.clone());
            Ok(Response::success(&request, "applied", json!({"done":true})))
        },
    );
    assert_ne!(ids[0], ids[1]);
    assert_eq!(
        responses[1]["result"]["_meta"]["pulsedeck/request_id"],
        ids[0]
    );
    assert_eq!(
        responses[1]["result"]["structuredContent"],
        json!({"done":true})
    );
    let deck = descriptor(
        tool_entries()
            .iter()
            .find(|e| e["command"] == "engine.command")
            .unwrap(),
    );
    assert_eq!(deck["annotations"]["idempotentHint"], false);
}

#[test]
fn source_guard_metadata_reaches_native_request_without_entering_typed_payload() {
    let mut observed = Vec::new();
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_control",
                json!({"type":"play","deck":"B","request_id":"guarded-play","expected_load_revision":17}),
            ),
            tool(
                3,
                "deck_load",
                json!({"deck":"A","track":"local:1","expected_load_revision":8}),
            ),
        ],
        |request| {
            observed.push(request.clone());
            Ok(Response::success(&request, "applied", json!({"done":true})))
        },
    );
    assert_eq!(observed[0].expected_load_revision, Some(17));
    assert_eq!(observed[0].request_id, "guarded-play");
    assert_eq!(
        observed[0].args,
        json!({"command":{"type":"play","deck":1}})
    );
    assert_eq!(observed[1].expected_load_revision, Some(8));
    assert!(observed[1].args.get("expected_load_revision").is_none());
    assert_eq!(responses[1]["result"]["isError"], false);
    let bad = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_control",
                json!({"type":"play","deck":0,"expected_load_revision":-1}),
            ),
        ],
        no_call,
    );
    assert_eq!(bad[1]["result"]["isError"], true);
}

#[test]
fn typed_deck_schema_exposes_numeric_domain_and_keeps_retry_key_on_variants() {
    let entry = tool_entries()
        .into_iter()
        .find(|e| e["command"] == "engine.command")
        .unwrap();
    let schema = descriptor(&entry)["inputSchema"].clone();
    let variants = schema["oneOf"].as_array().unwrap();
    let play = variants
        .iter()
        .find(|v| v["properties"]["type"]["enum"] == json!(["play"]))
        .unwrap();
    assert_eq!(
        play["properties"]["deck"]["oneOf"][0]["enum"],
        json!([0, 1])
    );
    assert_eq!(
        play["properties"]["deck"]["oneOf"][1]["enum"],
        json!(["A", "B"])
    );
    assert!(play["properties"].get("request_id").is_some());
    assert_eq!(play["properties"]["expected_load_revision"]["minimum"], 0);
    assert_eq!(play["additionalProperties"], false);
}

#[test]
fn typed_command_rejects_unknown_intent_at_top_level_and_nested_target() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "deck_control",
                json!({"type":"set_sync","deck":"B","enabled":true,"align_bar":true}),
            ),
            tool(
                3,
                "deck_control",
                json!({"type":"ramp","target":{"type":"eq_low","deck":0,"align_bar":true},"value":1,"seconds":1}),
            ),
        ],
        no_call,
    );
    for response in &responses[1..] {
        assert_eq!(response["result"]["isError"], true);
        let error: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(error["code"], "invalid_arguments");
        assert!(error["message"].as_str().unwrap().contains("unknown field"));
    }
}

#[test]
fn frame_limit_is_enforced_before_consuming_unbounded_input() {
    let mut input = Cursor::new(vec![b'x'; control_protocol::MAX_FRAME + 100]);
    let mut output = Vec::new();
    serve(&mut input, &mut output, no_call).unwrap();
    assert_eq!(input.position(), control_protocol::MAX_FRAME as u64 + 1);
    let error: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(error["error"]["code"], -32600);
    let mut incomplete = Vec::new();
    serve(Cursor::new(b"{}"), &mut incomplete, no_call).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&incomplete).unwrap()["error"]["code"],
        -32600
    );
}

#[test]
fn reviewed_ticket_commit_is_small_and_preserves_retry_identity() {
    let responses = run(
        &[
            init(1, PROTOCOL_VERSION),
            tool(
                2,
                "performance_commit",
                json!({"ticket":"score-reviewed-1"}),
            ),
            tool(
                3,
                "performance_commit",
                json!({"ticket":"score-reviewed-1"}),
            ),
        ],
        |request| {
            assert_eq!(request.command, "performance.commit");
            assert_eq!(request.args, json!({"ticket":"score-reviewed-1"}));
            Ok(Response::success(
                &request,
                "accepted",
                json!({"job_id":"same-job","replayed":false}),
            ))
        },
    );
    for response in &responses[1..] {
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(
            response["result"]["structuredContent"]["job_id"],
            "same-job"
        );
    }
}
