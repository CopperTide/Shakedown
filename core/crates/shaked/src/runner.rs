//! The runners: how a run starts, how a branch's agents run (one after another or all at once),
//! how its world is scored, and how the branch ends.

mod agents;
mod lifecycle;
mod score;
mod start;

pub(crate) use agents::{agent_end, driver_for, Brief};
pub use lifecycle::{
    finish, finish_failed, go_live, run_branch, spawn_branch, stop, Ending, StartPoint,
};
pub use score::NOTHING_CHECKED;
pub(crate) use score::{check, freeze};
pub use start::{
    begin_run, new_seed, record_start, setup_failed, setup_or_infra_error, stand_up, start_run,
    BranchPlan, RunError, RunOptions, MAX_SEED,
};
