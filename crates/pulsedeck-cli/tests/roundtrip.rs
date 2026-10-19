#[cfg(unix)]
#[test]
fn executable_sends_native_command_and_keeps_stdout_machine_readable() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::json;
    let root = std::env::temp_dir().join(format!("pd-cli-test-{}", std::process::id()));
    let path = root.join("control.sock");
    let server=Server::start(path.clone(),|r| {
        let result = if r.command == "jobs.get" { json!({"state":"failed"}) }
            else { json!({"command":r.command,"args":r.args,"expected_load_revision":r.expected_load_revision}) };
        Response::success(&r,"applied",result)
    }).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "deck",
            "B",
            "hotcue",
            "H",
            "set",
            "--at",
            "500ms",
            "--expected-load-revision",
            "4",
            "--json",
            "--socket",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["result"]["args"],
        json!({"type":"set_hot_cue_point","deck":"B","index":7,"seconds":0.5})
    );
    assert_eq!(value["result"]["expected_load_revision"], 4);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args(["deck", "C", "play", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["ok"], false);
    assert!(error["error"]["details"]["example"].is_string());
    // JSON request files retain their source guard, timeout and idempotency key.
    let file = root.join("request.json");
    std::fs::write(&file,r#"{"command":"engine.command","request_id":"fixed","expected_load_revision":9,"timeout_ms":12000,"args":{"type":"pause","deck":"A"}}"#).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "command",
            "--file",
            file.to_str().unwrap(),
            "--socket",
            path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["request_id"], "fixed");
    assert_eq!(value["result"]["expected_load_revision"], 9);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args(["session", "--stdio", "--socket", path.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"not-json\n{\"command\":\"status\",\"request_id\":\"stdio\"}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], "invalid_request");
    assert_eq!(lines[1]["request_id"], "stdio");
    assert_eq!(lines[1]["result"]["command"], "status");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args(["session", "--stdio", "--soket", "wrong"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    for (action, expected) in [("get", 0), ("wait", 5)] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
            .args([
                "jobs",
                action,
                "failed-job",
                "--socket",
                path.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["result"]["state"], "failed");
    }
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}
#[cfg(unix)]
#[test]
fn scheduled_plan_compact_reads_and_terminal_recovery_use_existing_cli_contracts() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::{json, Value};
    let root = std::env::temp_dir().join(format!("pd-scheduled-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.join("control.sock");
    let _server = Server::start(socket.clone(), |r| {
        let result = match r.command.as_str() {
            "jobs.get" => json!({"state":if r.args["id"]=="manual"{"yielded"}else{"recovered"}}),
            _ => json!({"command":r.command,"args":r.args,"view":r.view,"fields":r.fields}),
        };
        Response::success(&r, "applied", result)
    })
    .unwrap();
    let file = root.join("plan.json");
    std::fs::write(
        &file,
        include_str!("../../../research/examples/scheduled-handover.fixture.json"),
    )
    .unwrap();
    for action in ["validate", "run"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
            .args([
                "batch",
                action,
                "--file",
                file.to_str().unwrap(),
                "--socket",
                socket.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["result"]["command"], format!("batch.{action}"));
        assert_eq!(v["result"]["args"]["steps"].as_array().unwrap().len(), 4);
    }
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "status",
            "--view",
            "resume",
            "--fields",
            "/engine,/jobs",
            "--socket",
            socket.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["view"], "resume");
    assert_eq!(v["result"]["fields"], json!(["/engine", "/jobs"]));
    for (id, code) in [("recovered", 0), ("manual", 7)] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
            .args([
                "jobs",
                "wait",
                id,
                "--socket",
                socket.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(code));
        let lines: Vec<Value> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(lines.len(), 1);
    }
}

#[test]
fn incremental_batch_cli_preserves_patch_identity_and_event_cursor() {
    use pulsedeck_cli::parse;
    let parse = |args: &[&str]| {
        parse(&args.iter().map(|v| v.to_string()).collect::<Vec<_>>())
            .unwrap()
            .request
    };
    let append = parse(&[
        "batch",
        "append",
        "--id",
        "job-1",
        "--patch-id",
        "phase-2",
        "--expected-revision",
        "3",
        "--steps",
        r#"[{"action_id":"fade-v2","command":"engine.command","args":{"type":"set_channel_fader","deck":"B","value":1},"at_event":80}]"#,
        "--invalidate-action-ids",
        r#"["fade-v1"]"#,
    ]);
    assert_eq!(append.command, "batch.append");
    assert_eq!(append.args["patch_id"], "phase-2");
    assert_eq!(append.args["expected_revision"], 3);
    assert_eq!(append.args["invalidate_action_ids"][0], "fade-v1");
    assert_eq!(append.args["steps"][0]["action_id"], "fade-v2");
    control_protocol::validate_arguments(&append).unwrap();
    let invalidation = parse(&[
        "batch",
        "invalidate",
        "--id",
        "job-1",
        "--patch-id",
        "drop",
        "--expected-revision",
        "4",
        "--action-ids",
        r#"["fade-v2"]"#,
    ]);
    assert_eq!(invalidation.command, "batch.invalidate");
    control_protocol::validate_arguments(&invalidation).unwrap();
    let seal = parse(&[
        "batch",
        "seal",
        "--id",
        "job-1",
        "--patch-id",
        "done",
        "--expected-revision",
        "5",
        "--completion-event",
        "96",
    ]);
    assert_eq!(seal.command, "batch.seal");
    control_protocol::validate_arguments(&seal).unwrap();
    let wait = parse(&[
        "events",
        "wait",
        "--since",
        "42",
        "--job-id",
        "job-1",
        "--timeout",
        "60s",
    ]);
    assert_eq!(wait.command, "events.get");
    assert_eq!(wait.args["since"], 42);
    assert_eq!(wait.args["wait_ms"], 59900);
    assert_eq!(wait.args["job_id"], "job-1");
    control_protocol::validate_arguments(&wait).unwrap();
    let status = parse(&["jobs", "get", "job-1", "--detail"]);
    assert_eq!(status.args["detail"], true);
}
