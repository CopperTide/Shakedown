//! Running a suite's claimed slot: waiting out a cleanup and the saves limit, starting the slot's
//! run, and recording on the slot how that went (the run that filled it, or why it couldn't
//! start).

use crate::auth::Caller;
use std::future::Future;

use anyhow::{anyhow, Result};
use tokio::sync::OwnedRwLockReadGuard;

use super::{Slot, Suite};
use crate::{app::AppState, launch::Launched, runner::RunError};

/// The world seed of a suite's repeat `repeat` when the suite has the base seed `base`:
/// splitmix64 of `base + repeat`, at most `runner::MAX_SEED`. Every setup of an experiment has
/// the same base, so run i of each starts from the same seed.
pub fn paired_seed(base: u64, repeat: i32) -> u64 {
    let mut z = base
        .wrapping_add(repeat as u64)
        .wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) & crate::runner::MAX_SEED
}

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
    scope: &Caller,
    suite: &Suite,
    slot: &Slot,
) -> Result<OwnedRwLockReadGuard<()>> {
    let held = state.cleanup.clone().read_owned().await;
    if state.saves_over_limit().await? {
        let why = crate::api::SAVES_FULL;
        state
            .store
            .at(scope)
            .slot_failed(&suite.id, slot.ord, why)
            .await?;
        return Err(anyhow!(why));
    }
    Ok(held)
}

/// Starts the slot's run with `start` and records how it went on the slot: the run's ids, or why
/// it couldn't start. An error recording it is the error.
pub async fn run_slot(
    state: &AppState,
    scope: &Caller,
    suite: &Suite,
    slot: &Slot,
    start: impl Future<Output = Result<Launched, RunError>>,
) -> Result<SlotOutcome> {
    match start.await {
        Ok(l) => {
            state
                .store
                .at(scope)
                .fill_slot(&suite.id, slot.ord, &l.run_id, &l.branch_id)
                .await?;
            Ok(SlotOutcome::Filled(l))
        }
        Err(e) => {
            let why = format!("{e:#}");
            state
                .store
                .at(scope)
                .slot_failed(&suite.id, slot.ord, &why)
                .await?;
            Ok(SlotOutcome::Failed(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::MAX_SEED;

    #[test]
    fn paired_seed_is_stable_and_below_2_53() {
        assert_eq!(paired_seed(7, 1), paired_seed(7, 1));
        let seeds: std::collections::BTreeSet<u64> = (0..100).map(|r| paired_seed(7, r)).collect();
        assert_eq!(seeds.len(), 100, "every repeat its own seed");
        assert!(seeds.iter().all(|s| *s <= MAX_SEED));
        assert_ne!(paired_seed(7, 1), paired_seed(8, 1));
    }
}
