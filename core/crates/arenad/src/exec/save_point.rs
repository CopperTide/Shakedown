//! Save points: the parts that save at points (`describe.save_at: "points"`) are saved before a
//! model call through the gateway when they changed, when a driver asks, and at the run's end
//! (with the final saves). Each is a `SavePoint` event: bookkeeping, no step.

use super::{failure, BranchCtx, ToolExec};
use crate::{
    model::{Event, SavePointPayload},
    overhead::Cat,
};

impl ToolExec {
    /// Saves the branch's unsaved points parts behind the gate, the world clock paused, and
    /// records the save point with `reason`; None when nothing was unsaved. A failed save is the
    /// event's `error` and its part stays unsaved.
    pub async fn save_point(
        &self,
        branch: &BranchCtx,
        reason: &str,
    ) -> Result<Option<Event>, String> {
        let _gate = branch.gate_timed().await;
        if !branch.world.has_unsaved_points() {
            return Ok(None);
        }
        let paused = branch.clock.paused();
        let step = branch.world.save_points().await;
        branch.overhead.add(Cat::Save, paused.elapsed_ms_f64());
        let payload = SavePointPayload {
            reason: reason.to_string(),
            saves: step.saves,
            error: step.error.map(|e| failure(branch, e)),
            paused_ms: paused.elapsed_ms(),
        };
        drop(paused);
        let recorded = branch.record(&self.store, None, &payload).await;
        recorded
            .map(Some)
            .map_err(|e| format!("recording a save point: {e:#}"))
    }
}
