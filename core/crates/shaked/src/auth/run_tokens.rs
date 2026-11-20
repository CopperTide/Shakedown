//! Run, agent, worker and ticket tokens: short-lived, bound to one branch (and an agent or a
//! sandbox), stored as their hash in `shakedown.run_tokens`. A run token registers agents,
//! reports and finishes; a worker token is a sandbox's link for its branch.

use anyhow::Result;
use uuid::Uuid;

use super::{Caller, RunTokenKind, SystemScope};
use crate::store::Store;

/// How long a token lives without a run limit of its own: a day (the `local` plan's longest
/// branch, `docs/quotas.toml`).
pub const DEFAULT_TOKEN_SECS: u64 = 86_400;

/// The prefix of each kind's tokens.
pub fn prefix(kind: RunTokenKind) -> &'static str {
    match kind {
        RunTokenKind::Run => "sdr_",
        RunTokenKind::Agent => "sda_",
        RunTokenKind::Worker => "sdw_",
        RunTokenKind::Ticket => "sdt_",
    }
}

fn kind_of(s: &str) -> Option<RunTokenKind> {
    [
        RunTokenKind::Run,
        RunTokenKind::Agent,
        RunTokenKind::Worker,
        RunTokenKind::Ticket,
    ]
    .into_iter()
    .find(|k| k.as_str() == s)
}

const LOOKUP: &str = "SELECT run_id, branch_id, kind, agent, sandbox_id FROM shakedown.run_tokens
    WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()";

/// The branch-bound caller an `sdr_` or `sdw_` bearer token is; None when it is unknown, revoked
/// or expired.
pub async fn resolve_run_token(store: &Store, bearer: &str) -> Result<Option<Caller>> {
    if !(bearer.starts_with("sdr_") || bearer.starts_with("sdw_")) {
        return Ok(None);
    }
    let tx = store.system_tx(SystemScope::resolve_token()).await?;
    let row = tx
        .query_opt(LOOKUP, &[&super::tokens::hash(bearer)])
        .await?;
    tx.commit().await?;
    Ok(row.and_then(|r| {
        let kind = kind_of(r.get::<_, &str>(2))?;
        let c = Caller::run_token(r.get(0), r.get(1), kind, r.get(3));
        Some(match r.get::<_, Option<Uuid>>(4) {
            Some(sb) => c.with_sandbox(sb),
            None => c,
        })
    }))
}
