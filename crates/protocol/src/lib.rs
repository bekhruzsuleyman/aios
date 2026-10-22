//! Wire types shared by aiosd, the shell bridge and test clients.
//!
//! Requests are JSON-RPC 2.0 objects. Events are newline-delimited JSON
//! objects carrying `ts_ns`, `task_id`, `seq` and a `type` tag. Both travel
//! on the same connection, one JSON object per line; a client tells them
//! apart by the `jsonrpc` key (see [`ServerMsg`]). This crate does no I/O.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";

/// A step of a plan, as shown in the shell checklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub action: String,
    pub args: Value,
}

/// Risk level of an action, as decided by the broker policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Free,
    Ask,
    Never,
}

/// Events streamed from aiosd to clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    PlanCreated {
        steps: Vec<Step>,
    },
    StepStarted {
        step_id: String,
    },
    StepDone {
        step_id: String,
        summary: String,
    },
    StepFailed {
        step_id: String,
        reason: String,
    },
    ApprovalNeeded {
        approval_id: String,
        step_id: String,
        action: String,
        args: Value,
        risk: Risk,
    },
    Result {
        summary: String,
    },
    Error {
        message: String,
    },
    Cancelled {},
    Trace {
        stage: String,
        dt_ns: u64,
    },
}

impl Event {
    /// True for events after which no further events are sent for the task.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Event::Result { .. } | Event::Error { .. } | Event::Cancelled {}
        )
    }
}

/// An event stamped with its task, sequence number and monotonic time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventMsg {
    pub ts_ns: u64,
    pub task_id: String,
    pub seq: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// The method and params of a JSON-RPC request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Call {
    #[serde(rename = "task.start")]
    TaskStart { text: String },
    #[serde(rename = "task.cancel")]
    TaskCancel { task_id: String },
    #[serde(rename = "approval.respond")]
    ApprovalRespond {
        task_id: String,
        approval_id: String,
        allow: bool,
    },
    /// Is an AI endpoint configured? Answered with [`Status`].
    #[serde(rename = "status.get")]
    StatusGet {},
    /// First-run setup: store (or only test) the AI endpoint. Accepted only
    /// on the approval socket, i.e. only from the shell. Answered with
    /// [`SettingsResult`].
    #[serde(rename = "settings.ai.set")]
    SettingsAiSet {
        endpoint: String,
        model: String,
        api_key: String,
        test_only: bool,
    },
}

/// A JSON-RPC 2.0 request from a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(flatten)]
    pub call: Call,
}

impl Request {
    pub fn new(id: impl Into<Value>, call: Call) -> Self {
        Request {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: id.into(),
            call,
        }
    }
}

/// Result of `task.start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStarted {
    pub task_id: String,
}

/// Result of `status.get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub ai_configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Result of `settings.ai.set`: `ok` is whether the endpoint answered;
/// `message` is shown to the user as is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsResult {
    pub ok: bool,
    pub message: String,
}

/// Result of `task.cancel` and `approval.respond`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// Standard JSON-RPC error codes used by aiosd.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const INTERNAL_ERROR: i64 = -32603;

