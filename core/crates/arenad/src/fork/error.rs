//! Why a fork is refused.

use crate::restore::Refused;

pub const TEAM_NOT_EXTERNAL: &str =
    "a fork can change the whole team only in a run an external system drives; change its agents instead";

pub const TEAM_AND_CHANGES: &str = "give a fork either agent changes or a new team, not both";

#[derive(Debug)]
pub enum ForkError {
    /// No event with that seq on the branch (or the timeline lacks a step the plan needs).
    UnknownSeq,
    /// No branch with that id.
    UnknownBranch(String),
    /// A change names an agent that isn't in the team (or an event names one).
    UnknownAgent(String),
    /// The source branch is still running.
    BranchRunning,
    /// A successful write at `seq` has no save, so the world before the fork point
    /// can't be restored exactly.
    NoRestorePoint { seq: i64 },
    /// An agent before the fork point has no recorded handoff note.
    MissingHandoff(String),
    /// A part can't be restored in the mode asked for.
    Refused(Refused),
    /// Replaying a part gave back something else at step `seq` than the first time.
    ReplayDiverged { part: String, seq: i64 },
    /// A part to replay runs on a different machine or version than when the run started.
    FingerprintMismatch { part: String },
    /// A team change the run can't take (see [`ForkRequest::team`]).
    TeamChange(&'static str),
    /// Storage or database failure.
    Internal(anyhow::Error),
}

impl std::fmt::Display for ForkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForkError::UnknownSeq => write!(f, "there is no step with that number on this branch"),
            ForkError::UnknownBranch(b) => write!(f, "there is no branch \"{b}\""),
            ForkError::UnknownAgent(a) => write!(f, "there is no agent \"{a}\" in this team"),
            ForkError::BranchRunning => write!(f, "the branch is still running"),
            ForkError::NoRestorePoint { seq } => write!(
                f,
                "the write at step {seq} has no snapshot, so the world before this step can't be restored exactly"
            ),
            ForkError::MissingHandoff(a) => write!(f, "agent \"{a}\" has no recorded handoff note to carry over"),
            ForkError::Refused(r) => write!(f, "{r}"),
            ForkError::ReplayDiverged { part, seq } => write!(
                f,
                "replay of {part} diverged at step {seq}, so it can't be restored exactly"
            ),
            ForkError::FingerprintMismatch { part } => write!(
                f,
                "the {part} integration is running on a different machine or version than the original"
            ),
            ForkError::TeamChange(why) => write!(f, "{why}"),
            ForkError::Internal(e) => write!(f, "fork failed: {e:#}"),
        }
    }
}

impl std::error::Error for ForkError {}

impl From<anyhow::Error> for ForkError {
    fn from(e: anyhow::Error) -> ForkError {
        ForkError::Internal(e)
    }
}
