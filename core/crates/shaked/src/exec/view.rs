//! Recording the world's view: after a run's setup and after every step that wrote (a link's or
//! a disruption's included), shaked asks the benchmark for the world's view and records it as a
//! `world_view` event right after that step. A view that fails is logged and records nothing; it
//! never fails the run.

use std::time::Duration;

use super::{BranchCtx, ToolExec};
use crate::{model::WorldViewPayload, overhead::Cat};

/// How long the benchmark gets to draw one view.
const VIEW_TIMEOUT: Duration = Duration::from_secs(10);

impl ToolExec {
    /// Records the world's view after the step at `seq_of` (with the world clock paused), when
    /// the benchmark draws one.
    pub async fn record_view(&self, branch: &BranchCtx, seq_of: i64) {
        if !branch.world.draws_view() {
            return;
        }
        let paused = branch.clock.paused();
        let drawn = tokio::time::timeout(VIEW_TIMEOUT, branch.world.view()).await;
        branch.overhead.add(Cat::Record, paused.elapsed_ms_f64());
        drop(paused);
        let view = match drawn {
            Ok(Ok(view)) => view,
            Ok(Err(e)) => return eprintln!("branch {}: no view: {e}", branch.branch_id),
            Err(_) => return eprintln!("branch {}: the view took too long", branch.branch_id),
        };
        let payload = WorldViewPayload { seq_of, view };
        if let Err(e) = self.record(branch, None, &payload).await {
            eprintln!(
                "branch {}: could not record the view: {e}",
                branch.branch_id
            );
        }
    }
}
