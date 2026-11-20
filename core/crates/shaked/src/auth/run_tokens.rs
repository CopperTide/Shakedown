//! Run, agent, worker and ticket tokens: short-lived, bound to one branch (and an agent or a
//! sandbox), stored as their hash in `shakedown.run_tokens`. A run token registers agents,
//! reports and finishes; a worker token is a sandbox's link for its branch.

use anyhow::Result;
use uuid::Uuid;

use super::{Principal, RunTokenKind, SystemScope};
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

const LOOKUP: &str =
    "SELECT org_id, run_id, branch_id, kind, agent, sandbox_id FROM shakedown.run_tokens
    WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()";

/// The branch-bound principal an `sdr_` or `sdw_` bearer token is; None when it is unknown,
/// revoked or expired.
pub async fn resolve_run_token(store: &Store, bearer: &str) -> Result<Option<Principal>> {
    if !(bearer.starts_with("sdr_") || bearer.starts_with("sdw_")) {
        return Ok(None);
    }
    let tx = store.system_tx(&SystemScope::resolve_token()).await?;
    let row = tx
        .query_opt(LOOKUP, &[&super::tokens::hash(bearer)])
        .await?;
    tx.commit().await?;
    Ok(row.and_then(|r| {
        Some(Principal::RunToken {
            org: r.get(0),
            run: r.get(1),
            branch: r.get(2),
            kind: kind_of(r.get::<_, &str>(3))?,
            agent: r.get(4),
            sandbox: r.get::<_, Option<Uuid>>(5),
        })
    }))
}
