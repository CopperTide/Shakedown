//! Starting a fork: the new branch, its world restored, and its agents (or its driver) running
//! again.

use crate::{auth::Caller, store::Scoped};
use std::{collections::HashMap, sync::Arc};

use serde::Serialize;

use super::{
    apply_changes, disruptions, part_plans, prepare, rerun, settle_changes, ForkError, ForkPlan,
    ForkRequest, PartPlan, Rerun, TEAM_AND_CHANGES, TEAM_NOT_EXTERNAL,
};
use crate::{
    app::AppState,
    clock::BranchClock,
    exec::BranchCtx,
    external,
    model::*,
    overhead::{self, Cat},
    recorded,
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
    /// For a run a Code connection drives: `relaunched` when shaked started its driver again for
    /// the new branch (with `SHAKEDOWN_FORK_OF`), `failed` when it couldn't (the branch then ended
    /// `failed`, saying why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<&'static str>,
}

/// Forks `branch_id` at `at_seq` with `changes`, restoring exactly, and starts the new branch,
/// for the local owner.
pub async fn fork(
    state: &Arc<AppState>,
    branch_id: &str,
    at_seq: i64,
    changes: HashMap<String, Change>,
) -> Result<ForkResponse, ForkError> {
    let restore = RestoreMode::Exact;
    let req = ForkRequest {
        at_seq,
        changes,
        restore,
        team: None,
        disruptions: Default::default(),
    };
    let local = Arc::new(crate::auth::Caller::person());
    fork_with(state, &local, branch_id, req).await
}

/// Forks `branch_id` as `req` asks and starts the new branch running. A refusal comes before
/// anything is recorded; a replay that fails ends the new branch `infra_error`.
pub async fn fork_with(
    state: &Arc<AppState>,
    scope: &Arc<Caller>,
    branch_id: &str,
    mut req: ForkRequest,
) -> Result<ForkResponse, ForkError> {
    let records = state.store.at(scope);
    let parent = finished_branch(records, branch_id).await?;
    settle_changes(&parent.team, &mut req.changes, &state.config.providers);
    let events = records.events(branch_id).await?;
    let mut rerun = prepare(state, records, &parent, &events, &req).await?;
    let start = rerun.plan.start_seq;
    let schedule = disruptions::calls(&req.disruptions, &rerun.bench, &events, start);
    let schedule = schedule.map_err(ForkError::Disruptions)?;
    let task_id = task_of(records, &events, &parent).await?;
    let team = new_team(&parent.team, &events, &req)?;
    let new_id = create_child(records, &parent, &rerun, &team, &req).await?;
    let (world, restore_ms) =
        start_child_world(state, records, &events, &parent.id, &new_id, &mut rerun).await?;
    let ctx = BranchCtx::new(new_id, Arc::new(world), team, task_id.clone());
    let ctx = child_ctx(ctx, &events, &rerun, scope, restore_ms);
    reschedule(state, &ctx, &schedule).await?;
    if ctx.rules.runner == Runner::External {
        let conn = ctx.rules.connection.clone();
        let mut res = live_child(state, ctx, rerun.plan, &events).await;
        crate::window::watch(state, &res.branch_id);
        if let Some(conn) = conn {
            let ids = (parent.run_id.as_str(), parent.id.as_str(), task_id.as_str());
            res.driver = Some(relaunch(state, scope, &conn, ids, &res.branch_id).await);
        }
        return Ok(res);
    }
    Ok(spawn_child(state, ctx, rerun.plan))
}

/// The new branch's context: the run's rules, the clock at the fork point, the caller, and the
/// time restoring took.
fn child_ctx(
    ctx: BranchCtx,
    events: &[Event],
    rerun: &Rerun,
    scope: &Arc<Caller>,
    restore_ms: f64,
) -> BranchCtx {
    let ctx = ctx
        .with_rules(recorded::rules(events))
        .with_clock(BranchClock::from_env(rerun.clock_ms))
        .with_caller(scope.clone());
    ctx.overhead.add(Cat::Restore, restore_ms);
    ctx
}

