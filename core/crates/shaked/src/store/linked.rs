//! The events a sandbox's worker sends, each with its own counter `n` (`events.link_n`): appended
//! once, and found again when the worker sends it again, also after the control box restarted.

use anyhow::Result;
use serde_json::Value;

use super::{event_from, Scoped, EVENT_COLS};
use crate::{model::*, overhead::Overhead};

/// An event a worker sent as its `n`.
pub struct Linked {
    pub n: u64,
    pub agent: Option<String>,
    pub kind: EventKind,
    pub payload: Value,
    pub overhead: Option<Overhead>,
}

impl Scoped<'_> {
    /// Appends the worker's event `ev` to `branch_id` (with its overhead, as `append_with` does),
    /// in one statement: when the branch has an event of that `n` already, that event, unchanged.
    pub async fn append_linked(&self, branch_id: &str, ev: Linked) -> Result<Event> {
        let payload = super::with_overhead(ev.payload, ev.overhead)?;
        let n = i64::try_from(ev.n)?;
        let sql = format!(
            "WITH added AS (
                 INSERT INTO events (branch_id, seq, agent, kind, payload, link_n)
                 SELECT $1, coalesce(max(seq), 0) + 1, $2, $3, $4, $5 FROM events WHERE branch_id = $1
                 ON CONFLICT (branch_id, link_n) DO NOTHING
                 RETURNING {EVENT_COLS})
             SELECT {EVENT_COLS}, true AS added FROM added
             UNION ALL
             SELECT {EVENT_COLS}, false FROM events WHERE branch_id = $1 AND link_n = $5"
        );
        let lock = self.store.lock_for(branch_id);
        let _g = lock.lock().await;
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 5] = [
            &branch_id,
            &ev.agent.as_deref(),
            &ev.kind.as_str(),
            &payload,
            &n,
        ];
        let row = self.query_one(&sql, &params).await?;
        let event = event_from(&row)?;
        if row.get::<_, bool>("added") {
            let _ = self.store.tx.send(event.clone()); // no subscribers is fine
        }
        Ok(event)
    }

    /// The largest `n` of the worker events `branch_id` has: 0 before the first.
    pub async fn last_link_n(&self, branch_id: &str) -> Result<u64> {
        let sql = "SELECT coalesce(max(link_n), 0) FROM events WHERE branch_id = $1";
        let row = self.query_one(sql, &[&branch_id]).await?;
        Ok(u64::try_from(row.get::<_, i64>(0))?)
    }
}
