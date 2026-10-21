//! Client library for aiosd's Unix sockets.
//!
//! `Client::start` runs a background thread that keeps a connection to the
//! agent socket open, reconnecting with exponential backoff, and hands every
//! response and event to a caller-supplied sink. `approval.respond` is sent
//! on the separate approval socket, one short connection per answer.
//!
//! Plain std threads and blocking I/O: the shell runs its own Wayland event
//! loop and must not need an async runtime to use this.

use protocol::{Call, Request, Response, ServerMsg};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BACKOFF_MIN: Duration = Duration::from_millis(100);
const BACKOFF_MAX: Duration = Duration::from_secs(2);

/// What the client reports to its sink.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Connected,
    Disconnected(String),
    Response(Response),
    Event(protocol::EventMsg),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    NotConnected,
    Io(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotConnected => write!(f, "not connected to aiosd"),
            ClientError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}

type Sink = Arc<dyn Fn(ClientEvent) + Send + Sync>;

struct Inner {
    agent: PathBuf,
    approve: PathBuf,
    writer: Mutex<Option<UnixStream>>,
    next_id: AtomicU64,
    stop: AtomicBool,
    sink: Sink,
}

pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// Connect to `agent` (task start/cancel) and `approve` (approvals) and
    /// keep reconnecting until the client is dropped.
    pub fn start(
        agent: impl Into<PathBuf>,
        approve: impl Into<PathBuf>,
        sink: impl Fn(ClientEvent) + Send + Sync + 'static,
    ) -> Client {
        let inner = Arc::new(Inner {
            agent: agent.into(),
            approve: approve.into(),
            writer: Mutex::new(None),
            next_id: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            sink: Arc::new(sink),
        });
        let reader = inner.clone();
        std::thread::spawn(move || reader_loop(&reader));
        Client { inner }
    }

    pub fn is_connected(&self) -> bool {
        self.inner
            .writer
            .lock()
            .map(|w| w.is_some())
            .unwrap_or(false)
    }

    /// Send a request; its response arrives at the sink. Returns the id.
    pub fn send(&self, call: Call) -> Result<u64, ClientError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let mut line = serde_json::to_string(&Request::new(id, call.clone()))
            .map_err(|e| ClientError::Io(e.to_string()))?;
        line.push('\n');

        if matches!(call, Call::ApprovalRespond { .. }) {
            let path = self.inner.approve.clone();
            let sink = self.inner.sink.clone();
            std::thread::spawn(move || match exchange(&path, &line) {
                Ok(resp) => sink(ClientEvent::Response(resp)),
                Err(e) => sink(ClientEvent::Response(Response::err(
                    id.into(),
                    protocol::INTERNAL_ERROR,
                    e.to_string(),
                ))),
            });
            return Ok(id);
        }

        let mut writer = self
            .inner
            .writer
            .lock()
            .map_err(|_| ClientError::Io("client lock poisoned".into()))?;
        let stream = writer.as_mut().ok_or(ClientError::NotConnected)?;
        if let Err(e) = stream.write_all(line.as_bytes()) {
            // The reader thread notices too and reconnects.
            let _ = stream.shutdown(std::net::Shutdown::Both);
            *writer = None;
            return Err(ClientError::Io(e.to_string()));
        }
        Ok(id)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        if let Ok(mut w) = self.inner.writer.lock() {
            if let Some(s) = w.take() {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

fn reader_loop(inner: &Inner) {
    let mut backoff = BACKOFF_MIN;
    let mut reported_down = false;
    while !inner.stop.load(Ordering::Relaxed) {
        let stream = match UnixStream::connect(&inner.agent) {
            Ok(s) => s,
            Err(e) => {
                if !reported_down {
                    (inner.sink)(ClientEvent::Disconnected(e.to_string()));
                    reported_down = true;
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(BACKOFF_MAX);
                continue;
            }
        };
        let Ok(write_half) = stream.try_clone() else {
            std::thread::sleep(backoff);
            continue;
        };
        if let Ok(mut w) = inner.writer.lock() {
            *w = Some(write_half);
        }
        if inner.stop.load(Ordering::Relaxed) {
            break;
        }
        backoff = BACKOFF_MIN;
        (inner.sink)(ClientEvent::Connected);

        let reason = read_until_closed(stream, inner);
        if let Ok(mut w) = inner.writer.lock() {
            *w = None;
        }
        if inner.stop.load(Ordering::Relaxed) {
            break;
        }
        (inner.sink)(ClientEvent::Disconnected(reason));
        reported_down = true;
        std::thread::sleep(backoff);
    }
}

fn read_until_closed(stream: UnixStream, inner: &Inner) -> String {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return "aiosd closed the connection".into(),
            Ok(_) => match serde_json::from_str::<ServerMsg>(&line) {
                Ok(ServerMsg::Event(e)) => (inner.sink)(ClientEvent::Event(e)),
                Ok(ServerMsg::Response(r)) => (inner.sink)(ClientEvent::Response(r)),
                Err(_) => {} // ignore lines we do not understand
            },
            Err(e) => return e.to_string(),
        }
    }
}

/// Send one request line on a fresh connection and read one response.
fn exchange(path: &Path, line: &str) -> Result<Response, ClientError> {
    let io = |e: std::io::Error| ClientError::Io(e.to_string());
    let mut stream = UnixStream::connect(path).map_err(io)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(io)?;
    stream.write_all(line.as_bytes()).map_err(io)?;
    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    loop {
        resp.clear();
        if reader.read_line(&mut resp).map_err(io)? == 0 {
            return Err(ClientError::Io("connection closed before response".into()));
        }
        if let Ok(ServerMsg::Response(r)) = serde_json::from_str::<ServerMsg>(&resp) {
            return Ok(r);
        }
    }
}

/// One request, one response, on a fresh connection. For tools and tests.
pub fn call_once(path: &Path, id: u64, call: Call) -> Result<Response, ClientError> {
    let mut line = serde_json::to_string(&Request::new(id, call))
        .map_err(|e| ClientError::Io(e.to_string()))?;
    line.push('\n');
    exchange(path, &line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Event, EventMsg, TaskStarted};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    fn temp_sock(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aios-client-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("s.sock")
    }

    /// A minimal stand-in server: answers task.start and sends one event.
    fn serve_once(listener: &UnixListener) {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let req: Request = serde_json::from_str(&line).unwrap();
        let resp = Response::ok(
            req.id,
            TaskStarted {
                task_id: "t1".into(),
            },
        );
        let ev = EventMsg {
            ts_ns: 1,
            task_id: "t1".into(),
            seq: 0,
            event: Event::Result {
                summary: "ok".into(),
            },
        };
        for msg in [ServerMsg::Response(resp), ServerMsg::Event(ev)] {
            let mut s = serde_json::to_string(&msg).unwrap();
            s.push('\n');
            writer.write_all(s.as_bytes()).unwrap();
        }
    }

    fn wait_for(
        rx: &mpsc::Receiver<ClientEvent>,
        pred: impl Fn(&ClientEvent) -> bool,
    ) -> ClientEvent {
        loop {
            let e = rx.recv_timeout(Duration::from_secs(5)).expect("timed out");
            if pred(&e) {
                return e;
            }
        }
    }

    #[test]
    fn request_response_event_and_reconnect() {
        let agent = temp_sock("agent");
        let listener = UnixListener::bind(&agent).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let client = Client::start(&agent, agent.with_extension("approve"), move |e| {
            let _ = tx.lock().unwrap().send(e);
        });

        let server = std::thread::spawn(move || {
            serve_once(&listener);
            listener
        });
        wait_for(&rx, |e| *e == ClientEvent::Connected);
        let id = client.send(Call::TaskStart { text: "hi".into() }).unwrap();
        let ClientEvent::Response(r) = wait_for(&rx, |e| matches!(e, ClientEvent::Response(_)))
        else {
            unreachable!()
        };
        assert_eq!(r.id, serde_json::json!(id));
        assert_eq!(r.result.unwrap()["task_id"], "t1");
        wait_for(
            &rx,
            |e| matches!(e, ClientEvent::Event(ev) if ev.task_id == "t1"),
        );

        // Server goes away: client reports it and reconnects to the next one.
        let listener = server.join().unwrap();
        wait_for(&rx, |e| matches!(e, ClientEvent::Disconnected(_)));
        let server = std::thread::spawn(move || serve_once(&listener));
        wait_for(&rx, |e| *e == ClientEvent::Connected);
        client
            .send(Call::TaskStart {
                text: "again".into(),
            })
            .unwrap();
        wait_for(&rx, |e| matches!(e, ClientEvent::Event(_)));
        server.join().unwrap();
    }

    #[test]
    fn approvals_use_the_approve_socket() {
        let agent = temp_sock("agent2");
        let approve = agent.with_extension("approve");
        let approve_listener = UnixListener::bind(&approve).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        // No agent socket at all: approvals must still go through.
        let client = Client::start(&agent, &approve, move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let server = std::thread::spawn(move || {
            let (stream, _) = approve_listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            let req: Request = serde_json::from_str(&line).unwrap();
            assert!(matches!(
                req.call,
                Call::ApprovalRespond { allow: true, .. }
            ));
            let mut s =
                serde_json::to_string(&Response::ok(req.id, protocol::Ack { ok: true })).unwrap();
            s.push('\n');
            writer.write_all(s.as_bytes()).unwrap();
        });
        client
            .send(Call::ApprovalRespond {
                task_id: "t".into(),
                approval_id: "a".into(),
                allow: true,
            })
            .unwrap();
        let ClientEvent::Response(r) = wait_for(&rx, |e| matches!(e, ClientEvent::Response(_)))
        else {
            unreachable!()
        };
        assert_eq!(r.result.unwrap()["ok"], true);
        server.join().unwrap();
        assert_eq!(
            client.send(Call::TaskStart { text: "x".into() }),
            Err(ClientError::NotConnected)
        );
    }
}
