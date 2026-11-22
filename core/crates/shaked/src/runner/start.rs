//! Starting a run: its first branch, its world from the base save, its `RunStart` and the
//! task's setup.

use crate::{auth::Caller, store::Scoped};
use std::{sync::Arc, time::Instant};

use anyhow::{anyhow, Result};
use parts::proto::{BenchDescribe, WorldTimeParams};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    app::AppState,
    exec::{BranchCtx, BranchRecords, StoreRecords, ToolExec},
    model::*,
    overhead::{self, Cat, Overhead},
    scenario::{self, Scenario},
    store::{BranchId, RunId},
    world::{self, World, WorldFrom},
};

/// The largest seed a run may ask for: below 2^53, so every JSON reader keeps it exact.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// A new branch seed: random, below 2^53 so every JSON reader keeps it exact.
pub fn new_seed() -> u64 {
    (uuid::Uuid::new_v4().as_u128() as u64) & ((1 << 53) - 1)
}

/// Why a run did not start: the benchmark has no such task, or something failed on the way.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("unknown task {0}")]
    UnknownTask(String),
    /// The layer refused to admit the run (a quota reached).
    #[error("{}", .0.message)]
    Refused(crate::layer::Refusal),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Creates a run and its first branch (its world started from the benchmark's base save),
/// records `RunStart`, and runs the team on a spawned task, one agent after another. A world
/// whose tools clash is refused before anything is recorded.
pub async fn start_run(
    state: &Arc<AppState>,
    task_id: &str,
    team: Team,
) -> Result<(RunId, BranchId), RunError> {
    RunOptions::new(Runner::Chain)
        .start(state, task_id, team)
        .await
}

/// How a run on shaked's own runners starts: its runner, its scenario (else the task as it is)
/// and its model-spend limit (else `SHAKEDOWN_MAX_COST_USD`, else none).
#[derive(Debug, Clone, Copy)]
pub struct RunOptions<'a> {
    runner: Runner,
    /// Who the run is for; None: the local owner.
    scope: Option<&'a Arc<Caller>>,
    scenario: Option<&'a Scenario>,
    max_cost_usd: Option<f64>,
    benchmark: Option<&'a str>,
}

impl<'a> RunOptions<'a> {
    pub fn new(runner: Runner) -> RunOptions<'a> {
        RunOptions {
            runner,
            scope: None,
            scenario: None,
            max_cost_usd: None,
            benchmark: None,
        }
    }

    /// The run for `scope` (else for the local owner).
    pub fn scope(self, scope: &'a Arc<Caller>) -> RunOptions<'a> {
        RunOptions {
            scope: Some(scope),
            ..self
        }
    }

    /// The run on the benchmark `name` (else the current one).
    pub fn benchmark(self, name: &'a str) -> RunOptions<'a> {
        RunOptions {
            benchmark: Some(name),
            ..self
        }
    }

    /// The run in `scenario` (its disruptions added to the task's setup).
    pub fn scenario(self, scenario: &'a Scenario) -> RunOptions<'a> {
        RunOptions {
            scenario: Some(scenario),
            ..self
        }
    }

    /// The run with the spend limit `max` (`None` keeps the env default).
    pub fn max_cost(self, max: Option<f64>) -> RunOptions<'a> {
        RunOptions {
            max_cost_usd: max,
            ..self
        }
    }

    /// Starts the run (see [`launch`](crate::launch::launch)): its ids.
    pub async fn start(
        self,
        state: &Arc<AppState>,
        task_id: &str,
        team: Team,
    ) -> Result<(RunId, BranchId), RunError> {
        let started = self.launch(state, task_id, team).await?;
        Ok((started.run_id, started.branch_id))
    }

    /// Starts the run (see [`launch`](crate::launch::launch)): what started.
    pub async fn launch(
        self,
        state: &Arc<AppState>,
        task_id: &str,
        team: Team,
    ) -> Result<crate::launch::Launched, RunError> {
        let none = Scenario::default();
        let scope = match self.scope {
            Some(s) => s.clone(),
            None => Arc::new(crate::auth::Caller::person()),
        };
        let spec = crate::launch::LaunchSpec {
            scope,
            task_id,
            team,
            scenario: self.scenario.unwrap_or(&none),
            rules: Rules::new(self.runner).with_max_cost(self.max_cost_usd),
            driver: None,
            benchmark: self.benchmark,
        };
        crate::launch::launch(state, spec).await
    }
}

/// The benchmark `benchmark` names (else the current one), when it has the task `task_id`.
async fn bench_with_task(
    state: &AppState,
    benchmark: Option<&str>,
    task_id: &str,
) -> Result<BenchDescribe, RunError> {
    let bench = match benchmark {
        Some(name) => state.bench_named(name).await?,
        None => state.bench().await?,
    };
    if !bench.tasks.iter().any(|t| t.id == task_id) {
        return Err(RunError::UnknownTask(task_id.to_string()));
    }
    Ok(bench)
}

/// Everything a worker needs to stand a branch up and drive it. Serialisable: it travels in the
/// `start` frame.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BranchPlan {
    pub run_id: RunId,
    pub branch_id: BranchId,
    pub task_id: String,
    /// The benchmark the run is of (resolved, never empty).
    pub benchmark: String,
    pub team: Team,
    pub rules: Rules,
    pub scenario: Scenario,
    /// Where the branch's world clock starts: 0 for a run's first branch.
    pub clock_from_ms: i64,
}

