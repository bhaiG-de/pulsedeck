use control_protocol::{Request, Response};
use std::io::{BufRead, Write};
fn print(response: &Response, json: bool) {
    if json {
        println!("{}", serde_json::to_string(response).unwrap());
    } else if response.ok {
        println!(
            "{}",
            serde_json::to_string_pretty(&response.result).unwrap()
        );
    } else if let Some(error) = &response.error {
        eprintln!("{error}");
    }
    let _ = std::io::stdout().flush();
}
fn main() {
    let tokens: Vec<String> = std::env::args().skip(1).collect();
    if tokens.is_empty() || tokens.iter().any(|t| matches!(t.as_str(), "--help" | "-h")) {
        print!("{}", pulsedeck_cli::HELP);
        return;
    }
    if tokens == ["--version"] {
        println!(
            "pulsedeckctl {} protocol {}",
            env!("CARGO_PKG_VERSION"),
            control_protocol::VERSION
        );
        return;
    }
    if tokens.first().map(String::as_str) == Some("session") {
        let socket = match pulsedeck_cli::parse_session(&tokens) {
            Ok(socket) => socket,
            Err(e) => {
                print(&Response::failure("", e.clone()), true);
                std::process::exit(e.exit_code());
            }
        };
        for line in std::io::stdin().lock().lines() {
            let response = match line {
                Ok(line) => match serde_json::from_str::<Request>(&line) {
                    Ok(r) => control_protocol::ipc::call(&socket, &r)
                        .unwrap_or_else(|e| Response::failure(r.request_id, e)),
                    Err(e) => Response::failure(
                        "",
                        control_protocol::ControlError::new("invalid_request", e.to_string()),
                    ),
                },
                Err(_) => break,
            };
            print(&response, true);
        }
        return;
    }
    let invocation = match pulsedeck_cli::parse(&tokens) {
        Ok(v) => v,
        Err(e) => {
            let json = tokens.iter().any(|v| v == "--json" || v == "--json=true");
            print(&Response::failure("", e.clone()), json);
            std::process::exit(e.exit_code());
        }
    };
    let mut request = invocation.request;
    let mut iteration = 0;
    let started = std::time::Instant::now();
    loop {
        let response = control_protocol::ipc::call(&invocation.socket, &request)
            .unwrap_or_else(|e| Response::failure(request.request_id.clone(), e));
        print(&response, invocation.json || invocation.watch);
        if let Some(error) = response.error {
            std::process::exit(error.exit_code());
        }
        iteration += 1;
        let job_done = request.command == "jobs.get"
            && response.result.as_ref().is_some_and(|r| {
                matches!(
                    r["state"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                )
            });
        if !invocation.watch || job_done || invocation.count.is_some_and(|n| iteration >= n) {
            if job_done
                && invocation.watch
                && response
                    .result
                    .as_ref()
                    .is_some_and(|r| r["state"] == "failed")
            {
                std::process::exit(5);
            }
            if job_done
                && invocation.watch
                && response
                    .result
                    .as_ref()
                    .is_some_and(|r| r["state"] == "cancelled")
            {
                std::process::exit(7);
            }
            break;
        }
        if request.command == "jobs.get"
            && started.elapsed().as_millis() >= request.timeout_ms as u128
        {
            std::process::exit(6);
        }
        if request.command == "events.get" {
            if let Some(result) = response.result {
                request.args["since"] = result["sequence"].clone();
            }
        }
        request.request_id = Request::new("", serde_json::json!({})).request_id;
        std::thread::sleep(std::time::Duration::from_millis(invocation.interval_ms));
    }
}
