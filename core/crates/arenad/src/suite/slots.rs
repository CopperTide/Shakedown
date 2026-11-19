//! Running a suite's claimed slot: waiting out a cleanup and the saves limit, starting the slot's
//! run, and recording on the slot how that went (the run that filled it, or why it couldn't
//! start).

use std::future::Future;

use anyhow::{anyhow, Result};
use tokio::sync::OwnedRwLockReadGuard;

use super::{Slot, Suite};
use crate::{app::AppState, launch::Launched, runner::RunError};

/// How a slot's run start went, as recorded on the slot.
pub enum SlotOutcome {
    /// The run started; the slot names it.
    Filled(Launched),
    /// The run couldn't start; the slot says why.
    Failed(RunError),
}

/// Held while a slot's run starts, so no cleanup drops a save it is about to use: taken once the
/// cleanup that holds the lock is done. Saves over the limit fail the slot and stop the suite
/// (the error).
pub async fn permit(
    state: &AppState,
    suite: &Suite,
    slot: &Slot,
) -> Result<OwnedRwLockReadGuard<()>> {
    let held = state.cleanup.clone().read_owned().await;
    if state.saves_over_limit().await? {
        let why = crate::api::SAVES_FULL;
        state.store.slot_failed(&suite.id, slot.ord, why).await?;
        return Err(anyhow!(why));
    }
    Ok(held)
}

/// Starts the slot's run with `start` and records how it went on the slot: the run's ids, or why
/// it couldn't start. An error recording it is the error.
pub async fn run_slot(
    state: &AppState,
    suite: &Suite,
    slot: &Slot,
    start: impl Future<Output = Result<Launched, RunError>>,
) -> Result<SlotOutcome> {
    match start.await {
        Ok(l) => {
            let store = &state.store;
            store
                .fill_slot(&suite.id, slot.ord, &l.run_id, &l.branch_id)
                .await?;
            Ok(SlotOutcome::Filled(l))
        }
        Err(e) => {
            let why = format!("{e:#}");
            state.store.slot_failed(&suite.id, slot.ord, &why).await?;
            Ok(SlotOutcome::Failed(e))
        }
    }
}
