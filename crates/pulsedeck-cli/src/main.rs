use control_protocol::{Request, Response};
use std::io::Write;
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
    if tokens.first().is_some_and(|s| s == "mcp") {
        let mut socket = control_protocol::socket_path();
        let mut index = 1;
        while index < tokens.len() {
            match tokens[index].as_str() {
                "--stdio" => {}
                "--socket" => {
                    index += 1;
                    let Some(path) = tokens.get(index) else {
                        eprintln!("--socket requires a path");
                        std::process::exit(2)
                    };
                    socket = path.into();
                }
                _ => {
                    eprintln!("Use pulsedeckctl mcp --stdio [--socket PATH]");
                    std::process::exit(2)
                }
            }
            index += 1;
        }
        if let Err(error) = pulsedeck_cli::mcp::serve(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            |request| control_protocol::ipc::call(&socket, &request),
        ) {
            eprintln!("MCP transport: {error}");
            std::process::exit(3)
        }
        return;
    }
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
    if pulsedeck_cli::is_session(&tokens) {
        let invocation = match pulsedeck_cli::parse_session(&tokens) {
            Ok(socket) => socket,
            Err(e) => {
                print(&Response::failure("", e.clone()), true);
                std::process::exit(e.exit_code());
            }
        };
        let result = pulsedeck_cli::session::run(invocation.socket, invocation.start);
        if let Err(error) = result {
            print(&Response::failure("", error.clone()), true);
            std::process::exit(error.exit_code());
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
        print(
            &response,
            invocation.json || invocation.watch || invocation.follow || invocation.wait_ready,
        );
        if let Some(error) = &response.error {
            std::process::exit(error.exit_code());
        }
        if invocation.wait_ready {
            if let Err(error) = pulsedeck_cli::mix::wait_ready(&invocation.socket, &response) {
                print(&Response::failure(&request.request_id, error.clone()), true);
                std::process::exit(error.exit_code());
            }
            break;
        }
        if invocation.follow {
            if let Err(error) = pulsedeck_cli::mix::follow(&invocation.socket, &response) {
                print(&Response::failure(&request.request_id, error.clone()), true);
                std::process::exit(error.exit_code());
            }
            break;
        }
        iteration += 1;
        let job_done = request.command == "jobs.get"
            && response.result.as_ref().is_some_and(|r| {
                matches!(
                    r["state"].as_str(),
                    Some(
                        "completed"
                            | "recovered"
                            | "failed"
                            | "cancelled"
                            | "yielded"
                            | "interrupted"
                    )
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
            if job_done
                && invocation.watch
                && response
                    .result
                    .as_ref()
                    .is_some_and(|r| matches!(r["state"].as_str(), Some("yielded" | "interrupted")))
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
