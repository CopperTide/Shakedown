//! Driving an experiment: one run at a time, in `run_order`, on a task of its own. Before each
//! run it looks whether the experiment was stopped or shaked is shutting down; a suite ends when
//! its last run did, the experiment when every run did.

use crate::auth::Caller;
use std::sync::Arc;

use anyhow::Result;

use super::{run_order, STOPPED};
use crate::{
    app::AppState,
    suite::{self, shutting_down, Suite},
};

/// Drives the experiment `id` over its `suites` (in setup order) on a task of its own, then
/// records that it ended (with why, if it stopped early). On shutdown it stops; the next start
/// records why.
pub fn spawn(state: &Arc<AppState>, scope: Arc<Caller>, id: String, suites: Vec<Suite>) {
    let state = state.clone();
    tokio::spawn(async move {
        let stopped = drive(&state, &scope, &id, &suites).await.err();
        if shutting_down(&state) {
            return;
        }
        let why = stopped.map(|e| format!("{e:#}"));
        if let Err(e) = state
            .store
            .at(&scope)
            .end_experiment(&id, why.as_deref())
            .await
        {
            eprintln!("experiment {id}: could not record its end: {e:#}");
        }
    });
}

/// Makes each run in turn until all are done, the experiment is stopped or shaked shuts down;
/// an error stops the experiment.
async fn drive(
    state: &Arc<AppState>,
    scope: &Arc<Caller>,
    id: &str,
    suites: &[Suite],
) -> Result<()> {
    let runs: Vec<i32> = suites.iter().map(|s| s.repeats).collect();
    let records = state.store.at(scope);
    for (setup, run) in run_order(&runs) {
        if shutting_down(state) || records.experiment_ended(id).await? {
            return Ok(());
        }
        let suite = &suites[setup];
        suite::run_next(state, scope, suite, Some(id)).await?;
        if run + 1 == suite.repeats {
            records.end_suite(&suite.id, None).await?;
        }
    }
    Ok(())
}

/// Stops the experiment: no further run starts, its suites end, and the run that is running now
/// (or queued for a cloud slot) is stopped. Stopping one that ended changes nothing.
pub async fn stop(state: &AppState, scope: &Caller, id: &str) -> Result<()> {
    let records = state.store.at(scope);
    records.end_experiment(id, Some(STOPPED)).await?;
    for s in records.experiment_suites(id).await? {
        for slot in records.slots(&s.id).await? {
            if let Some(branch) = &slot.branch_id {
                state.live.stop(state, branch).await;
                crate::hosting::stop_queued(state, scope, branch).await?;
            }
        }
    }
    Ok(())
}
