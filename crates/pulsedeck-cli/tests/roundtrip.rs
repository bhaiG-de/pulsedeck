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
