//! Control transport fixtures exercise receipts/events, never substitute for music/audio checks.
#[cfg(unix)]
#[test]
fn one_invocation_executes_once_and_receives_progress_without_deck_polling() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};
    let root = std::env::temp_dir().join(format!("pd-move-stream-{}", std::process::id()));
    let socket = root.join("control.sock");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let requests = seen.clone();
    let server = Server::start(socket.clone(), move |r| {
        requests.lock().unwrap().push(r.clone());
        let result = match r.command.as_str() {
            "mix.execute" => json!({"operation_id":"move-1","state":"preparing","event_sequence":8}),
            "events.get" if r.args["since"] == 8 => json!({"events":[
                {"sequence":9,"type":"mix.updated","data":{"operation_id":"move-1","state":"ready"}},
                {"sequence":10,"type":"mix.updated","data":{"operation_id":"move-1","state":"arming"}},
                {"sequence":11,"type":"mix.updated","data":{"operation_id":"move-1","state":"armed"}}
            ],"sequence":11,"resync_required":false}),
            "events.get" if r.args["since"] == 11 => json!({"events":[
                {"sequence":12,"type":"mix.updated","data":{"operation_id":"move-1","state":"transitioning"}},
                {"sequence":13,"type":"mix.updated","data":{"operation_id":"move-1","state":"completed"}}
            ],"sequence":13,"resync_required":false}),
            _ => panic!("Unexpected client call {}", r.command),
        };
        Response::success(&r, "accepted", result)
    }).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "mix",
            "execute",
            "--option-id",
            "option-1",
            "--review-confirmed",
            "--follow",
            "--request-id",
            "execute-once",
            "--socket",
            socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let values: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(values.len(), 6);
    assert_eq!(values.last().unwrap()["data"]["state"], "completed");
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.iter().filter(|r| r.command == "mix.execute").count(),
        1
    );
    assert_eq!(seen[0].request_id, "execute-once");
    assert_eq!(
        seen[0].args,
        json!({"option_id":"option-1","review_confirmed":true})
    );
    for r in &seen[1..] {
        assert_eq!(r.command, "events.get");
        assert_eq!(r.args["operation_id"], "move-1");
        assert_eq!(r.args["wait_ms"], 60000);
    }
    drop(seen);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn event_gap_resyncs_once_and_native_recovery_exits_as_incomplete() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    let root = std::env::temp_dir().join(format!("pd-move-resync-{}", std::process::id()));
    let socket = root.join("control.sock");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let server=Server::start(socket.clone(),move |r| {
        seen.lock().unwrap().push(r.command.clone());
        let value=match r.command.as_str() {
            "mix.execute"=>json!({"operation_id":"move-2","state":"preparing","event_sequence":1}),
            "events.get" if r.args["since"]==1=>json!({"events":[],"sequence":900,"resync_required":true}),
            "mix.status"=>json!({"operation_id":"move-2","state":"armed","event_sequence":900}),
            "events.get" if r.args["since"]==900=>json!({"events":[{"sequence":901,"type":"mix.updated","data":{"operation_id":"move-2","state":"recovered","error":{"message":"Incoming launch failed; outgoing restored"}}}],"sequence":901}),
            _=>panic!("Unexpected request: {r:?}"),
        };Response::success(&r,"applied",value)
    }).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "call",
            "mix.execute",
            "--args",
            r#"{"option_id":"option","review_confirmed":true}"#,
            "--follow",
            "--socket",
            socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("mix.resync"));
    assert!(stdout.contains("move_incomplete"));
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["mix.execute", "events.get", "mix.status", "events.get"]
    );
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn successor_queue_submits_once_and_follows_queue_events_until_completion() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    let root = std::env::temp_dir().join(format!("pd-successor-{}", std::process::id()));
    let socket = root.join("control.sock");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    let server=Server::start(socket.clone(),move|r|{
        seen.lock().unwrap().push(r.clone());
        let result=match r.command.as_str(){
            "mix.queue.start"=>json!({"queue_id":"queue-1","state":"running","terminal":false,"event_sequence":1}),
            "events.get"=>{
                assert_eq!(r.args["queue_id"],"queue-1");
                json!({"sequence":3,"events":[{"type":"mix.queue.updated","sequence":2,"data":{"queue_id":"queue-1","state":"running","terminal":false,"completed_moves":1}},{"type":"mix.queue.updated","sequence":3,"data":{"queue_id":"queue-1","state":"completed","terminal":true,"completed_moves":2}}]})
            },_=>panic!("Unexpected queue command {}",r.command)
        };Response::success(&r,"accepted",result)
    }).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "mix",
            "queue",
            "start",
            "--playlist",
            "spotify:liked",
            "--provider",
            "spotify",
            "--duration-seconds",
            "3600",
            "--review-confirmed",
            "--replace-prepared",
            "--follow",
            "--socket",
            socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].command, "mix.queue.start");
    assert_eq!(calls[0].args["review_confirmed"], true);
    assert_eq!(calls[1].command, "events.get");
    drop(calls);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn detached_queue_returns_without_opening_an_event_follower() {
    use control_protocol::{ipc::Server, Response};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    let root = std::env::temp_dir().join(format!("pd-detached-{}", std::process::id()));
    let socket = root.join("control.sock");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let calls = seen.clone();
    let server = Server::start(socket.clone(), move |r| {
        calls.lock().unwrap().push(r.command.clone());
        assert_eq!(r.command, "mix.queue.start");
        Response::success(
            &r,
            "accepted",
            json!({"queue_id":"owned","state":"running","terminal":false,
            "handoff":{"owner":"application","agent_can_finish":true}}),
        )
    })
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "mix",
            "queue",
            "start",
            "--playlist",
            "spotify:liked",
            "--review-confirmed",
            "--socket",
            socket.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["result"]["handoff"]["agent_can_finish"], true);
    assert_eq!(*seen.lock().unwrap(), vec!["mix.queue.start"]);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}
