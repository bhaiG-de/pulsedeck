#![cfg(unix)]
use control_protocol::{ipc::Server, ControlError, Request, Response};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Queue {
    revision: u64,
    sequence: u64,
    events: Vec<Value>,
    calls: Vec<Request>,
    cancelled: bool,
    conflict_once: bool,
}
fn event(queue: &mut Queue, action: &str, state: &str) {
    queue.sequence += 1;
    queue.events.push(json!({"sequence":queue.sequence,"type":"job.action","data":{"id":"owned","action_id":action,"state":state,"revision":queue.revision}}));
}
fn receive(rx: &mpsc::Receiver<Value>, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let value = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("stream stalled");
        if predicate(&value) {
            return value;
        }
    }
}
fn finish(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("session did not close");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn persistent_session_registers_edits_and_receives_completion_without_more_input() {
    let root = std::env::temp_dir().join(format!("pd-duplex-{}", std::process::id()));
    let socket = root.join("control.sock");
    let state = Arc::new((
        Mutex::new(Queue {
            revision: 1,
            conflict_once: true,
            ..Queue::default()
        }),
        Condvar::new(),
    ));
    let server_state = state.clone();
    let server = Server::start(socket.clone(),move |r| {
        let (mutex,ready) = &*server_state;
        let mut state = mutex.lock().unwrap();
        state.calls.push(r.clone());
        let result = match r.command.as_str() {
            "batch.run" => {
                assert_eq!(r.args["steps"],json!([]));
                assert_eq!(r.args["open"],true);
                json!({"job_id":"owned","revision":1,"event_sequence":0})
            }
            "batch.append"|"batch.invalidate" => {
                if state.conflict_once {
                    state.conflict_once = false;state.revision = 2;
                    return Response::failure(&r.request_id,ControlError::new("revision_conflict","concurrent change").with_detail("current_revision",2));
                }
                assert_eq!(r.args["expected_revision"],state.revision);
                state.revision += 1;
                json!({"job_id":"owned","revision":state.revision,"event_sequence":state.sequence})
            }
            "events.get" => {
                let since = r.args["since"].as_u64().unwrap();
                if state.sequence <= since && r.args["wait_ms"].as_u64().unwrap()>0 {
                    state = ready.wait_timeout(state,Duration::from_millis(300)).unwrap().0;
                }
                json!({"sequence":state.sequence,"events":state.events.iter().filter(|e|e["sequence"].as_u64().unwrap()>since).collect::<Vec<_>>()})
            }
            "jobs.cancel" => {
                assert_eq!(r.args["id"],"owned");
                state.cancelled = true;
                event(&mut state,"future@2","cancelled");ready.notify_all();
                json!({"state":"cancelled"})
            }
            _ => panic!("Unexpected command {}",r.command),
        };
        Response::success(&r,"applied",result)
    }).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "session",
            "start",
            "--stdio",
            "--socket",
            socket.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            tx.send(serde_json::from_str::<Value>(&line).expect("stdout must contain only JSONL"))
                .unwrap();
        }
    });
    let ready = receive(&rx, |v| v["type"] == "session_ready");
    assert_eq!(ready["job_id"], "owned");
    assert!(ready.get("commands").is_none());
    let commands = json!([
        {"command":"automation.ramp","args":{"target":{"type":"channel_fader","deck":"B"},"value":1,"seconds":2}},
        {"command":"automation.ramp","args":{"target":{"type":"eq_high","deck":"B"},"value":1,"seconds":4}}
    ]);
    let initial = json!({"id":"initial","commands":commands,"when":{"type":"after_beats","clock":"A","beats":16},"request_id":"group-once"});
    writeln!(input, "{initial}").unwrap();
    input.flush().unwrap();
    let registered = receive(&rx, |v| v["type"] == "registered");
    assert_eq!(registered["ok"], true);
    assert_eq!(registered["item_id"], "initial");
    assert_eq!(registered["version"], 1);
    assert_eq!(registered["commands"], commands);
    assert_eq!(registered["member_count"], 2);
    assert!(registered.get("command").is_none());
    writeln!(input, "{initial}").unwrap();
    input.flush().unwrap();
    let duplicate = receive(&rx, |v| v["type"] == "registered");
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["replayed"], false);
    // A partial group remains running. The adapter forwards backend member receipts;
    // it neither serializes members nor fabricates group completion.
    {
        let (mutex, ready) = &*state;
        let mut queue = mutex.lock().unwrap();
        event(&mut queue, "initial", "running");
        queue.events.last_mut().unwrap()["data"]["members"] = json!([
            {"member_id":"initial:0","state":"executed","receipt":{"completed_frame":100}},
            {"member_id":"initial:1","state":"running"}
        ]);
        ready.notify_all();
    }
    let running = receive(&rx, |v| v["data"]["state"] == "running");
    assert_eq!(running["data"]["members"][0]["state"], "executed");
    assert_eq!(running["data"]["members"][1]["state"], "running");
    // Keep stdin open and send nothing. The software's event wakes the running CLI.
    {
        let (mutex, ready) = &*state;
        let mut queue = mutex.lock().unwrap();
        event(&mut queue, "initial", "executed");
        queue.events.last_mut().unwrap()["data"]["members"] = json!([
            {"member_id":"initial:0","state":"executed","receipt":{"completed_frame":100}},
            {"member_id":"initial:1","state":"executed","receipt":{"completed_frame":200}}
        ]);
        ready.notify_all();
    }
    let executed = receive(&rx, |v| v["data"]["state"] == "executed");
    assert_eq!(executed["data"]["item_id"], "initial");
    assert_eq!(executed["data"]["version"], 1);
    assert_eq!(
        executed["data"]["members"][1]["receipt"]["completed_frame"],
        200
    );
    for bad in [
        json!({"id":"bad-empty","commands":[]}),
        json!({"id":"bad-ambiguous","command":"engine.command","commands":commands}),
        json!({"id":"bad-provider","commands":[{"command":"deck.load","args":{"deck":"B"}},{"command":"engine.command","args":{}}]}),
    ] {
        writeln!(input, "{bad}").unwrap();
        input.flush().unwrap();
        assert_eq!(
            receive(&rx, |v| v["ok"] == false)["error"]["code"],
            "invalid_request"
        );
    }
    writeln!(input,"{}",json!({"op":"add","id":"future","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"initial","type":"after_seconds","seconds":30}})).unwrap();
    input.flush().unwrap();
    assert_eq!(receive(&rx, |v| v["type"] == "registered")["ok"], true);
    writeln!(input, "not-json").unwrap();
    input.flush().unwrap();
    assert_eq!(
        receive(&rx, |v| v["ok"] == false)["error"]["code"],
        "invalid_request"
    );
    let update = json!({"op":"update","id":"future","request_id":"edit-once","commands":commands,"when":{"type":"after_seconds","seconds":30}});
    writeln!(input, "{update}").unwrap();
    input.flush().unwrap();
    let updated = receive(&rx, |v| v["type"] == "updated");
    assert_eq!(updated["ok"], true);
    assert_eq!(updated["action_id"], "future@2");
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["commands"], commands);
    assert_eq!(updated["member_count"], 2);
    writeln!(input, "{update}").unwrap();
    input.flush().unwrap();
    let duplicate = receive(&rx, |v| v["type"] == "updated");
    assert_eq!(duplicate["ok"], true);
    assert_eq!(duplicate["action_id"], "future@2");
    assert_eq!(duplicate["version"], 2);
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["replayed"], false);
    let mut changed = update.clone();
    changed["commands"][0]["args"]["value"] = json!(0.5);
    writeln!(input, "{changed}").unwrap();
    input.flush().unwrap();
    assert_eq!(
        receive(&rx, |v| v["ok"] == false)["error"]["code"],
        "idempotency_conflict"
    );
    writeln!(input,"{}",json!({"op":"add","id":"future","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"initial","type":"after_seconds","seconds":30}})).unwrap();
    input.flush().unwrap();
    let old_registration = receive(&rx, |v| v["type"] == "registered");
    assert_eq!(old_registration["duplicate"], true);
    assert_eq!(old_registration["version"], 1);
    writeln!(input,"{}",json!({"op":"add","id":"following","command":"engine.command","args":{"type":"pause","deck":"A"},"when":{"after":"future"}})).unwrap();
    input.flush().unwrap();
    assert_eq!(receive(&rx, |v| v["type"] == "registered")["ok"], true);
    writeln!(input, "{}", json!({"op":"remove","id":"future"})).unwrap();
    input.flush().unwrap();
    assert_eq!(receive(&rx, |v| v["type"] == "removed")["ok"], true);
    drop(input); // Redirected stdin EOF cancels only the queue this process created.
    receive(&rx, |v| v["type"] == "session_closed");
    finish(&mut child);
    reader.join().unwrap();
    let queue = state.0.lock().unwrap();
    assert!(queue.cancelled);
    assert!(!queue
        .calls
        .iter()
        .any(|r| r.command == "jobs.get" || r.command == "automation.stop"));
    let appends: Vec<_> = queue
        .calls
        .iter()
        .filter(|r| r.command == "batch.append")
        .collect();
    assert_eq!(appends[1].args["steps"][0]["commands"], commands);
    assert!(appends[1].args["steps"][0].get("command").is_none());
    assert_eq!(appends[3].args["steps"][0]["commands"], commands);
    assert_eq!(appends.len(), 5); // Conflict, success, one update, following action; duplicate never replays.
    assert_eq!(appends[3].args["invalidate_action_ids"], json!(["future"]));
    assert_eq!(
        appends[3].args["replacements"],
        json!({"future":"future@2"})
    );
    assert!(appends[1].args.get("replacements").is_none());
    assert_eq!(appends[4].args["steps"][0]["when"]["after"], "future@2");
    assert_eq!(
        queue
            .calls
            .iter()
            .find(|r| r.command == "batch.invalidate")
            .unwrap()
            .args["action_ids"],
        json!(["future@2"])
    );
    drop(queue);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_stop_and_departed_server_close_even_while_stdin_remains_open() {
    for departed in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "pd-session-close-{}-{departed}",
            std::process::id()
        ));
        let socket = root.join("control.sock");
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let capture = cancelled.clone();
        let server = Server::start(socket.clone(), move |r| {
            let result = match r.command.as_str() {
                "batch.run" => json!({"job_id":"owned","revision":1,"event_sequence":0}),
                "batch.append" => {
                    assert!(r.args["steps"][0]["action_id"]
                        .as_str()
                        .unwrap()
                        .starts_with("item-"));
                    json!({"revision":2})
                }
                "events.get" => {
                    std::thread::sleep(Duration::from_millis(30));
                    json!({"events":[],"sequence":0})
                }
                "jobs.cancel" => {
                    assert_eq!(r.args["id"], "owned");
                    capture.store(true, std::sync::atomic::Ordering::Release);
                    json!({"state":"cancelled"})
                }
                _ => panic!("Unexpected request {}", r.command),
            };
            Response::success(&r, "applied", result)
        })
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
            .args([
                "--socket",
                socket.to_str().unwrap(),
                "session",
                "start",
                "--stdio",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["type"],
            "session_ready"
        );
        if departed {
            drop(server);
        } else {
            writeln!(
                input,
                "{}",
                json!({"command":"engine.command","args":{"type":"play","deck":"B"}})
            )
            .unwrap();
            writeln!(input, "{}", json!({"op":"stop"})).unwrap();
            input.flush().unwrap();
            finish(&mut child);
            assert!(cancelled.load(std::sync::atomic::Ordering::Acquire));
            drop(server);
        }
        finish(&mut child); // stdin is deliberately still open.
        let values: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        let closed = values
            .iter()
            .find(|v| v["type"] == "session_closed")
            .unwrap();
        assert_eq!(
            closed["reason"],
            if departed { "connection_lost" } else { "stop" }
        );
        if !departed {
            let registered = values.iter().find(|v| v["type"] == "registered").unwrap();
            assert!(registered["item_id"].as_str().unwrap().starts_with("item-"));
            assert_eq!(registered["command"], "engine.command");
        }
        drop(input);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn atomic_append_and_rejected_reuse_require_acknowledged_dependency_versions() {
    let root = std::env::temp_dir().join(format!("pd-session-atomic-{}", std::process::id()));
    let socket = root.join("control.sock");
    let accepted = Arc::new(Mutex::new(Vec::<Value>::new()));
    let calls = Arc::new(Mutex::new(Vec::<Request>::new()));
    let accepted_server = accepted.clone();
    let calls_server = calls.clone();
    let server = Server::start(socket.clone(), move |r| {
        calls_server.lock().unwrap().push(r.clone());
        let result = match r.command.as_str() {
            "batch.run" => json!({"job_id":"owned","revision":1,"event_sequence":0}),
            "batch.append" => {
                let steps = r.args["steps"].as_array().unwrap();
                let mut accepted = accepted_server.lock().unwrap();
                if steps.iter().any(|step| step["command"] == "reject") {
                    return Response::failure(
                        &r.request_id,
                        ControlError::new("invalid_arguments", "The whole append was rejected"),
                    );
                }
                if steps.iter().any(|step| {
                    accepted
                        .iter()
                        .any(|old| old["action_id"] == step["action_id"])
                }) {
                    return Response::failure(
                        &r.request_id,
                        ControlError::new("action_id_reused", "ID already accepted"),
                    );
                }
                accepted.extend(steps.iter().cloned());
                json!({"revision":2})
            }
            "jobs.get" => json!({"actions":accepted_server.lock().unwrap().clone(),"revision":2}),
            "jobs.cancel" => json!({"state":"cancelled"}),
            "events.get" => {
                std::thread::sleep(Duration::from_millis(20));
                json!({"events":[],"sequence":0})
            }
            _ => panic!("Unexpected request {}", r.command),
        };
        Response::success(&r, "applied", result)
    })
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pulsedeckctl"))
        .args([
            "session",
            "start",
            "--stdio",
            "--socket",
            socket.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            tx.send(serde_json::from_str::<Value>(&line.unwrap()).unwrap())
                .unwrap();
        }
    });
    receive(&rx, |v| v["type"] == "session_ready");
    let send = |input: &mut std::process::ChildStdin, value: &Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    };
    let plan = json!({"op":"add","request_id":"plan-once","items":[
        {"id":"pause-b","command":"engine.command","args":{"type":"pause","deck":"B"}},
        {"id":"load-b","command":"deck.load","args":{"deck":"B","uri":"spotify:track:fixture"},"when":{"after":"pause-b"}}
    ]});
    send(&mut input, &plan);
    let ack = receive(&rx, |v| v["type"] == "registered");
    assert_eq!(ack["atomic"], true);
    assert_eq!(ack["registered_count"], 2);
    assert_eq!(ack["items"][1]["item_id"], "load-b");
    assert_eq!(ack["items"][1]["version"], 1);
    send(&mut input, &plan);
    assert_eq!(
        receive(&rx, |v| v["type"] == "registered")["duplicate"],
        true
    );
    send(
        &mut input,
        &json!({"id":"pause-b","command":"engine.command","args":{"type":"play","deck":"B"},"request_id":"reuse-pause"}),
    );
    assert_eq!(
        receive(&rx, |v| v["type"] == "rejected")["error"]["code"],
        "action_id_reused"
    );
    let unsafe_after = json!({"id":"later","command":"engine.command","args":{"type":"play","deck":"B"},"when":{"after":"pause-b"}});
    send(&mut input, &unsafe_after);
    let rejected = receive(&rx, |v| v["type"] == "rejected");
    assert_eq!(rejected["error"]["code"], "dependency_ambiguous");
    assert_eq!(rejected["item_id"], "later");
    assert_eq!(
        rejected["error"]["details"]["accepted_versions"],
        json!([1])
    );
    send(&mut input, &json!({"op":"status"}));
    let status = receive(&rx, |v| v["type"] == "status");
    assert_eq!(
        status["dependency_ambiguities"],
        json!([{"item_id":"pause-b","accepted_versions":[1]}])
    );
    assert_eq!(status["result"]["actions"].as_array().unwrap().len(), 2);
    let mut pinned = unsafe_after.clone();
    pinned["when"]["after_version"] = json!(1);
    send(&mut input, &pinned);
    assert_eq!(
        receive(&rx, |v| v["type"] == "registered")["item_id"],
        "later"
    );
    let mut still_ambiguous = unsafe_after.clone();
    still_ambiguous["id"] = json!("another");
    send(&mut input, &still_ambiguous);
    assert_eq!(
        receive(&rx, |v| v["type"] == "rejected")["error"]["code"],
        "dependency_ambiguous"
    );
    send(
        &mut input,
        &json!({"op":"update","id":"pause-b","command":"engine.command","args":{"type":"pause","deck":"B"}}),
    );
    assert_eq!(receive(&rx, |v| v["type"] == "updated")["version"], 2);
    send(&mut input, &still_ambiguous);
    assert_eq!(
        receive(&rx, |v| v["type"] == "registered")["item_id"],
        "another"
    );
    let broken = json!({"op":"add","items":[{"id":"never-accepted","command":"status"},{"id":"invalid","command":"reject"}]});
    send(&mut input, &broken);
    let failed = receive(&rx, |v| v["type"] == "rejected");
    assert_eq!(failed["atomic"], true);
    assert_eq!(failed["registered_count"], 0);
    assert!(failed["items"][0].get("type").is_none());
    send(
        &mut input,
        &json!({"id":"cannot-follow","command":"status","when":{"after":"never-accepted","after_version":1}}),
    );
    assert_eq!(
        receive(&rx, |v| v["type"] == "rejected")["error"]["code"],
        "dependency_unknown"
    );
    send(
        &mut input,
        &json!({"id":"cannot-follow-either","command":"status","when":{"after":"never-accepted"}}),
    );
    assert_eq!(
        receive(&rx, |v| v["type"] == "rejected")["error"]["code"],
        "dependency_unknown"
    );
    send(&mut input, &json!({"op":"add","items":[]}));
    assert_eq!(
        receive(&rx, |v| v["type"] == "rejected")["registered_count"],
        0
    );
    send(&mut input, &json!({"op":"stop"}));
    receive(&rx, |v| v["type"] == "session_closed");
    finish(&mut child);
    reader.join().unwrap();
    let calls = calls.lock().unwrap();
    let patches: Vec<_> = calls
        .iter()
        .filter(|r| r.command == "batch.append")
        .collect();
    assert_eq!(patches.len(), 6); // Atomic registration, rejected reuse, explicit pin, update, new dependent, rejected atomic patch.
    assert_eq!(patches[0].args["steps"].as_array().unwrap().len(), 2);
    assert_eq!(
        patches[2].args["steps"][0]["when"],
        json!({"after":"pause-b"})
    );
    assert_eq!(patches[4].args["steps"][0]["when"]["after"], "pause-b@2");
    assert!(!accepted
        .lock()
        .unwrap()
        .iter()
        .any(|action| action["action_id"] == "never-accepted"));
    drop(calls);
    drop(server);
    std::fs::remove_dir_all(root).unwrap();
}
