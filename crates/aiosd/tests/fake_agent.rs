#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use aiosd::agent::Agent;
use aiosd::daemon::Daemon;
use aiosd::fake::FakeAgent;
use common::{temp_dir, Client};
use protocol::{Call, Event};
use std::time::Duration;

async fn spawn_daemon(name: &str) -> std::path::PathBuf {
    let sock = temp_dir(name).join("aiosd.sock");
    let daemon = Daemon::new(Agent::Fake(FakeAgent::new(Duration::from_millis(5))));
    let listener = aiosd::server::bind(&sock).unwrap();
    tokio::spawn(aiosd::server::serve(listener, daemon));
    sock
}

fn kind(e: &Event) -> &'static str {
    match e {
        Event::PlanCreated { .. } => "plan_created",
        Event::StepStarted { .. } => "step_started",
        Event::StepDone { .. } => "step_done",
        Event::StepFailed { .. } => "step_failed",
        Event::ApprovalNeeded { .. } => "approval_needed",
        Event::Result { .. } => "result",
        Event::Error { .. } => "error",
        Event::Cancelled {} => "cancelled",
        Event::Trace { .. } => "trace",
    }
}

#[tokio::test]
async fn full_ordered_event_sequence() {
    let sock = spawn_daemon("seq").await;
    let mut a = Client::connect(&sock).await;
    let mut b = Client::connect(&sock).await;

    let task = a.start("weather in Tashkent").await;
    let events = a.collect(&task).await;

    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (0..events.len() as u64).collect::<Vec<_>>());
    assert!(events.windows(2).all(|w| w[0].ts_ns <= w[1].ts_ns));

    let kinds: Vec<&str> = events
        .iter()
        .map(|e| kind(&e.event))
        .filter(|k| *k != "trace")
        .collect();
    let mut expected = vec!["plan_created"];
    for _ in 0..4 {
        expected.extend(["step_started", "step_done"]);
    }
    expected.push("result");
    assert_eq!(kinds, expected);

    match &events[0].event {
        Event::Trace { stage, .. } => assert_eq!(stage, "plan"),
        other => panic!("expected plan trace first, got {other:?}"),
    }
    let Some(Event::PlanCreated { steps }) = events
        .iter()
        .map(|e| &e.event)
        .find(|e| matches!(e, Event::PlanCreated { .. }))
    else {
        panic!("no plan")
    };
    let titles: Vec<&str> = steps.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Open browser",
            "Navigate to search",
            "Search",
            "Show results"
        ]
    );

    // A second client sees the same stream.
    let seen_by_b = b.collect(&task).await;
    assert_eq!(seen_by_b, events);
}

#[tokio::test]
async fn cancel_produces_cancelled() {
    let sock = spawn_daemon("cancel").await;
    let mut c = Client::connect(&sock).await;
    let task = c.start("anything").await;
    loop {
        if matches!(c.next_event(&task).await.event, Event::StepStarted { .. }) {
            break;
        }
    }
    let ack = c
        .call(Call::TaskCancel {
            task_id: task.clone(),
        })
        .await;
    assert_eq!(ack["ok"], true);

    let rest = c.collect(&task).await;
    assert!(matches!(rest.last().unwrap().event, Event::Cancelled {}));
    assert!(rest
        .iter()
        .any(|e| matches!(&e.event, Event::Trace { stage, .. } if stage == "cancel")));

    // Nothing else arrives for the task after `cancelled`.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let marker = c.start("marker").await;
    let _ = c.collect(&marker).await;
    assert!(c.events.iter().all(|e| e.task_id != task));

    // Cancelling again is a no-op.
    let ack = c.call(Call::TaskCancel { task_id: task }).await;
    assert_eq!(ack["ok"], false);
}

#[tokio::test]
async fn bad_request_gets_parse_error() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let sock = spawn_daemon("bad").await;
    let mut s = tokio::net::UnixStream::connect(&sock).await.unwrap();
    s.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"shell.exec\",\"params\":{}}\n")
        .await
        .unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["error"]["code"], protocol::PARSE_ERROR);
}
