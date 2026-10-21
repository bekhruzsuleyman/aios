//! Task registry, event fan-out and request dispatch, shared by the Unix
//! socket server and the shell's WebSocket bridge.

use crate::agent::Agent;
use protocol::{Ack, Call, Event, EventMsg, Request, Response, TaskStarted, INVALID_REQUEST};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, oneshot};
use tokio::task::AbortHandle;
use tracer::{now_ns, Tracer};

const EVENT_BUFFER: usize = 4096;

/// Stamps and broadcasts the events of one task.
pub struct Emitter {
    task_id: String,
    /// Next seq, or `None` once a terminal event was sent: nothing may
    /// follow it (e.g. trace events from stage guards dropped by an abort).
    seq: Mutex<Option<u64>>,
    tx: broadcast::Sender<EventMsg>,
}

impl Emitter {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn emit(&self, event: Event) {
        // Holding the lock across send keeps seq order equal to send order.
        let Ok(mut guard) = self.seq.lock() else {
            return;
        };
        let Some(seq) = *guard else {
            return;
        };
        *guard = if event.is_terminal() {
            None
        } else {
            Some(seq + 1)
        };
        let msg = EventMsg {
            ts_ns: now_ns(),
            task_id: self.task_id.clone(),
            seq,
            event,
        };
        // No receivers is fine: nobody is listening right now.
        let _ = self.tx.send(msg);
    }
}

/// What an agent gets for one task.
pub struct TaskCtx {
    pub text: String,
    pub emitter: Arc<Emitter>,
    pub tracer: Arc<Tracer>,
}

struct TaskEntry {
    abort: AbortHandle,
    emitter: Arc<Emitter>,
    tracer: Arc<Tracer>,
}

pub struct Daemon {
    tx: broadcast::Sender<EventMsg>,
    tasks: Mutex<HashMap<String, TaskEntry>>,
    agent: Agent,
}

/// Returned with a `task.start` response. The task does not emit anything
/// until `go()` is called (or this is dropped), so callers can write the
/// response to the client before the first event.
pub struct StartGate(Option<oneshot::Sender<()>>);

impl StartGate {
    pub fn go(mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

impl Daemon {
    pub fn new(agent: Agent) -> Arc<Daemon> {
        let (tx, _) = broadcast::channel(EVENT_BUFFER);
        Arc::new(Daemon {
            tx,
            tasks: Mutex::new(HashMap::new()),
            agent,
        })
    }

    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventMsg> {
        self.tx.subscribe()
    }

    /// Dispatch one request. Must be called inside a tokio runtime.
    pub fn handle(self: &Arc<Self>, req: Request) -> (Response, Option<StartGate>) {
        match req.call {
            Call::TaskStart { text } => {
                if text.trim().is_empty() {
                    return (Response::err(req.id, INVALID_REQUEST, "empty text"), None);
                }
                let (task_id, gate) = self.start_task(text);
                (Response::ok(req.id, TaskStarted { task_id }), Some(gate))
            }
            Call::TaskCancel { task_id } => {
                let ok = self.cancel_task(&task_id);
                (Response::ok(req.id, Ack { ok }), None)
            }
            Call::ApprovalRespond {
                task_id,
                approval_id,
                allow,
            } => {
                let ok = self.agent.respond_approval(&task_id, &approval_id, allow);
                (Response::ok(req.id, Ack { ok }), None)
            }
        }
    }

    fn start_task(self: &Arc<Self>, text: String) -> (String, StartGate) {
        let task_id = uuid::Uuid::new_v4().to_string();
        let emitter = Arc::new(Emitter {
            task_id: task_id.clone(),
            seq: Mutex::new(Some(0)),
            tx: self.tx.clone(),
        });
        let sink = emitter.clone();
        let tracer = Arc::new(Tracer::with_sink(move |e| sink.emit(e)));
        let (go_tx, go_rx) = oneshot::channel();
        let ctx = TaskCtx {
            text,
            emitter: emitter.clone(),
            tracer: tracer.clone(),
        };

        // Lock before spawning so the task cannot finish before it is registered.
        let Ok(mut tasks) = self.tasks.lock() else {
            return (task_id, StartGate(Some(go_tx)));
        };
        let daemon = self.clone();
        let id = task_id.clone();
        let handle = tokio::spawn(async move {
            let _ = go_rx.await;
            let outcome = daemon.agent.run(&ctx).await;
            daemon.finish(&id, outcome);
        });
        tasks.insert(
            task_id.clone(),
            TaskEntry {
                abort: handle.abort_handle(),
                emitter,
                tracer,
            },
        );
        (task_id, StartGate(Some(go_tx)))
    }

    /// Emit the terminal event unless the task was cancelled meanwhile.
    fn finish(&self, task_id: &str, outcome: Result<String, String>) {
        let Ok(mut tasks) = self.tasks.lock() else {
            return;
        };
        let Some(entry) = tasks.remove(task_id) else {
            return;
        };
        entry.emitter.emit(match outcome {
            Ok(summary) => Event::Result { summary },
            Err(message) => Event::Error { message },
        });
        log_timeline(task_id, &entry.tracer);
    }

    /// Abort a running task. Returns false if it is unknown or already done.
    pub fn cancel_task(&self, task_id: &str) -> bool {
        let t0 = now_ns();
        let entry = {
            let Ok(mut tasks) = self.tasks.lock() else {
                return false;
            };
            let Some(entry) = tasks.remove(task_id) else {
                return false;
            };
            entry.abort.abort();
            entry
        };
        self.agent.cancel_task(task_id);
        entry.tracer.record("cancel", now_ns().saturating_sub(t0));
        entry.emitter.emit(Event::Cancelled {});
        log_timeline(task_id, &entry.tracer);
        true
    }

    pub fn running_tasks(&self) -> usize {
        self.tasks.lock().map(|t| t.len()).unwrap_or(0)
    }
}

fn log_timeline(task_id: &str, tracer: &Tracer) {
    eprintln!("task {task_id} timeline:\n{}", tracer.timeline());
}