/// Creates a run and its first branch, starts its world from the base save, records `RunStart`
/// with `rules` and `scenario`, and makes the task's setup (with the scenario's disruptions): the
/// branch, ready to be driven. `record_start`, then `stand_up` into the store.
pub async fn begin_run(
    state: &AppState,
    scope: &Arc<Caller>,
    benchmark: Option<&str>,
    task_id: &str,
    team: Team,
    rules: Rules,
    scenario: &Scenario,
) -> Result<(RunId, BranchCtx), RunError> {
    let spec = crate::launch::LaunchSpec {
        scope: scope.clone(),
        task_id,
        team,
        scenario,
        rules,
        driver: None,
        benchmark,
    };
    let plan = record_start(state, scope, &spec).await?;
    let store = StoreRecords::new(state.store.clone(), scope.clone(), plan.branch_id.clone());
    let ctx = stand_up(state, &plan, Arc::new(store)).await?;
    Ok((plan.run_id, ctx.with_caller(scope.clone())))
}

/// The first half of beginning a run: creates the run and its first branch for `scope`, after
/// checking the benchmark has the task. The plan to stand the branch up from (`RunStart` waits
/// for the world, which it records, so `stand_up` writes it first of the branch's events).
pub async fn record_start(
    state: &AppState,
    scope: &Arc<Caller>,
    spec: &crate::launch::LaunchSpec<'_>,
) -> Result<BranchPlan, RunError> {
    let bench = bench_with_task(state, spec.benchmark, spec.task_id).await?;
    // A world whose tools clash is refused before anything is recorded.
    world::plan(&state.hub, &bench).await?;
    let records = state.store.at(scope);
    let run_id = records.create_run(spec.task_id, &spec.team).await?;
    let branch_id = records
        .create_branch(&run_id, None, &spec.team, &json!([]), None)
        .await?;
    Ok(BranchPlan {
        run_id,
        branch_id,
        task_id: spec.task_id.to_string(),
        benchmark: bench.name,
        team: spec.team.clone(),
        rules: spec.rules.clone(),
        scenario: spec.scenario.clone(),
        clock_from_ms: 0,
    })
}

