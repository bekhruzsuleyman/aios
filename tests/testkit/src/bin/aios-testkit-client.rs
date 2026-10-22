//! TEST-ONLY client for aiosd, run inside the test VM by tests/injection.
//!
//!   aios-testkit-client run <agent.sock> <text> [--cancel-after-ms N]
//!                           [--timeout-s S] [--forge-approve]
//!   aios-testkit-client connect <socket>
//!
//! `run` starts a task and prints every message as a JSON line, then one
//! `SUMMARY {...}` line. `--forge-approve` answers approval requests on the
//! agent socket (which aiosd must refuse). A task still waiting for an
//! approval at the timeout is cancelled. `connect` sends `status.get` and
//! reports whether the socket answered.

use protocol::{Call, Event, Request, ServerMsg};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

fn send(w: &mut UnixStream, id: u64, call: Call) -> std::io::Result<()> {
    let mut line = serde_json::to_string(&Request::new(id, call))?;
    line.push('\n');
    w.write_all(line.as_bytes())
}

fn run(sock: &str, text: &str, cancel_after: Option<u64>, timeout: u64, forge: bool) -> i32 {
    let mut s = match UnixStream::connect(sock) {
        Ok(s) => s,
        Err(e) => {
            println!("SUMMARY {}", json!({"error": format!("connect: {e}")}));
            return 2;
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    let mut w = s.try_clone().unwrap_or_else(|_| std::process::exit(2));
    let t0 = Instant::now();
    if send(&mut w, 1, Call::TaskStart { text: text.into() }).is_err() {
        return 2;
    }
    let mut reader = BufReader::new(&mut s);
    let mut task_id = String::new();
    let mut cancel_sent: Option<Instant> = None;
    let mut forged = Vec::new();
    let mut approvals = 0;
    let mut steps_failed = 0;
    let mut terminal = serde_json::Value::Null;
    let mut cancel_ms = None;
    let mut next_id = 2;
    loop {
        let elapsed = t0.elapsed();
        if let (Some(ms), None) = (cancel_after, cancel_sent) {
            if elapsed >= Duration::from_millis(ms) && !task_id.is_empty() {
                let _ = send(
                    &mut w,
                    next_id,
                    Call::TaskCancel {
                        task_id: task_id.clone(),
                    },
                );
                next_id += 1;
                cancel_sent = Some(Instant::now());
            }
        }
        if elapsed > Duration::from_secs(timeout) && cancel_sent.is_none() && !task_id.is_empty() {
            let _ = send(
                &mut w,
                next_id,
                Call::TaskCancel {
                    task_id: task_id.clone(),
                },
            );
            next_id += 1;
            cancel_sent = Some(Instant::now());
        }
        if elapsed > Duration::from_secs(timeout + 30) {
            terminal = json!("timeout");
            break;
        }
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => continue,
        }
        print!("{line}");
        let Ok(msg) = serde_json::from_str::<ServerMsg>(&line) else {
            continue;
        };
        match msg {
            ServerMsg::Response(r) => {
                if r.id == json!(1) {
                    if let Some(id) = r.result.as_ref().and_then(|v| v["task_id"].as_str()) {
                        task_id = id.to_string();
                    }
                } else if r.id.as_u64().is_some_and(|i| i >= 1000) {
                    forged.push(json!({"accepted": r.result.is_some(), "error": r.error.map(|e| e.message)}));
                }
            }
            ServerMsg::Event(e) if e.task_id == task_id => match e.event {
                Event::ApprovalNeeded { approval_id, .. } => {
                    approvals += 1;
                    if forge {
                        let call = Call::ApprovalRespond {
                            task_id: task_id.clone(),
                            approval_id,
                            allow: true,
                        };
                        let _ = send(&mut w, 1000 + approvals, call);
                    }
                }
                Event::StepFailed { .. } => steps_failed += 1,
                ev if ev.is_terminal() => {
                    if let (Some(t), Event::Cancelled {}) = (cancel_sent, &ev) {
                        cancel_ms = Some(t.elapsed().as_secs_f64() * 1000.0);
                    }
                    terminal = serde_json::to_value(&ev).unwrap_or_default();
                    break;
                }
                _ => {}
            },
            ServerMsg::Event(_) => {}
        }
    }
    // Let forged-approval answers arrive.
    let deadline = Instant::now() + Duration::from_secs(2);
    while forge && forged.len() < approvals as usize && Instant::now() < deadline {
        let mut line = String::new();
        if let Ok(n) = reader.read_line(&mut line) {
            if n == 0 {
                break;
            }
            if let Ok(ServerMsg::Response(r)) = serde_json::from_str::<ServerMsg>(&line) {
                if r.id.as_u64().is_some_and(|i| i >= 1000) {
                    forged.push(json!({"accepted": r.result.is_some(), "error": r.error.map(|e| e.message)}));
                }
            }
        }
    }
    println!(
        "SUMMARY {}",
        json!({
            "task_id": task_id,
            "terminal": terminal,
            "approvals_requested": approvals,
            "forged_approvals": forged,
            "steps_failed": steps_failed,
            "cancel_latency_ms": cancel_ms,
            "total_ms": t0.elapsed().as_secs_f64() * 1000.0,
        })
    );
    0
}

fn connect(path: &str) -> i32 {
    let mut s = match UnixStream::connect(path) {
        Ok(s) => s,
        Err(e) => {
            println!("refused: {e}");
            return 1;
        }
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    if let Err(e) = send(&mut s, 1, Call::StatusGet {}) {
        println!("closed: {e}");
        return 1;
    }
    let mut line = String::new();
    match BufReader::new(s).read_line(&mut line) {
        Ok(n) if n > 0 => {
            println!("answered: {}", line.trim());
            0
        }
        Ok(_) => {
            println!("closed without answer");
            1
        }
        Err(e) => {
            println!("no answer: {e}");
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name);
    let num = |name: &str| {
        flag(name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
    };
    let code = match args.get(1).map(String::as_str) {
        Some("run") if args.len() >= 4 => run(
            &args[2],
            &args[3],
            num("--cancel-after-ms"),
            num("--timeout-s").unwrap_or(60),
            flag("--forge-approve").is_some(),
        ),
        Some("connect") if args.len() >= 3 => connect(&args[2]),
        _ => {
            eprintln!("usage: aios-testkit-client run <sock> <text> [--cancel-after-ms N] [--timeout-s S] [--forge-approve] | connect <sock>");
            2
        }
    };
    std::process::exit(code);
}
