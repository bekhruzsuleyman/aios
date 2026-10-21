//! Unix socket server: JSON-RPC requests in, responses and events out, one
//! JSON object per line. Every client receives the events of every task.

use crate::daemon::Daemon;
use protocol::{Request, Response, ServerMsg, PARSE_ERROR};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A stale socket from a previous run would make bind fail.
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    UnixListener::bind(path)
}

pub async fn serve(listener: UnixListener, daemon: Arc<Daemon>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(client(stream, daemon.clone()));
            }
            Err(e) => eprintln!("aiosd: accept failed: {e}"),
        }
    }
}

/// Parse one request line and dispatch it. The returned gate, when
/// present, must be called after the response was queued (see `StartGate`).
pub fn handle_line(
    daemon: &Arc<Daemon>,
    line: &str,
) -> (Response, Option<crate::daemon::StartGate>) {
    match serde_json::from_str::<Request>(line) {
        Ok(req) => daemon.handle(req),
        Err(e) => (
            Response::err(Value::Null, PARSE_ERROR, format!("bad request: {e}")),
            None,
        ),
    }
}

async fn client(stream: UnixStream, daemon: Arc<Daemon>) {
    let (read, mut write) = stream.into_split();
    let mut events = daemon.subscribe();
    let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<Response>();

    let writer = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                // Responses first, so `task.start` is answered before its events.
                biased;
                r = resp_rx.recv() => match r {
                    Some(r) => ServerMsg::Response(r),
                    None => break,
                },
                e = events.recv() => match e {
                    Ok(e) => ServerMsg::Event(e),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("aiosd: client lagged, dropped {n} events");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
            };
            let Ok(mut line) = serde_json::to_string(&msg) else {
                continue;
            };
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let (resp, gate) = handle_line(&daemon, &line);
        if resp_tx.send(resp).is_err() {
            break;
        }
        if let Some(gate) = gate {
            gate.go();
        }
    }
    writer.abort();
}
