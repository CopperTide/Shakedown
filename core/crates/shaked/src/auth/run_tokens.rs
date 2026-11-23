//! Run, agent, worker and ticket tokens: short-lived, bound to one branch (and an agent or a
//! sandbox), stored as their hash in `shakedown.run_tokens`. A run token registers agents,
//! reports and finishes; a worker token is a sandbox's link for its branch.

use anyhow::Result;
use uuid::Uuid;

use super::{
    engine_check, Action, Caller, CallerKind, Decision, RunTokenKind, SystemScope, Target,
};
use crate::store::Store;

/// How long a token lives: a day, longer than a run is expected to take (a local shaked sets no
/// limit on a branch's length).
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

/// A run token `t` of the branch `b` is live: not revoked, and not past its expiry counted in
/// unpaused time (a pause in progress since `b.paused_at` doesn't count against it, as its resume
/// extends the token by the pause). The one definition every query of a token's liveness splices
/// in, with those aliases.
macro_rules! token_live {
    () => {
        "t.revoked_at IS NULL AND t.expires_at + coalesce(now() - b.paused_at, interval '0') > now()"
    };
}
pub(crate) use token_live;

const LOOKUP: &str = concat!(
    "SELECT t.run_id, t.branch_id, t.kind, t.agent, t.sandbox_id FROM shakedown.run_tokens t
       LEFT JOIN shakedown.branches b ON b.id = t.branch_id
     WHERE t.token_hash = $1 AND ",
    token_live!()
);

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

/// The caller a worker token is, when it is live and may do `action` on `branch` of its run
/// (None: the token's own branch); None for any other token, or a branch it may not.
pub async fn worker_caller(
    store: &Store,
    token: &str,
    action: Action,
    branch: Option<&str>,
) -> Result<Option<Caller>> {
    let Some(c) = resolve_run_token(store, token).await? else {
        return Ok(None);
    };
    let CallerKind::RunToken {
        run,
        branch: own,
        kind: RunTokenKind::Worker,
        ..
    } = &c.kind
    else {
        return Ok(None);
    };
    let on = Target::Branch {
        run,
        branch: branch.unwrap_or(own),
    };
    let allowed = engine_check(&c, &action, &on) == Decision::Allow;
    Ok(allowed.then_some(c))
}
