//! TEST-ONLY event server: stands in for aiosd so the shell can be tested
//! before the real daemon exists. Every task gets the same four-step plan;
//! step 3 asks for approval. Never part of the product image.
//!
//!   aios-testkit-events <agent.sock> <approve.sock> [step_ms]

use protocol::{
    Ack, Call, Event, EventMsg, Request, Response, Risk, ServerMsg, SettingsResult, Status, Step,
    TaskStarted,
};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    clients: Vec<UnixStream>,
    cancelled: HashMap<String, bool>,
    approvals: HashMap<String, Sender<bool>>,
}

type Shared = Arc<Mutex<State>>;

fn send(state: &Shared, msg: &ServerMsg) {
    let Ok(mut line) = serde_json::to_string(msg) else {
        return;
    };
    line.push('\n');
    if let Ok(mut st) = state.lock() {
        st.clients
            .retain_mut(|c| c.write_all(line.as_bytes()).is_ok());
    }
}

fn run_task(state: Shared, task_id: String, step_ms: u64) {
    let epoch = Instant::now();
    let mut seq = 0;
    let emit = |seq: &mut u64, event: Event| {
        let msg = EventMsg {
            ts_ns: epoch.elapsed().as_nanos() as u64,
            task_id: task_id.clone(),
            seq: *seq,
            event,
        };
        *seq += 1;
        send(&state, &ServerMsg::Event(msg));
    };
    let cancelled = |st: &Shared| {
        st.lock()
            .map(|s| s.cancelled.contains_key(&task_id))
            .unwrap_or(true)
    };
    let titles = [
        "Open the browser",
        "Navigate to youtube.com",
        "Search for lofi for late night coding",
        "Show the results",
    ];
    std::thread::sleep(Duration::from_millis(step_ms));
    emit(
        &mut seq,
        Event::PlanCreated {
            steps: titles
                .iter()
                .enumerate()
                .map(|(i, t)| Step {
                    id: format!("s{}", i + 1),
                    title: t.to_string(),
                    action: "test.step".into(),
                    args: json!({}),
                })
                .collect(),
        },
    );
    for i in 1..=4 {
        if cancelled(&state) {
            return;
        }
        let step_id = format!("s{i}");
        emit(
            &mut seq,
            Event::StepStarted {
                step_id: step_id.clone(),
            },
        );
        if i == 3 {
            let (tx, rx) = channel();
            let approval_id = format!("a-{task_id}");
            if let Ok(mut st) = state.lock() {
                st.approvals.insert(approval_id.clone(), tx);
            }
            emit(
                &mut seq,
                Event::ApprovalNeeded {
                    approval_id,
                    step_id: step_id.clone(),
                    action: "app.launch".into(),
                    args: json!({"name": "browser"}),
                    risk: Risk::Ask,
                },
            );
            let allowed = loop {
                if cancelled(&state) {
                    return;
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(a) => break a,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return,
                }
            };
            if !allowed {
                emit(
                    &mut seq,
                    Event::StepFailed {
                        step_id,
                        reason: "denied by user".into(),
                    },
                );
                emit(
                    &mut seq,
                    Event::Error {
                        message: "You denied the action.".into(),
                    },
                );
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(step_ms));
        if cancelled(&state) {
            return;
        }
        emit(
            &mut seq,
            Event::StepDone {
                step_id,
                summary: String::new(),
            },
        );
    }
    emit(
        &mut seq,
        Event::Result {
            summary: "Test plan finished.".into(),
        },
    );
}

fn handle(state: Shared, stream: UnixStream, register: bool, step_ms: u64) {
    if register {
        if let (Ok(c), Ok(mut st)) = (stream.try_clone(), state.lock()) {
            st.clients.push(c);
        }
    }
    let Ok(mut reply) = stream.try_clone() else {
        return;
    };
    let mut next = 0u64;
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        let Ok(req) = serde_json::from_str::<Request>(&line) else {
            continue;
        };
        let resp = match req.call {
            Call::TaskStart { .. } => {
                next += 1;
                let task_id = format!("test-{}-{next}", std::process::id());
                let st = state.clone();
                let id = task_id.clone();
                std::thread::spawn(move || run_task(st, id, step_ms));
                Response::ok(req.id, TaskStarted { task_id })
            }
            Call::TaskCancel { task_id } => {
                if let Ok(mut st) = state.lock() {
                    st.cancelled.insert(task_id.clone(), true);
                }
                let msg = EventMsg {
                    ts_ns: 0,
                    task_id,
                    seq: 999,
                    event: Event::Cancelled {},
                };
                send(&state, &ServerMsg::Event(msg));
                Response::ok(req.id, Ack { ok: true })
            }
            Call::ApprovalRespond {
                approval_id, allow, ..
            } => {
                let tx = state
                    .lock()
                    .ok()
                    .and_then(|mut s| s.approvals.remove(&approval_id));
                let ok = tx.map(|t| t.send(allow).is_ok()).unwrap_or(false);
                Response::ok(req.id, Ack { ok })
            }
            Call::StatusGet {} => Response::ok(
                req.id,
                Status {
                    ai_configured: true,
                    model: Some("test-event-server".into()),
                },
            ),
            Call::SettingsAiSet { .. } => Response::ok(
                req.id,
                SettingsResult {
                    ok: false,
                    message: "the test event server does not store settings".into(),
                },
            ),
        };
        let Ok(mut out) = serde_json::to_string(&ServerMsg::Response(resp)) else {
            continue;
        };
        out.push('\n');
        if reply.write_all(out.as_bytes()).is_err() {
            break;
        }
    }
}

fn listen(path: &str, state: Shared, register: bool, step_ms: u64) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let st = state.clone();
            std::thread::spawn(move || handle(st, s, register, step_ms));
        }
    });
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: aios-testkit-events <agent.sock> <approve.sock> [step_ms]");
        std::process::exit(2);
    }
    let step_ms = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1500);
    let state: Shared = Arc::default();
    listen(&args[1], state.clone(), true, step_ms)?;
    listen(&args[2], state, false, step_ms)?;
    eprintln!(
        "aios-testkit-events: TEST-ONLY event server on {} and {}",
        args[1], args[2]
    );
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
