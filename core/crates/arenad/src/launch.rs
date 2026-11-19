//! Starting a new run, whoever drives it: arenad's own runners, an external system, or a Code
//! connection's driver that arenad launches. Every path that makes a new run goes through
//! [`launch`]: the run and its first branch, its world, its `RunStart` and its setup
//! ([`runner::begin_run`]), then the branch started the way its runner says.

use std::sync::Arc;

use crate::{
    app::AppState,
    driver::{self, RunIds},
    external::{self, Endpoints},
    model::*,
    runner::{self, RunError, StartPoint},
    scenario::Scenario,
    store::{BranchId, Connection, RunId},
};

/// What a new run is: its task, team, scenario and rules (`rules.runner` says who drives it),
/// and for a run a Code connection drives, the connection whose driver arenad launches for it.
pub struct LaunchSpec<'a> {
    pub task_id: &'a str,
    pub team: Team,
    pub scenario: &'a Scenario,
    pub rules: Rules,
    pub driver: Option<&'a Connection>,
}

/// A run just started: its ids, each agent's endpoints when an external system drives it, and
/// the driver arenad launched for it (`None` when it has none, or it couldn't start: the run
/// then ended `failed`, saying why).
pub struct Launched {
    pub run_id: RunId,
    pub branch_id: BranchId,
    pub agents: Vec<Endpoints>,
    pub driver: Option<driver::Launched>,
}

/// Launches `conn`'s driver for the new run `(run, branch, task)`; `None` when it couldn't start
/// (the run then ended `failed`, saying why).
async fn launch_driver(
    state: &Arc<AppState>,
    conn: &Connection,
    (run_id, branch_id, task_id): (&str, &str, &str),
) -> Option<driver::Launched> {
    let ids = RunIds {
        run_id,
        branch_id,
        task_id,
        fork_of: None,
    };
    driver::launch_or_fail(state, conn, ids).await
}

/// Starts the run `spec` describes. arenad's own runners run its team on a spawned task, one
/// agent after another (or as the runner says); a run an external system drives goes live, its
/// team's agents (there may be none yet) started, and then its driver is launched, if it has one.
pub async fn launch(state: &Arc<AppState>, spec: LaunchSpec<'_>) -> Result<Launched, RunError> {
    let LaunchSpec {
        task_id,
        team,
        scenario,
        rules,
        driver,
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
    let driver = match driver {
        Some(conn) => launch_driver(state, conn, (&run_id, &branch_id, task_id)).await,
        None => None,
    };
    Ok(Launched {
        run_id,
        branch_id,
        agents,
        driver,
    })
}
