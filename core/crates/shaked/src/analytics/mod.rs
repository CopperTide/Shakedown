//! Analytics across runs. Each ended branch's figures are worked out once from its record
//! ([`stats`]) and kept on the branch by a background job ([`every`]); the summary the Analytics
//! page and `analytics.summary` show aggregates those small rows in SQL, as the caller
//! (`store/analytics.rs`), filtered by a [`Filter`].
//!
//! ```text
//!  branch ends ──▶ analytics job (system tx) ──▶ branches.stats ──▶ analytics.summary (caller tx)
//! ```
//!
//! Reading the events at query time instead takes seconds on a long run's record (its model
//! calls' bodies are most of it), so the figures are kept.

mod failures;
pub mod filter;
pub mod stats;

use std::{sync::Arc, time::Duration};

use anyhow::Result;

pub use filter::{Filter, Group, Query};
pub use stats::{stats_of, BranchStats};

use crate::{app::AppState, store::Store};

/// How many branches one pass of the job works out.
pub const BATCH: i64 = 20;
/// How long the job waits once it has caught up.
const IDLE: Duration = Duration::from_secs(5);

/// Works out and keeps the figures of up to `limit` ended branches that lack current ones (a
/// system job): how many it kept. A branch whose record cannot be read is marked so, not retried.
pub async fn fill(store: &Store, limit: i64) -> Result<usize> {
    let mut kept = 0;
    for id in store.unfilled_stats(limit).await? {
        let figures = match store.stats_record(&id).await {
            Ok(Some(rec)) => {
                let s = stats_of(&rec.branch, &rec.task_id, &rec.events);
                Some((rec.branch.ended_at, serde_json::to_value(s)?))
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("analytics: branch {id}: {e:#}");
                store.mark_stats_unreadable(&id, &format!("{e:#}")).await?;
                None
            }
        };
        if let Some((Some(ended_at), value)) = figures {
            if store.put_stats(&id, ended_at, &value).await? {
                kept += 1;
            }
        }
    }
    Ok(kept)
}

/// Keeps every ended branch's figures up to date until shaked stops: a full batch goes straight on
/// to the next, else it waits [`IDLE`].
pub async fn every(state: Arc<AppState>) {
    loop {
        let wait = match fill(&state.store, BATCH).await {
            Ok(n) if n as i64 >= BATCH => Duration::from_millis(50),
            Ok(_) => IDLE,
            Err(e) => {
                eprintln!("analytics: {e:#}");
                IDLE * 6
            }
        };
        tokio::time::sleep(wait).await;
    }
}
