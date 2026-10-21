//! Test client for the aiosd Unix socket.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use protocol::{Call, EventMsg, Request, Response, ServerMsg};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;

pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aiosd-test-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
    next_id: u64,
    pub events: Vec<EventMsg>,
}

impl Client {
    pub async fn connect(path: &Path) -> Client {
        let stream = UnixStream::connect(path).await.unwrap();
        let (r, w) = stream.into_split();
        Client {
            lines: BufReader::new(r).lines(),
            write: w,
            next_id: 1,
            events: Vec::new(),
        }
    }

    async fn next_msg(&mut self) -> ServerMsg {
        let line = tokio::time::timeout(Duration::from_secs(10), self.lines.next_line())
            .await
            .expect("timed out waiting for aiosd")
            .unwrap()
            .expect("aiosd closed the connection");
        serde_json::from_str(&line).unwrap()
    }

    /// Send a request and return its result, buffering events seen meanwhile.
    pub async fn call(&mut self, call: Call) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut line = serde_json::to_string(&Request::new(id, call)).unwrap();
        line.push('\n');
        self.write.write_all(line.as_bytes()).await.unwrap();
        loop {
            match self.next_msg().await {
                ServerMsg::Response(Response {
                    id: rid,
                    result,
                    error,
                    ..
                }) if rid == id => {
                    assert!(error.is_none(), "rpc error: {error:?}");
                    return result.unwrap();
                }
                ServerMsg::Response(_) => {}
                ServerMsg::Event(e) => self.events.push(e),
            }
        }
    }

    pub async fn start(&mut self, text: &str) -> String {
        let r = self.call(Call::TaskStart { text: text.into() }).await;
        r["task_id"].as_str().unwrap().to_string()
    }

    /// Next event for `task_id` (buffered ones first).
    pub async fn next_event(&mut self, task_id: &str) -> EventMsg {
        if let Some(i) = self.events.iter().position(|e| e.task_id == task_id) {
            return self.events.remove(i);
        }
        loop {
            if let ServerMsg::Event(e) = self.next_msg().await {
                if e.task_id == task_id {
                    return e;
                }
            }
        }
    }

    /// All events for `task_id` up to and including the terminal one.
    pub async fn collect(&mut self, task_id: &str) -> Vec<EventMsg> {
        let mut out = Vec::new();
        loop {
            let e = self.next_event(task_id).await;
            let done = e.event.is_terminal();
            out.push(e);
            if done {
                return out;
            }
        }
    }
}
