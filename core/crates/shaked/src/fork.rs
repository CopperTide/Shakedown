//! Fork: go back to step N of a branch, change components, and rerun from there with the
//! world restored as it was just before step N: exactly (from saves, or by replay), or
//! approximately when asked.

mod changes;
mod disruptions;
mod error;
mod exact;
mod plan;
mod rerun;
mod start;
#[cfg(test)]
mod tests;

pub use changes::{apply_changes, settle_changes, tools_added};
pub use disruptions::DisruptionChange;
pub use error::{ForkError, TEAM_AND_CHANGES, TEAM_NOT_EXTERNAL};
pub use plan::{
    plan_fork, plan_fork_with, ForkPlan, REASON_AGENT_RESTART, REASON_CLI, REASON_CONCURRENT,
    REASON_EARLIER, REASON_FINISHED, REASON_KIND, REASON_LAST_MODEL_CALL, REASON_NO_MODEL_CALL,
    REASON_RUN_START, REASON_TOOL, REASON_WORLD_CALL,
};
pub use rerun::{part_plans, prepare, ForkRequest, PartPlan, Rerun};
pub use start::{fork, fork_with, restore_preview, ForkResponse};