/// A JSON-RPC 2.0 response to a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Value, result: impl Serialize) -> Self {
        let result = serde_json::to_value(result).unwrap_or(Value::Null);
        Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        Response {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Anything aiosd writes to a client: a response or an event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerMsg {
    Response(Response),
    Event(EventMsg),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip<T>(v: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let s = serde_json::to_string(v).unwrap();
        assert!(!s.contains('\n'), "must fit on one line: {s}");
        serde_json::from_str(&s).unwrap()
    }

    fn all_events() -> Vec<Event> {
        vec![
            Event::PlanCreated {
                steps: vec![Step {
                    id: "s1".into(),
                    title: "Search the web".into(),
                    action: "web.search".into(),
                    args: json!({"query": "rust"}),
                }],
            },
            Event::StepStarted {
                step_id: "s1".into(),
            },
            Event::StepDone {
                step_id: "s1".into(),
                summary: "3 results".into(),
            },
            Event::StepFailed {
                step_id: "s1".into(),
                reason: "denied".into(),
            },
            Event::ApprovalNeeded {
                approval_id: "a1".into(),
                step_id: "s1".into(),
                action: "app.launch".into(),
                args: json!({"name": "browser"}),
                risk: Risk::Ask,
            },
            Event::Result {
                summary: "done".into(),
            },
            Event::Error {
                message: "boom".into(),
            },
            Event::Cancelled {},
            Event::Trace {
                stage: "plan".into(),
                dt_ns: 1_500_000,
            },
        ]
    }

    #[test]
    fn every_event_round_trips() {
        for (seq, event) in all_events().into_iter().enumerate() {
            let msg = EventMsg {
                ts_ns: 42,
                task_id: "0b7c7f8e-1111-4222-8333-444455556666".into(),
                seq: seq as u64,
                event,
            };
            assert_eq!(round_trip(&msg), msg);
            let as_server = ServerMsg::Event(msg.clone());
            assert_eq!(round_trip(&as_server), as_server);
        }
    }

    #[test]
    fn event_wire_shape() {
        let msg = EventMsg {
            ts_ns: 7,
            task_id: "t".into(),
            seq: 3,
            event: Event::StepStarted {
                step_id: "s2".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&msg).unwrap(),
            json!({"ts_ns": 7, "task_id": "t", "seq": 3, "type": "step_started", "step_id": "s2"})
        );
        let cancelled = EventMsg {
            event: Event::Cancelled {},
            ..msg
        };
        assert_eq!(
            serde_json::to_value(&cancelled).unwrap()["type"],
            json!("cancelled")
        );
    }

    #[test]
    fn every_request_round_trips() {
        let calls = vec![
            Call::TaskStart {
                text: "open browser".into(),
            },
            Call::TaskCancel {
                task_id: "t".into(),
            },
            Call::ApprovalRespond {
                task_id: "t".into(),
                approval_id: "a".into(),
                allow: true,
            },
            Call::StatusGet {},
            Call::SettingsAiSet {
                endpoint: "http://10.0.2.2:8081/v1".into(),
                model: "m".into(),
                api_key: String::new(),
                test_only: true,
            },
        ];
        for (i, call) in calls.into_iter().enumerate() {
            let req = Request::new(i as u64, call);
            assert_eq!(round_trip(&req), req);
        }
    }

    #[test]
    fn request_wire_shape() {
        let req: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0", "id": 1, "method": "task.start", "params": {"text": "hi"}
        }))
        .unwrap();
        assert_eq!(req.call, Call::TaskStart { text: "hi".into() });
        let bad = serde_json::from_value::<Request>(json!({
            "jsonrpc": "2.0", "id": 1, "method": "shell.exec", "params": {"cmd": "rm -rf /"}
        }));
        assert!(bad.is_err());
    }

    #[test]
    fn every_response_round_trips() {
        let responses = vec![
            Response::ok(
                json!(1),
                TaskStarted {
                    task_id: "t".into(),
                },
            ),
            Response::ok(json!(2), Ack { ok: true }),
            Response::err(json!(3), INVALID_REQUEST, "bad"),
        ];
        for r in responses {
            assert_eq!(round_trip(&r), r);
            let as_server = ServerMsg::Response(r.clone());
            assert_eq!(round_trip(&as_server), as_server);
        }
        let started: TaskStarted = serde_json::from_value(
            Response::ok(
                json!(1),
                TaskStarted {
                    task_id: "x".into(),
                },
            )
            .result
            .unwrap(),
        )
        .unwrap();
        assert_eq!(started.task_id, "x");
    }

    #[test]
    fn terminal_events() {
        let terminal: Vec<bool> = all_events().iter().map(Event::is_terminal).collect();
        assert_eq!(
            terminal,
            vec![false, false, false, false, false, true, true, true, false]
        );
    }
}
