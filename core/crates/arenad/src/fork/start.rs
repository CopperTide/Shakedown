//! Starting a fork: the new branch, its world restored, and its agents (or its driver) running
//! again.

use std::{collections::HashMap, sync::Arc};

use serde::Serialize;

use super::{
    apply_changes, part_plans, prepare, rerun, settle_changes, ForkError, ForkPlan, ForkRequest,
    PartPlan, Rerun, TEAM_AND_CHANGES, TEAM_NOT_EXTERNAL,
};
use crate::{
    app::AppState,
    clock::BranchClock,
    exec::BranchCtx,
    external,
    model::*,
    overhead::{self, Cat},
    recorded,
    restore::restored,
    runner::{self, StartPoint},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForkResponse {
    pub branch_id: String,
    pub start_seq: i64,
    pub moved_from: Option<i64>,
    pub reason: Option<String>,
    /// For a run an external system drives: each agent's recorded history before `start_seq`
    /// and its endpoints on the new branch, which is live and waits for the system to resume it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<external::Resumable>>,
    /// For a run a Code connection drives: `relaunched` when arenad started its driver again for
    /// the new branch (with `ARENA_FORK_OF`), `failed` when it couldn't (the branch then ended
    /// `failed`, saying why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<&'static str>,
}

/// Forks `branch_id` at `at_seq` with `changes`, restoring exactly, and starts the new branch.
pub async fn fork(
    state: &Arc<AppState>,
    branch_id: &str,
    at_seq: i64,
    changes: HashMap<String, Change>,
) -> Result<ForkResponse, ForkError> {
    let restore = parts::proto::Mode::Exact;
    let req = ForkRequest {
        at_seq,
        changes,
        restore,
        team: None,
    };
    fork_with(state, branch_id, req).await
}

/// Forks `branch_id` as `req` asks and starts the new branch running. A refusal comes before
/// anything is recorded; a replay that fails ends the new branch `infra_error`.
pub async fn fork_with(
    state: &Arc<AppState>,
    branch_id: &str,
    mut req: ForkRequest,
) -> Result<ForkResponse, ForkError> {
    let parent = finished_branch(state, branch_id).await?;
    settle_changes(&parent.team, &mut req.changes, &state.config.providers);
    let events = state.store.events(branch_id).await?;
    let mut rerun = prepare(state, &parent, &events, &req).await?;
    let task_id = task_of(state, &events, &parent).await?;
    let team = new_team(&parent.team, &events, &req)?;
    let new_id = create_child(state, &parent, &rerun, &team, &req).await?;
    let (world, restore_ms) =
        start_child_world(state, &events, &parent.id, &new_id, &mut rerun).await?;
    let ctx = BranchCtx::new(new_id, Arc::new(world), team, task_id.clone())
        .with_rules(recorded::rules(&events))
        .with_clock(BranchClock::from_env(rerun.clock_ms));
    ctx.overhead.add(Cat::Restore, restore_ms);
    if ctx.rules.runner == Runner::External {
        let conn = ctx.rules.connection.clone();
        let mut res = live_child(state, ctx, rerun.plan, &events).await;
        if let Some(conn) = conn {
            let ids = (parent.run_id.as_str(), parent.id.as_str(), task_id.as_str());
            res.driver = Some(relaunch(state, &conn, ids, &res.branch_id).await);
        }
        return Ok(res);
    }
    Ok(spawn_child(state, ctx, rerun.plan))
}

/// Starts the Code connection `conn_id`'s driver again for the new branch `branch_id` of a fork
/// of `(run, parent, task)`: `relaunched`, or `failed` (the branch ended `failed`, saying why).
async fn relaunch(
    state: &Arc<AppState>,
    conn_id: &str,
    (run_id, parent, task_id): (&str, &str, &str),
    branch_id: &str,
) -> &'static str {
    let ids = crate::driver::RunIds {
        run_id,
        branch_id,
        task_id,
        fork_of: Some(parent),
    };
    match crate::launch::launch_driver(state, conn_id, ids).await {
        Some(_) => "relaunched",
        None => "failed",
    }
}

/// The new branch's team: the one the request gives (a run an external system drives only, and
/// without agent changes), else the parent's with the changes applied.
pub(super) fn new_team(
    parent: &Team,
    events: &[Event],
    req: &ForkRequest,
) -> Result<Team, ForkError> {
    let Some(team) = &req.team else {
        return Ok(apply_changes(parent, &req.changes));
    };
    if recorded::rules(events).runner != Runner::External {
        return Err(ForkError::TeamChange(TEAM_NOT_EXTERNAL));
    }
    if !req.changes.is_empty() {
        return Err(ForkError::TeamChange(TEAM_AND_CHANGES));
    }
    Ok(team.clone())
}

