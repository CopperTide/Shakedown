//! Starting a new run, whoever drives it: arenad's own runners, an external system, or a Code
//! connection's driver that arenad launches. Every path that makes a new run goes through
//! [`launch`]: the run and its first branch, its world, its `RunStart` and its setup
//! ([`runner::begin_run`]), then the branch started the way its runner says.

use std::sync::Arc;

use crate::{
    app::AppState,
    model::*,
    runner::{self, RunError, StartPoint},
    scenario::Scenario,
    store::{BranchId, RunId},
};

/// What a new run is: its task, team, scenario and rules (`rules.runner` says who drives it).
pub struct LaunchSpec<'a> {
    pub task_id: &'a str,
    pub team: Team,
    pub scenario: &'a Scenario,
    pub rules: Rules,
}

/// A run just started: its ids.
#[derive(Debug, Clone)]
pub struct Launched {
    pub run_id: RunId,
    pub branch_id: BranchId,
}

/// Starts the run `spec` describes. arenad's own runners run its team on a spawned task, one
/// agent after another (or as the runner says).
pub async fn launch(state: &Arc<AppState>, spec: LaunchSpec<'_>) -> Result<Launched, RunError> {
    let LaunchSpec {
        task_id,
        team,
        scenario,
        rules,
    } = spec;
    let (run_id, ctx) = runner::begin_run(state, task_id, team, rules, scenario).await?;
    let branch_id = ctx.branch_id.clone();
    runner::spawn_branch(state, Arc::new(ctx), StartPoint::default());
    Ok(Launched { run_id, branch_id })
}
