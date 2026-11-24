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
pub(crate) use score::score_now;
pub use score::NOTHING_CHECKED;
pub use start::{
    begin_run, new_seed, record_start, record_unplanned, setup_failed, stand_up, start_run,
    BranchPlan, RunError, RunOptions, MAX_SEED, NO_BENCHMARK,
};
