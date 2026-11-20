//! Background jobs that span every org, on the store's system connections.

use anyhow::Result;

use crate::store::Store;

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
    store.system_db().await?.batch_execute(INTERRUPT).await?;
    Ok(())
}
