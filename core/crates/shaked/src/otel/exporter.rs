//! The exporter: every few seconds, the branches that ended and are queued for a target are sent
//! there, from the stored record (so it runs on the machine that keeps the records, never in a
//! sandbox). A send that fails is tried again later ([`crate::store::backoff`]); each try is
//! recorded on the export's row. Spans keep their ids from one try to the next, so a collector
//! that got part of a run before sees the same spans again, not new ones.

use std::{sync::Arc, time::Duration};

use super::{encode, prom, proto, send};
use crate::{
    app::AppState,
    auth::Caller,
    store::{Due, Tried},
};

/// How often the queue is looked at.
pub const EVERY: Duration = Duration::from_secs(5);
/// The most exports one look sends.
const PER_TICK: i64 = 16;

/// How often OTLP metrics are pushed to the targets that take them.
pub const METRICS_EVERY: Duration = Duration::from_secs(60);

/// Works the queue, and pushes metrics, until shaked stops.
pub async fn every(state: Arc<AppState>) {
    let mut pushed: Option<std::time::Instant> = None;
    loop {
        if let Err(e) = tick(&state).await {
            eprintln!("observability: {e:#}");
        }
        if pushed.is_none_or(|p| p.elapsed() >= METRICS_EVERY) {
            pushed = Some(std::time::Instant::now());
            if let Err(e) = push_metrics(&state).await {
                eprintln!("observability: metrics: {e:#}");
            }
        }
        tokio::time::sleep(EVERY).await;
    }
}

/// Pushes each metrics target its owner's metrics (OTLP, cumulative since the first branch it
/// counts): how many targets took them.
pub async fn push_metrics(state: &AppState) -> anyhow::Result<usize> {
    let vault = state.vault().map_err(anyhow::Error::msg)?;
    let mut sent = 0;
    for t in state.store.metric_targets(&vault).await? {
        let snap = state.store.target_snapshot(t.row.id).await?;
        let first = snap.branches.iter().map(|b| b.created_at).min();
        let now = chrono::Utc::now();
        let (start, now) = (encode::nanos(first.unwrap_or(now)), encode::nanos(now));
        let body = proto::bytes_of(&prom::otlp(&prom::families(&snap), start, now));
        let url = super::targets::metrics_url(&t.row.endpoint);
        match send::post(&state.http, &t, &url, body).await {
            Ok(()) => sent += 1,
            Err(why) => eprintln!("observability: metrics to {}: {why}", t.row.name),
        }
    }
    Ok(sent)
}

/// Sends what is due now: how many exports were tried.
pub async fn tick(state: &AppState) -> anyhow::Result<usize> {
    let due = state.store.due_exports(PER_TICK).await?;
    if due.is_empty() {
        return Ok(0);
    }
    let vault = state.vault().map_err(anyhow::Error::msg)?;
    for d in &due {
        let tried = match state.store.due_parts(&vault, d).await {
            Ok(Some((target, record))) => {
                outcome(send::export(&state.http, &target, &record).await)
            }
            Ok(None) => continue,
            Err(e) => Tried::Failed(format!("{e:#}")),
        };
        state.store.record_due(d, &tried).await?;
    }
    Ok(due.len())
}

fn outcome(sent: Result<i32, String>) -> Tried {
    match sent {
        Ok(n) => Tried::Sent(n),
        Err(why) => Tried::Failed(why),
    }
}

/// How an export someone asked for went, per target.
#[derive(Debug, serde::Serialize)]
pub struct Asked {
    pub target_id: uuid::Uuid,
    pub target: String,
    /// `done`, `failed` (tried again later) or `queued` (the branch has not ended).
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Sends `record` to `t` now for `records`' caller, when it has ended: how it went.
async fn send_one(
    state: &AppState,
    records: crate::store::Scoped<'_>,
    t: &crate::store::OpenTarget,
    record: &crate::store::RunRecord,
) -> anyhow::Result<Asked> {
    let mut a = Asked {
        target_id: t.row.id,
        target: t.row.name.clone(),
        state: "queued",
        spans: None,
        error: None,
    };
    if record.branch.status.goes_on() {
        return Ok(a);
    }
    let tried = outcome(send::export(&state.http, t, record).await);
    let due = Due {
        target_id: t.row.id,
        branch_id: record.branch.id.clone(),
        attempts: 0,
    };
    records.record_export(&due, &tried).await?;
    (a.state, a.spans, a.error) = match tried {
        Tried::Sent(n) => ("done", Some(n), None),
        Tried::Failed(why) => ("failed", None, Some(why)),
    };
    Ok(a)
}

/// Exports branch `id` to target `target` (every target when None) for `caller` now, when the
/// branch has ended (else it is queued for when it does); None when the caller sees no such
/// branch.
pub async fn export_now(
    state: &AppState,
    caller: &Caller,
    id: &str,
    target: Option<uuid::Uuid>,
) -> anyhow::Result<Option<Vec<Asked>>> {
    let records = state.store.at(caller);
    let Some(record) = records.run_record(id).await? else {
        return Ok(None);
    };
    records.ask_export(id, target).await?;
    let vault = state.vault().map_err(anyhow::Error::msg)?;
    let targets = records.open_targets(&vault).await?;
    let chosen = targets
        .iter()
        .filter(|t| target.is_none_or(|w| w == t.row.id));
    let mut out = vec![];
    for t in chosen {
        out.push(send_one(state, records, t, &record).await?);
    }
    Ok(Some(out))
}
