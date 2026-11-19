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
    begin_run, setup_failed, setup_or_infra_error, start_run, start_run_capped, start_run_in,
    start_run_with, RunError, MAX_SEED,
};