/// The second half: starts the branch's world from the base save, records `RunStart` (it holds
/// what the world says, so it is recorded here) and makes the task's setup, recording through
/// `records`. A failure ends the branch `infra_error` with why.
pub async fn stand_up(
    state: &AppState,
    plan: &BranchPlan,
    records: Arc<dyn BranchRecords>,
) -> Result<BranchCtx, RunError> {
    let started = start_world(state, plan, &*records).await;
    let (bench, world, start_seq, record_ms) = match started {
        Ok(s) => s,
        Err(e) => {
            log_setup_failure(&plan.branch_id, fail_branch(&*records, &e).await);
            return Err(e.into());
        }
    };
    let ctx = BranchCtx::new(
        plan.branch_id.clone(),
        Arc::new(world),
        plan.team.clone(),
        plan.task_id.clone(),
    );
    let ctx = ctx.with_rules(plan.rules.clone()).with_records(records);
    ctx.overhead.add(Cat::Record, record_ms);
    set_up_task(state, &bench, &ctx, &plan.scenario, start_seq).await?;
    Ok(ctx)
}

/// Ends a branch `infra_error` with `e`'s message.
async fn fail_branch(records: &dyn BranchRecords, e: &anyhow::Error) -> Result<()> {
    let status = BranchStatus::InfraError;
    records.set_status(status, Some(format!("{e:#}"))).await
}

/// Makes the task's setup calls (with `scenario`'s disruptions) on a new run's world, then records
/// the world's view after them (after `RunStart`, at `start_seq`, when nothing was written); when
/// a call fails, stops the world and ends the branch `infra_error` with why.
async fn set_up_task(
    state: &AppState,
    bench: &BenchDescribe,
    ctx: &BranchCtx,
    scenario: &Scenario,
    start_seq: i64,
) -> Result<()> {
    let task = bench.tasks.iter().find(|t| t.id == ctx.task_id);
    let task_calls = task.map_or(&[][..], |t| &t.setup[..]);
    let exec = ToolExec::new(state.store.clone());
    let calls = scenario::setup_calls(task_calls, scenario, bench.scenario_options.as_ref());
    let done = match calls {
        Ok(calls) => exec.setup(ctx, &calls).await,
        Err(e) => Err(e),
    };
    let e = match done {
        Ok(wrote) => {
            save_after_setup(&exec, ctx).await;
            exec.record_view(ctx, wrote.unwrap_or(start_seq)).await;
            return Ok(());
        }
        Err(e) => e,
    };
    ctx.world.stop().await;
    ctx_setup_failed(ctx, e.clone()).await;
    Err(anyhow!(e))
}

/// A save point after the task's setup, when it wrote to a part that saves at points: forks
/// where the agents start restore from it. A failure is logged; the run goes on.
async fn save_after_setup(exec: &ToolExec, ctx: &BranchCtx) {
    if let Err(e) = exec.save_point(ctx, SAVE_AFTER_SETUP).await {
        eprintln!("branch {}: save point after setup: {e}", ctx.branch_id);
    }
}

/// A new branch's setup result; a failed setup ends the branch `infra_error` with its message.
pub async fn setup_or_infra_error<T>(
    records: Scoped<'_>,
    branch_id: &str,
    r: Result<T>,
) -> Result<T> {
    if let Err(e) = &r {
        setup_failed(records, branch_id, format!("{e:#}")).await;
    }
    r
}

/// Ends a branch whose setup failed `infra_error` with `error`; a store failure is logged.
pub async fn setup_failed(records: Scoped<'_>, branch_id: &str, error: String) {
    let set = records.set_status(branch_id, BranchStatus::InfraError, Some(error));
    log_setup_failure(branch_id, set.await);
}

/// `setup_failed` for a branch that has its context: its own records end it.
async fn ctx_setup_failed(ctx: &BranchCtx, error: String) {
    let set = ctx
        .records()
        .set_status(BranchStatus::InfraError, Some(error));
    log_setup_failure(&ctx.branch_id, set.await);
}

