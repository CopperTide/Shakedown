//! Starting a new run, whoever drives it: arenad's own runners, an external system, or a Code
//! connection's driver that arenad launches. Every path that makes a new run goes through
//! [`launch`]: the run and its first branch, its world, its `RunStart` and its setup
//! ([`runner::begin_run`]), then the branch started the way its runner says.

use std::sync::Arc;

use crate::{
    app::AppState,
    external::{self, Endpoints},
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

/// A run just started: its ids, and each agent's endpoints when an external system drives it.
#[derive(Debug, Clone)]
pub struct Launched {
    pub run_id: RunId,
    pub branch_id: BranchId,
    pub agents: Vec<Endpoints>,
}

/// Starts the run `spec` describes. arenad's own runners run its team on a spawned task, one
/// agent after another (or as the runner says); a run an external system drives goes live, its
/// team's agents (there may be none yet) started.
pub async fn launch(state: &Arc<AppState>, spec: LaunchSpec<'_>) -> Result<Launched, RunError> {
    let LaunchSpec {
        task_id,
        team,
        scenario,
        rules,
    } = spec;
    let (run_id, ctx) = runner::begin_run(state, task_id, team, rules, scenario).await?;
    let (branch_id, ctx) = (ctx.branch_id.clone(), Arc::new(ctx));
    let agents = match ctx.rules.runner {
        Runner::External => external::go(state, &ctx).await?,
        _ => {
            runner::spawn_branch(state, ctx, StartPoint::default());
            vec![]
        }
    };
    Ok(Launched {
        run_id,
        branch_id,
        agents,
    })
}