/// Makes the fork's change to the scheduled disruptions on the new branch's world (its setup
/// steps), before any agent. A part that refuses it stops the world and ends the branch
/// `infra_error` saying why.
async fn reschedule(
    state: &AppState,
    ctx: &BranchCtx,
    calls: &[parts::proto::SetupCall],
) -> Result<(), ForkError> {
    let exec = crate::exec::ToolExec::new(state.store.clone());
    let Err(e) = exec.setup(ctx, calls).await else {
        return Ok(());
    };
    ctx.world.stop().await;
    runner::setup_failed(ctx.records(&state.store), &ctx.branch_id, e.clone()).await;
    let part = calls.first().map(|c| c.part.clone()).unwrap_or_default();
    Err(ForkError::Refused(crate::restore::Refused {
        part,
        reason: e,
    }))
}

/// Starts the Code connection `conn_id`'s driver again for the new branch `branch_id` of a fork
/// of `(run, parent, task)`: `relaunched`, or `failed` (the branch ended `failed`, saying why).
async fn relaunch(
    state: &Arc<AppState>,
    scope: &Caller,
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
    match crate::launch::launch_driver(state, scope, conn_id, ids).await {
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
    let (branch_id, base) = (ctx.branch_id.clone(), state.config.public_base());
    let mut agents =
        external::resumables(base, &branch_id, &ctx.roster.team, events, plan.start_seq);
    for a in &mut agents {
        let env = crate::proxy::env::env_for(state, &ctx, Some(&a.spec.id));
        a.endpoints = external::endpoints(base, &branch_id, &a.spec.id, env)
            .with_token(ctx.agent_token(&a.spec.id));
    }
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
    scope: &Caller,
    branch_id: &str,
    at_seq: i64,
    restore: RestoreMode,
) -> Result<Vec<PartPlan>, ForkError> {
    let records = state.store.at(scope);
    let branch = records
        .find_branch(branch_id)
        .await?
        .ok_or_else(|| ForkError::UnknownBranch(branch_id.to_string()))?;
    let events = records.events(branch_id).await?;
    let req = ForkRequest {
        at_seq,
        changes: HashMap::new(),
        restore,
        team: None,
        disruptions: Default::default(),
    };
    let rerun = prepare(state, records, &branch, &events, &req).await?;
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
async fn finished_branch(records: Scoped<'_>, branch_id: &str) -> Result<Branch, ForkError> {
    match records.find_branch(branch_id).await? {
        None => Err(ForkError::UnknownBranch(branch_id.to_string())),
        Some(b) if b.status == BranchStatus::Running => Err(ForkError::BranchRunning),
        Some(b) => Ok(b),
    }
}

/// Records the new branch: the parent's run, the fork point, the effective team, the changes,
/// and how its world is restored.
async fn create_child(
    store: Scoped<'_>,
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
    let id = store
        .create_branch(&parent.run_id, from, team, &changes_json, note)
        .await?;
    store
        .set_restore(&id, req.restore, &rerun.restored())
        .await?;
    Ok(id)
}

/// The task of the run: from `RunStart`, else from the run row.
async fn task_of(records: Scoped<'_>, events: &[Event], parent: &Branch) -> anyhow::Result<String> {
    if let Some(t) = RunStartPayload::of(events).and_then(|s| s.task_id) {
        return Ok(t);
    }
    let run = records.run(&parent.run_id).await?;
    run.map(|r| r.task_id)
        .ok_or_else(|| anyhow::anyhow!("no run {} for branch {}", parent.run_id, parent.id))
}

/// Copies the parent's events before the fork point and starts the new branch's world as the
/// rerun plans it, with the run's benchmark and seed: the world, and how long restoring it took.
/// A failure ends the new branch `infra_error` with its message.
async fn start_child_world(
    state: &AppState,
    records: Scoped<'_>,
    events: &[Event],
    parent: &str,
    new_id: &str,
    rerun: &mut Rerun,
) -> Result<(crate::world::World, f64), ForkError> {
    let started = async {
        let start = rerun.plan.start_seq;
        records.copy_prefix(parent, new_id, start).await?;
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
        runner::setup_failed(records, new_id, error).await;
    }
    started
}
