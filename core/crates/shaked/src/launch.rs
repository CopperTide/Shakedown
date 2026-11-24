//! Starting a new run, whoever drives it: shaked's own runners, an external system, or a Code
//! connection's driver that shaked launches. Every path that makes a new run goes through
//! [`launch`]: the run and its first branch, its world, its `RunStart` and its setup
//! ([`runner::begin_run`]), then the branch started the way its runner says. On a control box
//! with [`Hosting`](crate::hosting::Hosting) the run and its branch are recorded here and the
//! branch runs in a sandbox instead.

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
/// and for a run a Code connection drives, the connection whose driver shaked launches for it.
pub struct LaunchSpec<'a> {
    /// Who the run is for.
    pub scope: Arc<crate::auth::Caller>,
    pub task_id: &'a str,
    pub team: Team,
    pub scenario: &'a Scenario,
    pub rules: Rules,
    pub driver: Option<&'a Connection>,
    /// The benchmark the run is of (None: the current one).
    pub benchmark: Option<&'a str>,
}

/// A run just started: its ids, each agent's endpoints when an external system drives it, and
/// the driver shaked launched for it (`None` when it has none, or it couldn't start: the run
/// then ended `failed`, saying why).
pub struct Launched {
    pub run_id: RunId,
    pub branch_id: BranchId,
    pub agents: Vec<Endpoints>,
    pub driver: Option<driver::Launched>,
    /// An external run's run token (`sdr_…`), for whoever drives it.
    pub run_token: Option<String>,
    /// What the layer warned when it admitted the run (a quota nearly used), if anything.
    pub warning: Option<String>,
}

/// Launches the Code connection `conn_id`'s driver for the branch `ids` names (a new run's, or a
/// fork's): `None` when it couldn't start, or the connection is gone (the branch then ended
/// `failed`, saying why).
pub async fn launch_driver(
    state: &Arc<AppState>,
    scope: &crate::auth::Caller,
    conn_id: &str,
    ids: RunIds<'_>,
) -> Option<driver::Launched> {
    let found = state.store.at(scope).connection(conn_id).await;
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

/// Starts a new run's first branch the way its runner says: shaked's own runners on a spawned
/// task; a run an external system drives made live, with its agents' starts (their endpoints).
async fn go(state: &Arc<AppState>, ctx: Arc<BranchCtx>) -> anyhow::Result<Vec<Endpoints>> {
    if ctx.rules.runner == Runner::External {
        let agents = external::go(state, &ctx).await?;
        crate::window::watch(state, &ctx.branch_id);
        return Ok(agents);
    }
    runner::spawn_branch(state, ctx, StartPoint::default());
    Ok(vec![])
}

/// A run token for a run an external system drives (None for shaked's own runners), good as
/// long as the branch's agent tokens.
async fn run_token_of(
    state: &AppState,
    ctx: &BranchCtx,
    run_id: &str,
) -> anyhow::Result<Option<String>> {
    if ctx.rules.runner != Runner::External {
        return Ok(None);
    }
    let expires = chrono::Utc::now() + ctx.token_ttl();
    let records = ctx.scoped(&state.store);
    let kind = crate::auth::RunTokenKind::Run;
    Ok(Some(
        records
            .mint_run_token(run_id, &ctx.branch_id, kind, None, None, expires)
            .await?,
    ))
}

/// Starts the run `spec` describes. shaked's own runners run its team on a spawned task, one
/// agent after another (or as the runner says); a run an external system drives goes live, its
/// team's agents (there may be none yet) started, and then its driver is launched, if it has one.
/// The layer admits the run first (a refusal starts nothing) and meters it once it has begun, so
/// every way a run starts (`POST /api/runs`, suites, experiments, a suite's claim) is counted.
pub async fn launch(state: &Arc<AppState>, spec: LaunchSpec<'_>) -> Result<Launched, RunError> {
    if let Some(hosting) = state.hosting.get() {
        return hosted(state, hosting, &spec).await;
    }
    let (warning, run_id, ctx) = admitted_begin(state, &spec).await?;
    let branch_id = ctx.branch_id.clone();
    let run_token = run_token_of(state, &ctx, &run_id).await?;
    let agents = go(state, Arc::new(ctx)).await?;
    let ids = RunIds {
        run_id: &run_id,
        branch_id: &branch_id,
        task_id: spec.task_id,
        fork_of: None,
    };
    let driver = match spec.driver {
        Some(conn) => driver::launch_or_fail(state, conn, ids).await,
        None => None,
    };
    Ok(Launched {
        run_id,
        branch_id,
        agents,
        driver,
        run_token,
        warning,
    })
}

/// Why a control box that runs branches in sandboxes refuses a run an external system or a
/// Code connection drives.
pub const NOT_IN_A_SANDBOX: &str =
    "runs an external system or a Code connection drives don't run in the cloud yet";

/// [`launch`] on a control box with [`Hosting`](crate::hosting::Hosting): the run admitted,
/// recorded here (`record_start`) and metered, its branch run in a sandbox. When the sandbox
/// can't start, the branch ends `infra_error`.
async fn hosted(
    state: &Arc<AppState>,
    hosting: &crate::hosting::Hosting,
    spec: &LaunchSpec<'_>,
) -> Result<Launched, RunError> {
    if spec.rules.runner == Runner::External || spec.driver.is_some() {
        let status = axum::http::StatusCode::UNPROCESSABLE_ENTITY;
        let refusal = crate::layer::Refusal::new(status, NOT_IN_A_SANDBOX);
        return Err(RunError::Refused(refusal));
    }
    let warning = admit_run(state, spec).await?;
    let plan = runner::record_start(state, spec).await?;
    meter_run(state, spec, &plan.run_id, &plan.branch_id).await;
    hosting.start_or_fail(state, &plan, &spec.scope).await;
    Ok(Launched {
        run_id: plan.run_id,
        branch_id: plan.branch_id,
        agents: vec![],
        driver: None,
        run_token: None,
        warning,
    })
}

/// The run `spec` describes admitted by the layer, begun ([`runner::begin_run`]) and metered:
/// the layer's warning, the run and its first branch.
async fn admitted_begin(
    state: &Arc<AppState>,
    spec: &LaunchSpec<'_>,
) -> Result<(Option<String>, RunId, BranchCtx), RunError> {
    let warning = admit_run(state, spec).await?;
    let (run_id, ctx) = runner::begin_run(state, spec).await?;
    meter_run(state, spec, &run_id, &ctx.branch_id).await;
    Ok((warning, run_id, ctx))
}

/// The layer's admission of a new run: its warning, if any.
async fn admit_run(state: &AppState, spec: &LaunchSpec<'_>) -> Result<Option<String>, RunError> {
    let costly = crate::layer::Costly::StartRun;
    Ok(crate::layer::admit(&state.store, &spec.scope, costly).await?)
}

/// Meters a run begun.
async fn meter_run(state: &AppState, spec: &LaunchSpec<'_>, run_id: &str, branch_id: &str) {
    let usage = runs_usage(run_id, branch_id);
    crate::layer::meter(&state.store, &spec.scope, usage).await;
}

/// One run started, for the layer's `runs` meter (keyed by the run, so it counts once).
fn runs_usage<'a>(run_id: &'a str, branch_id: &'a str) -> crate::layer::Usage<'a> {
    crate::layer::Usage {
        meter: crate::layer::Meter::Runs,
        quantity: 1,
        key: format!("run:{run_id}"),
        run: Some(run_id),
        branch: Some(branch_id),
        paid_by: crate::layer::KeySource::Environment,
    }
}