/// Makes the new branch of an external run live, for its system to resume: the response gives
/// each agent's history before the fork point and its endpoints.
async fn live_child(
    state: &AppState,
    ctx: BranchCtx,
    plan: ForkPlan,
    events: &[Event],
) -> ForkResponse {
    let (branch_id, base) = (ctx.branch_id.clone(), &state.config.self_base);
    let agents = external::resumables(base, &branch_id, &ctx.team, events, plan.start_seq);
    runner::go_live(state, Arc::new(ctx)).await;
    ForkResponse {
        branch_id,
        start_seq: plan.start_seq,
        moved_from: plan.moved_from,
        reason: plan.reason,
        agents: Some(agents),
        driver: None,
    }
}

/// How each part of a fork of `branch_id` at `at_seq` would be restored in `mode` (what the
/// rerun panel shows before forking).
pub async fn restore_preview(
    state: &AppState,
    branch_id: &str,
    at_seq: i64,
    restore: parts::proto::Mode,
) -> Result<Vec<PartPlan>, ForkError> {
    let branch = state
        .store
        .find_branch(branch_id)
        .await?
        .ok_or_else(|| ForkError::UnknownBranch(branch_id.to_string()))?;
    let events = state.store.events(branch_id).await?;
    let req = ForkRequest {
        at_seq,
        changes: HashMap::new(),
        restore,
        team: None,
    };
    let rerun = prepare(state, &branch, &events, &req).await?;
    Ok(part_plans(&rerun, &events))
}

/// Runs the new branch from the plan's start point; the response names where it started.
fn spawn_child(state: &Arc<AppState>, ctx: BranchCtx, plan: ForkPlan) -> ForkResponse {
    let ForkPlan {
        start_seq,
        moved_from,
        reason,
        agent_index,
        resume,
        notes,
        ..
    } = plan;
    let branch_id = ctx.branch_id.clone();
    runner::spawn_branch(
        state,
        Arc::new(ctx),
        StartPoint {
            agent_index,
            resume,
            notes,
        },
    );
    ForkResponse {
        branch_id,
        start_seq,
        moved_from,
        reason,
        agents: None,
        driver: None,
    }
}

/// The branch to fork: it must exist and have finished.
async fn finished_branch(state: &AppState, branch_id: &str) -> Result<Branch, ForkError> {
    match state.store.find_branch(branch_id).await? {
        None => Err(ForkError::UnknownBranch(branch_id.to_string())),
        Some(b) if b.status == BranchStatus::Running => Err(ForkError::BranchRunning),
        Some(b) => Ok(b),
    }
}

/// Records the new branch: the parent's run, the fork point, the effective team, the changes,
/// and how its world is restored.
async fn create_child(
    state: &AppState,
    parent: &Branch,
    rerun: &Rerun,
    team: &Team,
    req: &ForkRequest,
) -> anyhow::Result<String> {
    let changes_json = serde_json::to_value(&req.changes)?;
    let plan = &rerun.plan;
    let from = Some((parent.id.as_str(), plan.start_seq));
    let swapped = (req.team.as_ref()).map(|t| format!("the team changed to {}", t.name));
    let note = plan.reason.clone().or(swapped);
    let store = &state.store;
    let id = store
        .create_branch(&parent.run_id, from, team, &changes_json, note)
        .await?;
    store
        .set_restore(&id, req.restore, &restored(&rerun.hows()))
        .await?;
    Ok(id)
}

/// The task of the run: from `RunStart`, else from the run row.
async fn task_of(state: &AppState, events: &[Event], parent: &Branch) -> anyhow::Result<String> {
    if let Some(t) = RunStartPayload::of(events).and_then(|s| s.task_id) {
        return Ok(t);
    }
    let run = state.store.run(&parent.run_id).await?;
    run.map(|r| r.task_id)
        .ok_or_else(|| anyhow::anyhow!("no run {} for branch {}", parent.run_id, parent.id))
}

/// Copies the parent's events before the fork point and starts the new branch's world as the
/// rerun plans it, with the run's benchmark and seed: the world, and how long restoring it took.
/// A failure ends the new branch `infra_error` with its message.
async fn start_child_world(
    state: &AppState,
    events: &[Event],
    parent: &str,
    new_id: &str,
    rerun: &mut Rerun,
) -> Result<(crate::world::World, f64), ForkError> {
    let started = async {
        let start = rerun.plan.start_seq;
        state.store.copy_prefix(parent, new_id, start).await?;
        let since = std::time::Instant::now();
        let world = rerun::start_world(state, events, new_id, rerun).await?;
        Ok((world, overhead::ms_since(since)))
    }
    .await;
    if let Err(e) = &started {
        let error = match e {
            ForkError::Internal(e) => format!("{e:#}"),
            e => e.to_string(),
        };
        runner::setup_failed(state, new_id, error).await;
    }
    started
}
