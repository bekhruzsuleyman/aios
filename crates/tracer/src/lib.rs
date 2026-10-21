//! Per-task stage timing.
//!
//! `Tracer::stage("name")` returns a guard; dropping it records how long the
//! stage took and emits a `trace` event through the tracer's sink.
//! `Tracer::timeline()` renders the recorded stages as a table.

use protocol::Event;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Monotonic nanoseconds since the first call in this process.
pub fn now_ns() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    u64::try_from(epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

type Sink = Box<dyn Fn(Event) + Send + Sync>;

#[derive(Default)]
pub struct Tracer {
    stages: Mutex<Vec<(String, u64)>>,
    sink: Option<Sink>,
}

impl Tracer {
    pub fn new() -> Self {
        Self::default()
    }

    /// A tracer that also emits a `trace` event for every recorded stage.
    pub fn with_sink(sink: impl Fn(Event) + Send + Sync + 'static) -> Self {
        Tracer {
            stages: Mutex::new(Vec::new()),
            sink: Some(Box::new(sink)),
        }
    }

    /// Start timing a stage; the duration is recorded when the guard drops.
    pub fn stage(&self, name: impl Into<String>) -> StageGuard<'_> {
        StageGuard {
            tracer: self,
            name: name.into(),
            start: Instant::now(),
        }
    }

    /// Record a stage whose duration was measured elsewhere.
    pub fn record(&self, name: impl Into<String>, dt_ns: u64) {
        let name = name.into();
        if let Ok(mut stages) = self.stages.lock() {
            stages.push((name.clone(), dt_ns));
        }
        if let Some(sink) = &self.sink {
            sink(Event::Trace { stage: name, dt_ns });
        }
    }

    /// Recorded stages in completion order, as (name, dt_ns).
    pub fn stages(&self) -> Vec<(String, u64)> {
        self.stages.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// The timeline as a table: stage, ms, cumulative ms.
    pub fn timeline(&self) -> String {
        let stages = self.stages();
        let width = stages
            .iter()
            .map(|(n, _)| n.len())
            .max()
            .unwrap_or(0)
            .max("stage".len());
        let mut out = String::new();
        let _ = writeln!(out, "{:<width$}  {:>10}  {:>10}", "stage", "ms", "cum ms");
        let mut cum = 0u64;
        for (name, dt) in &stages {
            cum = cum.saturating_add(*dt);
            let _ = writeln!(
                out,
                "{:<width$}  {:>10.3}  {:>10.3}",
                name,
                ms(*dt),
                ms(cum)
            );
        }
        out
    }

    /// Print the timeline to stderr.
    pub fn print_timeline(&self) {
        eprint!("{}", self.timeline());
    }
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

pub struct StageGuard<'a> {
    tracer: &'a Tracer,
    name: String,
    start: Instant,
}

impl Drop for StageGuard<'_> {
    fn drop(&mut self) {
        let dt = u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.tracer.record(std::mem::take(&mut self.name), dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn three_stage_timeline() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        let tracer = Tracer::with_sink(move |e| sink_events.lock().unwrap().push(e));
        for (name, ms) in [("route", 2), ("plan", 5), ("execute", 3)] {
            let _g = tracer.stage(name);
            std::thread::sleep(Duration::from_millis(ms));
        }
        let table = tracer.timeline();
        println!("{table}");
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("stage"));
        assert!(lines[1].starts_with("route"));
        assert!(lines[2].starts_with("plan"));
        assert!(lines[3].starts_with("execute"));

        let stages = tracer.stages();
        assert!(stages[1].1 >= 5_000_000);
        let cum: f64 = lines[3].split_whitespace().last().unwrap().parse().unwrap();
        assert!(cum >= 10.0, "cumulative {cum}");

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], Event::Trace { stage, .. } if stage == "route"));
    }

    #[test]
    fn now_ns_is_monotonic() {
        let a = now_ns();
        let b = now_ns();
        assert!(b >= a);
    }
}
