//! Starting a new run, whoever drives it: arenad's own runners, an external system, or a Code
//! connection's driver that arenad launches. Every path that makes a new run goes through
//! [`launch`]: the run and its first branch, its world, its `RunStart` and its setup
//! ([`runner::begin_run`]), then the branch started the way its runner says.

use std::sync::Arc;

use crate::{
    app::AppState,
    driver::{self, RunIds},
    exec::BranchCtx,
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

/// Launches the Code connection `conn_id`'s driver for the branch `ids` names (a new run's, or a
/// fork's): `None` when it couldn't start, or the connection is gone (the branch then ended
/// `failed`, saying why).
pub async fn launch_driver(
    state: &Arc<AppState>,
    conn_id: &str,
    ids: RunIds<'_>,
) -> Option<driver::Launched> {
    let found = state.store.connection(conn_id).await;
    let found = found.map_err(|e| format!("reading the Code connection: {e:#}"));
    let gone = || format!("the Code connection {conn_id} was deleted");
    match found.and_then(|c| c.ok_or_else(gone)) {
        Ok(conn) => driver::launch_or_fail(state, &conn, ids).await,
        Err(why) => {
            state.live.fail_if_live(state, ids.branch_id, why).await;
            None
        }
    }
}

/// Starts a new run's first branch the way its runner says: arenad's own runners on a spawned
/// task; a run an external system drives made live, with its agents' starts (their endpoints).
async fn go(state: &Arc<AppState>, ctx: Arc<BranchCtx>) -> anyhow::Result<Vec<Endpoints>> {
    if ctx.rules.runner == Runner::External {
        return external::go(state, &ctx).await;
    }
    runner::spawn_branch(state, ctx, StartPoint::default());
    Ok(vec![])
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
    let branch_id = ctx.branch_id.clone();
    let agents = go(state, Arc::new(ctx)).await?;
    let ids = RunIds {
        run_id: &run_id,
        branch_id: &branch_id,
        task_id,
        fork_of: None,
    };
    let driver = match driver {
        Some(conn) => driver::launch_or_fail(state, conn, ids).await,
        None => None,
    };
    Ok(Launched {
        run_id,
        branch_id,
        agents,
        driver,
    })
}