fn log_setup_failure(branch_id: &str, set: Result<()>) {
    if let Err(e) = set {
        eprintln!("branch {branch_id}: could not record its failed setup: {e:#}");
    }
}

/// Starts the first branch's world from the base save and records `RunStart`:
/// `{task_id, team, benchmark, world, seed, base_seed, fingerprints, saves, runner, handoff,
/// systems, tools, scenario?}` (`saves` names only the parts that keep saves; `base_seed` is the
/// seed the base save was made with; `runner` and `handoff` are the run's rules; `systems` and
/// `tools` are what `World::systems` and `World::tool_info` say; `scenario` the expanded scenario,
/// when the run has one), with the world's restore as its overhead. The benchmark, the world,
/// `RunStart`'s seq and how long recording it took (overhead for the branch's next event).
async fn start_world(
    state: &AppState,
    plan: &BranchPlan,
    records: &dyn BranchRecords,
) -> Result<(BenchDescribe, World, i64, f64)> {
    crate::mode::local_only(state.store.layer().policy(), "starting a world's parts")
        .map_err(anyhow::Error::msg)?;
    let bench = state.bench_named(&plan.benchmark).await?;
    let mut planned = world::plan(&state.hub, &bench).await?;
    let branch_id = plan.branch_id.as_str();
    let seed = plan.scenario.seed.unwrap_or_else(new_seed);
    let world_time = world_time_of(&bench, &plan.task_id);
    planned.world_time = Some(world_time);
    let since = Instant::now();
    let hub = state.hub.clone();
    let world =
        World::start_planned(hub, &bench, planned, branch_id, seed, WorldFrom::Base).await?;
    let restored = Overhead::of(Cat::Restore, overhead::ms_since(since));
    let mut start = run_start(&bench, &world, seed, plan);
    start.world_epoch_ms = Some(world_time.epoch_ms);
    let since = Instant::now();
    let payload = serde_json::to_value(&start)?;
    match records
        .append(None, EventKind::RunStart, payload, restored)
        .await
    {
        Ok(e) => Ok((bench, world, e.seq, overhead::ms_since(since))),
        Err(e) => {
            world.stop().await;
            Err(e)
        }
    }
}

/// A new run's world time: its epoch (the task's `clock_start` as UTC, else now) and the clock
/// speed its branches run at.
fn world_time_of(bench: &BenchDescribe, task_id: &str) -> WorldTimeParams {
    let task = bench.tasks.iter().find(|t| t.id == task_id);
    let start = task.and_then(|t| t.clock_start.as_deref());
    WorldTimeParams {
        epoch_ms: crate::world_clock::WorldClock::epoch_ms(start, chrono::Utc::now()),
        speed: crate::clock::speed_from_env(),
    }
}

/// The payload of `RunStart` (see `start_world`).
fn run_start(bench: &BenchDescribe, world: &World, seed: u64, of: &BranchPlan) -> RunStartPayload {
    let scenario = (!of.scenario.is_empty()).then(|| json!(of.scenario));
    RunStartPayload {
        task_id: Some(of.task_id.to_string()),
        team: Some(of.team.clone()),
        benchmark: Some(bench.name.clone()),
        world: Some(bench.world.clone()),
        seed,
        base_seed: world::SHARED_SEED,
        fingerprints: world.fingerprints(),
        saves: Some(world.saves()),
        runner: Some(of.rules.runner),
        handoff: Some(of.rules.handoff_word().to_string()),
        systems: world.systems(),
        tools: world.tool_info(),
        scenario,
        max_cost_usd: of.rules.max_cost_usd,
        connection: of.rules.connection.clone(),
        world_epoch_ms: None,
        world_minutes: of.rules.world_minutes,
    }
}

impl From<crate::layer::Refusal> for RunError {
    fn from(r: crate::layer::Refusal) -> RunError {
        RunError::Refused(r)
    }
}
