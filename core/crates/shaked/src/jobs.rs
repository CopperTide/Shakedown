//! Background jobs without a caller: system transactions (`Store::system_tx`).

use anyhow::Result;

use crate::{auth::SystemScope, store::Store};

/// What was running when shaked last stopped won't go on: branches are interrupted, built-in
/// suites and experiments get no more runs, and reviews won't finish.
const INTERRUPT: &str = "
UPDATE branches SET status = 'interrupted', ended_at = now() WHERE status = 'running';
UPDATE suites SET ended_at = now(), error = 'shaked stopped before the suite finished'
 WHERE (runner <> 'external' OR connection IS NOT NULL) AND ended_at IS NULL;
UPDATE experiments SET ended_at = now(), error = 'shaked stopped before the experiment finished'
 WHERE ended_at IS NULL;
UPDATE reviews SET status = 'error', finished_at = now(), error = 'shaked stopped before the review finished'
 WHERE status = 'running';";

/// Marks what was running when shaked last stopped as interrupted (at start, before any run).
pub async fn interrupt_at_start(store: &Store) -> Result<()> {
    let tx = store.system_tx(SystemScope::interrupt_at_start()).await?;
    tx.batch(INTERRUPT).await?;
    tx.commit().await
}
