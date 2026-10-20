//! A single invocation submits a move and waits on lifecycle events, without transport polling.
use control_protocol::{ipc::call, ControlError, Request, Response};
use serde_json::{json, Value};
use std::{io::Write, path::Path};

fn terminal_result(summary: &Value) -> Option<Result<(), ControlError>> {
    match summary["state"].as_str()? {
        "completed" => Some(Ok(())),
        "recovered" | "failed" | "cancelled" | "yielded" | "interrupted" | "invalidated" => {
            Some(Err(ControlError::new(
                if summary["state"] == "cancelled" {
                    "cancelled"
                } else {
                    "move_incomplete"
                },
                summary["error"]["message"]
                    .as_str()
                    .unwrap_or("Move ended without completing the handover"),
            )
            .with_detail("operation_id", summary["operation_id"].clone())))
        }
        _ => None,
    }
}
fn emit(value: &Value) {
    println!("{value}");
    let _ = std::io::stdout().flush();
}
pub fn follow(socket: &Path, accepted: &Response) -> Result<(), ControlError> {
    let summary = accepted
        .result
        .as_ref()
        .ok_or_else(|| ControlError::new("protocol_error", "Missing move receipt"))?;
    if summary["queue_id"].is_string() {
        return follow_queue(socket, summary, false);
    }
    let operation = summary["operation_id"]
        .as_str()
        .ok_or_else(|| ControlError::new("protocol_error", "Missing operation_id"))?;
    if let Some(result) = terminal_result(summary) {
        return result;
    }
    let mut cursor = summary["event_sequence"].as_u64().unwrap_or(0);
    loop {
        let mut request = Request::new(
            "events.get",
            json!({"since":cursor,"operation_id":operation,"wait_ms":60000}),
        );
        request.timeout_ms = 61000;
        let response =
            call(socket, &request).map_err(|e| e.with_detail("operation_id", operation))?;
        if let Some(error) = response.error {
            return Err(error.with_detail("operation_id", operation));
        }
        let result = response.result.unwrap_or_default();
        if result["resync_required"] == true {
            let response = call(
                socket,
                &Request::new("mix.status", json!({"operation_id":operation})),
            )?;
            if let Some(error) = response.error {
                return Err(error);
            }
            let summary = response.result.unwrap_or_default();
            emit(&json!({"type":"mix.resync","data":summary}));
            if let Some(result) = terminal_result(&summary) {
                return result;
            }
            cursor = summary["event_sequence"].as_u64().unwrap_or(0);
            continue;
        }
        for event in result["events"].as_array().into_iter().flatten() {
            if event["type"] != "mix.updated" || event["data"]["operation_id"] != operation {
                continue;
            }
            emit(event);
            if let Some(result) = terminal_result(&event["data"]) {
                return result;
            }
        }
        cursor = result["sequence"].as_u64().unwrap_or(cursor);
    }
}

pub fn wait_ready(socket: &Path, accepted: &Response) -> Result<(), ControlError> {
    let summary = accepted
        .result
        .as_ref()
        .filter(|r| r["queue_id"].is_string())
        .ok_or_else(|| ControlError::new("protocol_error", "Missing queue receipt"))?;
    follow_queue(socket, summary, true)
}

fn queue_ready(summary: &Value) -> bool {
    summary["readiness"] == "ready"
        || summary["completed_moves"].as_u64().is_some_and(|n| n > 0)
        || summary["items"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(std::iter::once(&summary["item"]))
            .any(|item| {
                matches!(
                    item["state"].as_str(),
                    Some("armed" | "transitioning" | "completed")
                )
            })
}

fn follow_queue(socket: &Path, initial: &Value, until_ready: bool) -> Result<(), ControlError> {
    let queue = initial["queue_id"].as_str().unwrap();
    let mut summary = initial.clone();
    let mut cursor = summary["event_sequence"].as_u64().unwrap_or(0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    loop {
        if summary["terminal"] == true {
            return if summary["state"] == "completed" && (!until_ready || queue_ready(&summary)) {
                Ok(())
            } else {
                Err(ControlError::new(
                    "queue_incomplete",
                    summary["reason"]
                        .as_str()
                        .unwrap_or("Successor queue ended before completion"),
                )
                .with_detail("queue_id", queue))
            };
        }
        if until_ready && queue_ready(&summary) {
            return Ok(());
        }
        let wait_ms = if until_ready {
            let remaining = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis() as u64;
            if remaining == 0 {
                return Err(ControlError::new(
                    "timeout",
                    "First successor is not armed; inspect the existing queue before retrying",
                )
                .with_detail("queue_id", queue));
            }
            remaining.min(30_000)
        } else {
            60_000
        };
        let mut request = Request::new(
            "events.get",
            json!({"since":cursor,"queue_id":queue,"wait_ms":wait_ms}),
        );
        request.timeout_ms = wait_ms + 1000;
        let response = call(socket, &request)?;
        if let Some(error) = response.error {
            return Err(error.with_detail("queue_id", queue));
        }
        let result = response.result.unwrap_or_default();
        if result["resync_required"] == true {
            let response = call(
                socket,
                &Request::new("mix.queue.status", json!({"queue_id":queue})),
            )?;
            if let Some(error) = response.error {
                return Err(error.with_detail("queue_id", queue));
            }
            summary = response.result.unwrap_or_default();
            emit(&json!({"type":"mix.queue.resync","data":summary}));
            cursor = summary["event_sequence"].as_u64().unwrap_or(0);
            continue;
        }
        for event in result["events"].as_array().into_iter().flatten() {
            if event["type"] == "mix.queue.updated" && event["data"]["queue_id"] == queue {
                emit(event);
                summary = event["data"].clone();
                if summary["terminal"] == true {
                    break;
                }
            }
        }
        cursor = result["sequence"].as_u64().unwrap_or(cursor);
    }
}
