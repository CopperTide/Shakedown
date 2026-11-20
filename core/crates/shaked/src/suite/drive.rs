//! Driving a suite's runs. With a built-in runner shaked runs them one after another: it claims
//! the next slot, starts its run, waits for it to end, and goes on. With `external`, the external
//! system claims each next slot and shaked starts an external run for it, which the system then
//! drives through the external-run API.

use crate::auth::Caller;
use std::{sync::Arc, time::Duration};

use anyhow::Result;

use crate::scenario::Scenario;
use serde::Serialize;

use super::{
    slots::{self, SlotOutcome},
    Slot, Suite,
};
use crate::{
    app::AppState,
    external::Started,
    launch::{launch, LaunchSpec},
    model::{BranchStatus, Rules, Runner},
    runner::RunError,
};

/// How often a built-in suite looks whether its current run has ended.
const POLL: Duration = Duration::from_millis(200);

pub(crate) fn shutting_down(state: &AppState) -> bool {
    *state.shutdown.borrow()
}

/// Runs a built-in suite's runs one after another on a task of its own, then records that it
/// ended (with why, if it stopped early). On shutdown it stops; the next start records why.
pub fn spawn_builtin(state: &Arc<AppState>, scope: Arc<Caller>, suite: Suite) {
    let state = state.clone();
    tokio::spawn(async move {
        let stopped = drive_builtin(&state, &scope, &suite).await.err();
        if shutting_down(&state) {
            return;
        }
        let why = stopped.map(|e| format!("{e:#}"));
        if let Err(e) = state
            .store
            .at(&scope)
            .end_suite(&suite.id, why.as_deref())
            .await
        {
            eprintln!("suite {}: could not record its end: {e:#}", suite.id);
        }
    });
}

/// Claims and runs each slot in turn until all are done or shaked shuts down; an error stops
/// the suite.
async fn drive_builtin(state: &Arc<AppState>, scope: &Arc<Caller>, suite: &Suite) -> Result<()> {
    loop {
        if shutting_down(state) {
            return Ok(());
        }
        let Some(slot) = state.store.at(scope).claim_slot(&suite.id).await? else {
            return Ok(());
        };
        if let Some(branch) = start_builtin(state, scope, suite, &slot).await? {
            wait_ended(state, scope, &branch).await?;
        }
    }
}

/// Starts the slot's run (waiting out a cleanup): its branch, or None when its setup failed
/// (recorded on the slot). Saves over the limit stop the suite.
pub(crate) async fn start_builtin(
    state: &Arc<AppState>,
    scope: &Arc<Caller>,
    suite: &Suite,
    slot: &Slot,
) -> Result<Option<String>> {
    let _permit = slots::permit(state, scope, suite, slot).await?;
    let rules = Rules::new(suite.runner).with_max_cost(suite.max_cost_usd);
    let scenario = suite.slot_scenario(slot);
    let start = launch(state, slot_spec(scope, suite, slot, &scenario, rules));
    match slots::run_slot(state, scope, suite, slot, start).await? {
        SlotOutcome::Filled(l) => Ok(Some(l.branch_id)),
        SlotOutcome::Failed(_) => Ok(None),
    }
}

/// The run of `suite`'s `slot` in `scenario` (`Suite::slot_scenario`) under `rules` (with the
/// suite's world time window): its task, the suite's team.
pub(crate) fn slot_spec<'a>(
    scope: &Arc<Caller>,
    suite: &'a Suite,
    slot: &'a Slot,
    scenario: &'a Scenario,
    rules: Rules,
) -> LaunchSpec<'a> {
    LaunchSpec {
        scope: scope.clone(),
        task_id: &slot.task_id,
        team: suite.team.clone(),
        scenario,
        rules: rules.with_world_minutes(suite.world_minutes),
        driver: None,
        benchmark: Some(&suite.benchmark),
    }
}

/// Waits until the branch is no longer running (or shaked shuts down).
pub(crate) async fn wait_ended(state: &AppState, scope: &Caller, branch: &str) -> Result<()> {
    while !shutting_down(state) {
        match state.store.at(scope).find_branch(branch).await? {
            Some(b) if b.status == BranchStatus::Running => tokio::time::sleep(POLL).await,
            _ => return Ok(()),
        }
    }
    Ok(())
}

/// The slot an external system claimed and the run shaked started for it.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalClaim {
    pub task_id: String,
    /// Which of the suite's variants the run starts in (0 when it has none).
    pub variant: i32,
    pub repeat: i32,
    #[serde(flatten)]
    pub started: Started,
}

/// Claims the external suite's next slot and starts its run: None when every slot is claimed. A
/// run that could not start is recorded on its slot, and the error returned.
pub async fn claim_external(
    state: &Arc<AppState>,
    scope: &Arc<Caller>,
    suite: &Suite,
) -> Result<Option<ExternalClaim>, RunError> {
    let Some(slot) = state.store.at(scope).claim_slot(&suite.id).await? else {
        return Ok(None);
    };
    let (rules, scenario) = (Rules::new(Runner::External), suite.slot_scenario(&slot));
    let start = launch(state, slot_spec(scope, suite, &slot, &scenario, rules));
    match slots::run_slot(state, scope, suite, &slot, start).await? {
        SlotOutcome::Filled(l) => Ok(Some(ExternalClaim {
            task_id: slot.task_id,
            variant: slot.variant,
            repeat: slot.repeat,
            started: Started::of(l.run_id, l.branch_id, l.agents),
        })),
        SlotOutcome::Failed(e) => Err(e),
    }
}
