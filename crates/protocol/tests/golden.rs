//! Golden JSON fixtures: each file must parse into its typed message and
//! serialise back to the same JSON value.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use protocol::{EventMsg, Request, Response, ServerMsg};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

fn check<T: Serialize + DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap();
    let golden: Value = serde_json::from_str(&text).unwrap();
    let typed: T = serde_json::from_value(golden.clone())
        .unwrap_or_else(|e| panic!("{name}: does not parse: {e}"));
    assert_eq!(serde_json::to_value(&typed).unwrap(), golden, "{name}");
    typed
}

#[test]
fn golden_events() {
    for name in [
        "event_plan_created",
        "event_step_started",
        "event_step_done",
        "event_step_failed",
        "event_approval_needed",
        "event_result",
        "event_error",
        "event_cancelled",
        "event_trace",
    ] {
        let e: EventMsg = check(name);
        assert!(matches!(check::<ServerMsg>(name), ServerMsg::Event(_)));
        assert_eq!(e.task_id, "0b7c7f8e-1111-4222-8333-444455556666");
    }
}

#[test]
fn golden_requests() {
    for name in [
        "request_task_start",
        "request_task_cancel",
        "request_approval_respond",
    ] {
        let _: Request = check(name);
    }
}

#[test]
fn golden_responses() {
    for name in ["response_task_start", "response_ack", "response_error"] {
        let _: Response = check(name);
        assert!(matches!(check::<ServerMsg>(name), ServerMsg::Response(_)));
    }
}

#[test]
fn every_fixture_is_covered() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    assert_eq!(std::fs::read_dir(dir).unwrap().count(), 15);
}
