//! Starting a run: its first branch, its world from the base save, its `RunStart` and the
//! task's setup.

use std::{sync::Arc, time::Instant};

use anyhow::{anyhow, Result};
use parts::proto::BenchDescribe;
use serde_json::json;

use crate::{
    app::AppState,
    exec::{BranchCtx, ToolExec},
    model::*,
    overhead::{self, Cat},
    scenario::{self, Scenario},
    store::{BranchId, RunId},
    world::{self, Plan, World, WorldFrom},
};

/// The largest seed a run may ask for: below 2^53, so every JSON reader keeps it exact.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// A new branch seed: random, below 2^53 so every JSON reader keeps it exact.
fn new_seed() -> u64 {
    (uuid::Uuid::new_v4().as_u128() as u64) & ((1 << 53) - 1)
}

/// Why a run did not start: the benchmark has no such task, or something failed on the way.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("unknown task {0}")]
    UnknownTask(String),
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

/// How a run on arenad's own runners starts: its runner, its scenario (else the task as it is)
/// and its model-spend limit (else `ARENA_MAX_COST_USD`, else none).
#[derive(Debug, Clone, Copy)]
pub struct RunOptions<'a> {
    runner: Runner,
    scenario: Option<&'a Scenario>,
    max_cost_usd: Option<f64>,
}

impl<'a> RunOptions<'a> {
    pub fn new(runner: Runner) -> RunOptions<'a> {
        RunOptions {
            runner,
            scenario: None,
            max_cost_usd: None,
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
        let none = Scenario::default();
        let spec = crate::launch::LaunchSpec {
            task_id,
            team,
            scenario: self.scenario.unwrap_or(&none),
            rules: Rules::new(self.runner).with_max_cost(self.max_cost_usd),
        };
        let started = crate::launch::launch(state, spec).await?;
        Ok((started.run_id, started.branch_id))
    }
}

/// Creates a run and its first branch, starts its world from the base save, records `RunStart`
/// with `rules` and `scenario`, and makes the task's setup (with the scenario's disruptions): the
/// branch, ready to be driven.
pub async fn begin_run(
    state: &AppState,
    task_id: &str,
    team: Team,
    rules: Rules,
    scenario: &Scenario,
) -> Result<(RunId, BranchCtx), RunError> {
    let bench = state.bench().await?;
    if !bench.tasks.iter().any(|t| t.id == task_id) {
        return Err(RunError::UnknownTask(task_id.to_string()));
    }
    let planned = world::plan(&state.hub, &bench).await?;
    let run_id = state.store.create_run(task_id, &team).await?;
    let branch_id = state
        .store
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await?;
    let start = RunStartOf {
        branch_id: &branch_id,
        task_id,
        team: &team,
        rules: rules.clone(),
        scenario,
    };
    let started = start_world(state, &bench, planned, start).await;
    let (world, start_seq) = setup_or_infra_error(state, &branch_id, started).await?;
    let ctx = BranchCtx::new(branch_id, Arc::new(world), team, task_id.to_string());
    let ctx = ctx.with_rules(rules);
    set_up_task(state, &bench, &ctx, scenario, start_seq).await?;
    Ok((run_id, ctx))
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
            exec.record_view(ctx, wrote.unwrap_or(start_seq)).await;
            return Ok(());
        }
        Err(e) => e,
    };
    ctx.world.stop().await;
    setup_failed(state, &ctx.branch_id, e.clone()).await;
    Err(anyhow!(e))
}

/// What a run's `RunStart` records besides its world.
struct RunStartOf<'a> {
    branch_id: &'a str,
    task_id: &'a str,
    team: &'a Team,
    rules: Rules,
    scenario: &'a Scenario,
}

/// A new branch's setup result; a failed setup ends the branch `infra_error` with its message.
pub async fn setup_or_infra_error<T>(state: &AppState, branch_id: &str, r: Result<T>) -> Result<T> {
    if let Err(e) = &r {
        setup_failed(state, branch_id, format!("{e:#}")).await;
    }
    r
}

/// Ends a branch whose setup failed `infra_error` with `error`; a store failure is logged.
pub async fn setup_failed(state: &AppState, branch_id: &str, error: String) {
    let set = state
        .store
        .set_status(branch_id, BranchStatus::InfraError, Some(error));
    if let Err(e) = set.await {
        eprintln!("branch {branch_id}: could not record its failed setup: {e:#}");
    }
}

/// Starts the first branch's world from the base save and records `RunStart`:
/// `{task_id, team, benchmark, world, seed, base_seed, fingerprints, saves, runner, handoff,
/// systems, tools, scenario?}` (`saves` names only the parts that keep saves; `base_seed` is the
/// seed the base save was made with; `runner` and `handoff` are the run's rules; `systems` and
/// `tools` are what `World::systems` and `World::tool_info` say; `scenario` the expanded scenario,
/// when the run has one). The world, and `RunStart`'s seq.
async fn start_world(
    state: &AppState,
    bench: &BenchDescribe,
    planned: Plan,
    of: RunStartOf<'_>,
) -> Result<(World, i64)> {
    let branch_id = of.branch_id;
    let seed = of.scenario.seed.unwrap_or_else(new_seed);
    let hub = state.hub.clone();
    let since = Instant::now();
    let world = World::start_planned(hub, bench, planned, branch_id, seed, WorldFrom::Base).await?;
    overhead::add_since(branch_id, Cat::Restore, since);
    let start = run_start(bench, &world, seed, &of);
    match state.store.record(branch_id, None, &start).await {
        Ok(e) => Ok((world, e.seq)),
        Err(e) => {
            world.stop().await;
            Err(e)
        }
    }
}

/// The payload of `RunStart` (see `start_world`).
fn run_start(
    bench: &BenchDescribe,
    world: &World,
    seed: u64,
    of: &RunStartOf<'_>,
) -> RunStartPayload {
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
    }
}
