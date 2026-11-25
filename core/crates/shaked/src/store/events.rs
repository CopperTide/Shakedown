//! A branch's event log: appended under the branch's lock (`max(seq)+1`), broadcast to
//! subscribers, read back in seq order, and copied onto a fork.

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio_postgres::Row;

use super::{Scoped, Store, VISIBLE_BRANCH};
use crate::{auth::SystemScope, model::*, overhead::Overhead};

pub(super) const EVENT_COLS: &str = "branch_id, seq, at, agent, kind, payload";

/// An event's payload without a model call's bodies (`Scoped::events_lean`).
const LEAN_PAYLOAD: &str = "CASE WHEN kind = 'model_call'
    THEN (payload - 'provider_response') || jsonb_build_object(
        'request', CASE WHEN payload->'request'->>'model' IS NOT NULL
            THEN jsonb_build_object('model', payload->'request'->'model') ELSE NULL END,
        'response', NULL, 'bodies_omitted', true)
    ELSE payload END";

impl Store {
    /// Every event that can name a save (`run_start`, `tool_call`, `check`), of every branch (a system job),
    /// by branch and seq.
    pub async fn events_naming_saves(&self) -> Result<Vec<Event>> {
        let kinds =
            [EventKind::RunStart, EventKind::ToolCall, EventKind::Check].map(|k| k.as_str());
        let sql =
            format!("SELECT {EVENT_COLS} FROM events WHERE kind = ANY($1) ORDER BY branch_id, seq");
        let tx = self.system_tx(SystemScope::startup_gc()).await?;
        let rows = tx.query(&sql, &[&kinds.as_slice()]).await?;
        tx.commit().await?;
        rows.iter().map(event_from).collect()
    }
}

impl Scoped<'_> {
    /// Records an event of a branch.
    pub async fn append(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) -> Result<Event> {
        self.append_with(branch_id, agent, kind, payload, None)
            .await
    }

    /// Records an event of a branch; an object payload also gets `overhead`, the Shakedown's time
    /// since the branch's last event, as `overhead_ms`.
    pub async fn append_with(
        &self,
        branch_id: &str,
        agent: Option<&str>,
        kind: EventKind,
        payload: Value,
        overhead: Option<Overhead>,
    ) -> Result<Event> {
        let payload = with_overhead(payload, overhead)?;
        let sql = format!(
            "INSERT INTO events (branch_id, seq, agent, kind, payload)
                     SELECT $1, coalesce(max(seq), 0) + 1, $2, $3, $4 FROM events WHERE branch_id = $1
                     RETURNING {EVENT_COLS}"
        );
        let _g = self.store.lock_for(branch_id).lock_owned().await;
        let row = self
            .query_one(&sql, &[&branch_id, &agent, &kind.as_str(), &payload])
            .await?;
        let ev = event_from(&row)?;
        let _ = self.store.tx.send(ev.clone()); // no subscribers is fine
        Ok(ev)
    }

    pub async fn events(&self, branch_id: &str) -> Result<Vec<Event>> {
        let sql = format!(
            "SELECT {EVENT_COLS} FROM events WHERE branch_id = $1
                        AND EXISTS ({VISIBLE_BRANCH}) ORDER BY seq"
        );
        let rows = self.query(&sql, &[&branch_id]).await?;
        rows.iter().map(event_from).collect()
    }

    /// One full event, scoped to a visible branch, for lazy step inspection.
    pub async fn event(&self, branch_id: &str, seq: i64) -> Result<Option<Event>> {
        let sql = format!(
            "SELECT {EVENT_COLS} FROM events WHERE branch_id = $1 AND seq = $2
                        AND EXISTS ({VISIBLE_BRANCH})"
        );
        self.query(&sql, &[&branch_id, &seq])
            .await?
            .first()
            .map(event_from)
            .transpose()
    }

    /// `events`, retaining `request.model` for pricing but omitting message and response bodies,
    /// which are most of a long run's record. Usage and cost stay; `bodies_omitted` tells the
    /// inspector to load this event in full when selected.
    pub async fn events_lean(&self, branch_id: &str) -> Result<Vec<Event>> {
        let sql = format!(
            "SELECT branch_id, seq, at, agent, kind, {LEAN_PAYLOAD} AS payload FROM events
                        WHERE branch_id = $1 AND EXISTS ({VISIBLE_BRANCH}) ORDER BY seq"
        );
        let rows = self.query(&sql, &[&branch_id]).await?;
        rows.iter().map(event_from).collect()
    }

    /// Incremental stream catch-up; skip already loaded rows and optionally omit model bodies.
    pub async fn events_after(
        &self,
        branch_id: &str,
        after: i64,
        lean: bool,
    ) -> Result<Vec<Event>> {
        let payload = if lean { LEAN_PAYLOAD } else { "payload" };
        let sql = format!(
            "SELECT branch_id, seq, at, agent, kind, {payload} AS payload FROM events
             WHERE branch_id = $1 AND seq > $2 AND EXISTS ({VISIBLE_BRANCH}) ORDER BY seq"
        );
        self.query(&sql, &[&branch_id, &after])
            .await?
            .iter()
            .map(event_from)
            .collect()
    }

    /// `(count, last seq)` of the branch's events: a recorded branch's record is the same while
    /// these are (events are only ever appended).
    pub async fn events_version(&self, branch_id: &str) -> Result<(i64, i64)> {
        let sql = format!(
            "SELECT count(*), coalesce(max(seq), 0) FROM events
                        WHERE branch_id = $1 AND EXISTS ({VISIBLE_BRANCH})"
        );
        let row = self.query_one(&sql, &[&branch_id]).await?;
        Ok((row.get(0), row.get(1)))
    }

    /// Copies `from`'s events with `seq < before_seq` onto `to`, keeping their seqs.
    pub async fn copy_prefix(
        &self,
        from_branch: &str,
        to_branch: &str,
        before_seq: i64,
    ) -> Result<()> {
        let _g = self.store.lock_for(to_branch).lock_owned().await;
        self.execute(
            "INSERT INTO events (branch_id, seq, at, agent, kind, payload)
                 SELECT $2, seq, at, agent, kind, payload FROM events
                 WHERE branch_id = $1 AND seq < $3",
            &[&from_branch, &to_branch, &before_seq],
        )
        .await?;
        Ok(())
    }
}

/// `payload` with `overhead` as its `overhead_ms`, when it is an object.
pub(crate) fn with_overhead(mut payload: Value, overhead: Option<Overhead>) -> Result<Value> {
    if let (Some(o), Some(obj)) = (overhead, payload.as_object_mut()) {
        obj.insert("overhead_ms".into(), serde_json::to_value(o)?);
    }
    Ok(payload)
}

pub(super) fn event_from(r: &Row) -> Result<Event> {
    let kind: String = r.get("kind");
    Ok(Event {
        branch_id: r.get("branch_id"),
        seq: r.get("seq"),
        at: r.get("at"),
        agent: r.get("agent"),
        kind: EventKind::parse(&kind).ok_or_else(|| anyhow!("unknown event kind {kind}"))?,
        payload: r.get("payload"),
    })
}
