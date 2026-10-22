//! Unix socket server: JSON-RPC requests in, responses and events out, one
//! JSON object per line. Every connection is checked with `SO_PEERCRED`.

use crate::daemon::{Daemon, Origin, StartGate};
use protocol::{Request, Response, ServerMsg, PARSE_ERROR};
use serde_json::Value;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

const MAX_LINE: usize = 256 * 1024;

pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A stale socket from a previous run would make bind fail.
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let l = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    Ok(l)
}

/// Uids allowed to connect: root, our own uid, and the desktop user.
#[derive(Debug, Clone)]
pub struct Allowed(pub Vec<u32>);

impl Allowed {
    /// Our uid is the owner of the socket we just bound.
    pub fn for_socket(path: &Path, desktop_user: &str) -> std::io::Result<Allowed> {
        let mut uids = vec![0, std::fs::metadata(path)?.uid()];
        if let Some(uid) = uid_of(desktop_user) {
            uids.push(uid);
        }
        uids.dedup();
        Ok(Allowed(uids))
    }
}

/// Look a user up in /etc/passwd (no NSS needed on this system).
pub fn uid_of(user: &str) -> Option<u32> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let mut f = l.split(':');
        (f.next()? == user).then_some(())?;
        f.nth(1)?.parse().ok()
    })
}

pub async fn serve(listener: UnixListener, daemon: Arc<Daemon>, origin: Origin, allowed: Allowed) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let uid = stream.peer_cred().map(|c| c.uid()).ok();
                match uid {
                    Some(uid) if allowed.0.contains(&uid) => {
                        tokio::spawn(client(stream, daemon.clone(), origin));
                    }
                    other => eprintln!("aiosd: refused {origin:?} connection from uid {other:?}"),
                }
            }
            Err(e) => eprintln!("aiosd: accept failed: {e}"),
        }
    }
}

/// Parse one request line and dispatch it. The returned gate, when
/// present, must be called after the response was queued (see `StartGate`).
pub async fn handle_line(
    daemon: &Arc<Daemon>,
    line: &str,
    origin: Origin,
) -> (Response, Option<StartGate>) {
    match serde_json::from_str::<Request>(line) {
        Ok(req) => daemon.handle(req, origin).await,
        Err(e) => (
            Response::err(Value::Null, PARSE_ERROR, format!("bad request: {e}")),
            None,
        ),
    }
}

async fn client(stream: UnixStream, daemon: Arc<Daemon>, origin: Origin) {
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
                // The approval socket only answers requests.
                e = events.recv(), if origin == Origin::Agent => match e {
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

    let mut reader = BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        match (&mut reader)
            .take(MAX_LINE as u64)
            .read_line(&mut line)
            .await
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if !line.ends_with('\n') && line.len() >= MAX_LINE {
            eprintln!("aiosd: request line too long, closing connection");
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let (resp, gate) = handle_line(&daemon, &line, origin).await;
        if resp_tx.send(resp).is_err() {
            break;
        }
        if let Some(gate) = gate {
            gate.go();
        }
    }
    writer.abort();
}
