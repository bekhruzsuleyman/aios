//! Scripted agent for exercising the event stream without a model.

use crate::daemon::TaskCtx;
use protocol::{Event, Step};
use serde_json::json;
use std::time::Duration;

pub struct FakeAgent {
    /// Base delay; steps take small multiples of it.
    unit: Duration,
}

impl FakeAgent {
    /// `unit` of ~100 ms gives realistic pacing; tests use a few ms.
    pub fn new(unit: Duration) -> Self {
        FakeAgent { unit }
    }

    pub fn steps(text: &str) -> Vec<Step> {
        let step = |id: &str, title: &str, action: &str, args| Step {
            id: id.into(),
            title: title.into(),
            action: action.into(),
            args,
        };
        vec![
            step(
                "s1",
                "Open browser",
                "app.launch",
                json!({"name": "browser"}),
            ),
            step(
                "s2",
                "Navigate to search",
                "browser.navigate",
                json!({"url": "https://search.example/"}),
            ),
            step("s3", "Search", "web.search", json!({"query": text})),
            step("s4", "Show results", "ui.show", json!({})),
        ]
    }

    pub async fn run(&self, ctx: &TaskCtx) -> Result<String, String> {
        let steps = {
            let _plan = ctx.tracer.stage("plan");
            tokio::time::sleep(self.unit * 3).await;
            Self::steps(&ctx.text)
        };
        ctx.emitter.emit(Event::PlanCreated {
            steps: steps.clone(),
        });
        for (i, step) in steps.iter().enumerate() {
            let _stage = ctx.tracer.stage(format!("step:{}", step.id));
            ctx.emitter.emit(Event::StepStarted {
                step_id: step.id.clone(),
            });
            tokio::time::sleep(self.unit * (2 + (i as u32 % 3))).await;
            ctx.emitter.emit(Event::StepDone {
                step_id: step.id.clone(),
                summary: format!("{} done", step.title),
            });
        }
        Ok(format!("Showed results for \"{}\"", ctx.text))
    }
}
