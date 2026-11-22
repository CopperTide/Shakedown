//! Why a fork is refused.

use crate::restore::Refused;

pub const TEAM_NOT_EXTERNAL: &str =
    "a fork can change the whole team only in a run an external system drives; change its agents instead";

pub const TEAM_AND_CHANGES: &str = "give a fork either agent changes or a new team, not both";

#[derive(Debug, thiserror::Error)]
pub enum ForkError {
    /// No event with that seq on the branch (or the timeline lacks a step the plan needs).
    #[error("there is no step with that number on this branch")]
    UnknownSeq,
    /// No branch with that id.
    #[error("there is no branch \"{0}\"")]
    UnknownBranch(String),
    /// A change names an agent that isn't in the team (or an event names one).
    #[error("there is no agent \"{0}\" in this team")]
    UnknownAgent(String),
    /// The source branch is still running.
    #[error("the branch is still running")]
    BranchRunning,
    /// A successful write at `seq` has no save, so the world before the fork point
    /// can't be restored exactly.
    #[error("the write at step {seq} has no snapshot, so the world before this step can't be restored exactly")]
    NoRestorePoint { seq: i64 },
    /// An agent before the fork point has no recorded handoff note.
    #[error("agent \"{0}\" has no recorded handoff note to carry over")]
    MissingHandoff(String),
    /// A part can't be restored in the mode asked for.
    #[error("{0}")]
    Refused(Refused),
    /// Replaying a part gave back something else at step `seq` than the first time.
    #[error("replay of {part} diverged at step {seq}, so it can't be restored exactly")]
    ReplayDiverged { part: String, seq: i64 },
    /// A part to replay runs on a different machine or version than when the run started.
    #[error(
        "the {part} integration is running on a different machine or version than the original"
    )]
    FingerprintMismatch { part: String },
    /// A tools change with an empty list: that would mean every tool again, not none.
    #[error("a fork can't cut every tool of \"{0}\" with `tools: []`, which means every tool again; make it read_only or list the tools it keeps")]
    NoTools(String),
    /// A tools change lists a tool the world doesn't offer agents.
    #[error("the world has no tool \"{0}\" for an agent to use")]
    UnknownTool(String),
    /// A change to the scheduled disruptions that can't be made (see
    /// [`ForkRequest::disruptions`]).
    #[error("{0}")]
    Disruptions(String),
    /// A team change the run can't take (see [`ForkRequest::team`]).
    #[error("{0}")]
    TeamChange(&'static str),
    /// Storage or database failure.
    #[error("fork failed: {0:#}")]
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ForkError {
    fn from(e: anyhow::Error) -> ForkError {
        ForkError::Internal(e)
    }
}
