//! A branch's life: made live, driven, finished or stopped, and how it ended recorded.

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use super::{
    agents::drive,
    score::{check, freeze},
};
use crate::{app::AppState, exec::BranchCtx, model::*};

/// Where a branch's work starts. `notes[j]` is the recorded handoff note of agent `j`
/// (one per agent before `agent_index`); `resume` applies to the agent at `agent_index`.
#[derive(Debug, Clone, Default)]
pub struct StartPoint {
    pub agent_index: usize,
    pub resume: Option<Vec<Value>>,
    pub notes: Vec<String>,
}

/// Runs `run_branch` on its own task; a panic still ends the branch as `infra_error`. The branch
/// is live before this returns, so a cleanup can't slip in between the start and the task.
pub fn spawn_branch(state: &Arc<AppState>, ctx: Arc<BranchCtx>, from: StartPoint) {
    let state = state.clone();
    state.live.insert(ctx.clone());
    tokio::spawn(async move {
        let inner = tokio::spawn(run_branch(state.clone(), ctx.clone(), from));
        if let Err(e) = inner.await {
            leave_live(&state, &ctx).await;
            let ending = Ending::infra(format!("runner crashed: {e}"));
            record_ending(&state, &ctx, ending).await;
        }
    });
}

/// Runs the agents from `from.agent_index` of a branch already in `state.live` (the clock
/// ticking), scores the result, stops the world and sets the branch status. Never leaves the
/// branch `running`.
pub async fn run_branch(state: Arc<AppState>, ctx: Arc<BranchCtx>, from: StartPoint) {
    ctx.start_ticker(&state.store).await;
    let outcome = drive(&state, &ctx, from).await;
    conclude(&state, &ctx, outcome).await;
}

/// Makes a branch that an external system drives live, its clock ticking.
pub async fn go_live(state: &AppState, ctx: Arc<BranchCtx>) {
    state.live.insert(ctx.clone());
    ctx.start_ticker(&state.store).await;
}

/// Ends a live branch an external system drove: freezes and scores its world, stops it and
/// records how it ended. `None` when the branch is ending already.
pub async fn finish(state: &AppState, ctx: &BranchCtx) -> Option<Ending> {
    if !ctx.lifecycle.claim_end() {
        return None;
    }
    freeze(ctx).await;
    let outcome = check(state, ctx).await;
    Some(conclude(state, ctx, outcome).await)
}

/// Ends a live branch an external system drove that went wrong (its driver raised, or exited
/// without finishing): its world is frozen and scored like `finish`'s, so the check is recorded,
/// but the branch ends `failed` with `why`. An infra error stays one. `None` when the branch is
/// ending already.
pub async fn finish_failed(state: &AppState, ctx: &BranchCtx, why: String) -> Option<Ending> {
    if !ctx.lifecycle.claim_end() {
        return None;
    }
    freeze(ctx).await;
    let outcome = check(state, ctx).await.map(|ending| match ending.status {
        BranchStatus::InfraError => ending,
        _ => Ending::failed(why),
    });
    Some(conclude(state, ctx, outcome).await)
}

/// Stops a live branch: its agents are dropped where they are, calls that wait are refused, and
/// it ends `stopped` without being scored. A branch an external system drives ends here; shaked's
/// own runner ends its branch itself. False when the branch was stopped or ending already.
pub async fn stop(state: &AppState, ctx: &BranchCtx) -> bool {
    if !ctx.flow.stop() {
        return false;
    }
    if ctx.rules.runner == Runner::External {
        if !ctx.lifecycle.claim_end() {
            return false;
        }
        conclude(state, ctx, Ok(Ending::STOPPED)).await;
    }
    true
}

/// Stops a branch's world and records how it ended: as `outcome` says, unless a world program
/// stopped, which makes it `infra_error` whatever the agents did after.
async fn conclude(state: &AppState, ctx: &BranchCtx, outcome: Result<Ending>) -> Ending {
    leave_live(state, ctx).await;
    let ending = match (ctx.lifecycle.fatal(), outcome) {
        (Some(f), _) => Ending::infra(f),
        (None, Ok(v)) => v,
        (None, Err(e)) => Ending::infra(format!("{e:#}")),
    };
    record_ending(state, ctx, ending.clone()).await;
    ending
}

/// Stops the branch's ticker and world, then takes it out of `state.live`. In that order: a
/// cleanup is refused while a branch is live, so it can't drop instances the world is stopping.
async fn leave_live(state: &AppState, ctx: &BranchCtx) {
    ctx.stop_ticker().await;
    ctx.world.stop().await;
    state.live.remove(&ctx.branch_id);
    state.proxy.tickets.revoke(&ctx.branch_id, None);
    state.proxy.cas.drop_branch(&ctx.branch_id);
}

/// How a branch ended: its status and error.
#[derive(Debug, Clone, PartialEq)]
pub struct Ending {
    pub status: BranchStatus,
    pub error: Option<String>,
}

impl Ending {
    /// How a branch someone stopped ends: unscored.
    pub const STOPPED: Ending = Ending::new(BranchStatus::Stopped, None);

    pub const fn new(status: BranchStatus, error: Option<String>) -> Ending {
        Ending { status, error }
    }

    /// An end with `status`, saying why.
    pub fn because(status: BranchStatus, why: impl Into<String>) -> Ending {
        Ending::new(status, Some(why.into()))
    }

    /// An infrastructure problem ended the branch: `infra_error`, saying why.
    pub fn infra(why: impl Into<String>) -> Ending {
        Ending::because(BranchStatus::InfraError, why)
    }

    /// The branch ended `failed`, saying why.
    pub fn failed(why: impl Into<String>) -> Ending {
        Ending::because(BranchStatus::Failed, why)
    }

    pub fn is_infra(&self) -> bool {
        self.status == BranchStatus::InfraError
    }
}

/// Records how a branch ended, trying once more when the store fails; a failure is logged (the
/// branch then stays `running` in the store). What overhead built up after its last event has no
/// event to go on, and is dropped.
async fn record_ending(state: &AppState, ctx: &BranchCtx, Ending { status, error }: Ending) {
    ctx.overhead.take();
    let branch_id = &ctx.branch_id;
    let records = ctx.records(&state.store);
    for attempt in 1..=2 {
        let set = records.set_status(branch_id, status, error.clone());
        match set.await {
            Ok(()) => return,
            Err(e) => {
                let what = format!("could not record its end ({status:?}), attempt {attempt}");
                eprintln!("branch {branch_id}: {what}: {e:#}");
            }
        }
    }
}
