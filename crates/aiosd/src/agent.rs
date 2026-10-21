//! Agent selection. T2 only has the scripted `FakeAgent`.

use crate::daemon::TaskCtx;
use crate::fake::FakeAgent;

pub enum Agent {
    Fake(FakeAgent),
}

impl Agent {
    /// Run one task to completion; returns the result summary or an error.
    pub async fn run(&self, ctx: &TaskCtx) -> Result<String, String> {
        match self {
            Agent::Fake(a) => a.run(ctx).await,
        }
    }

    /// Resolve a pending approval. The fake agent never asks.
    pub fn respond_approval(&self, _task_id: &str, _approval_id: &str, _allow: bool) -> bool {
        false
    }

    /// Called after a task was aborted.
    pub fn cancel_task(&self, _task_id: &str) {}
}
