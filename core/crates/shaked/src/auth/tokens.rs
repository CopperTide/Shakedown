//! Secrets with a prefix per kind (`sd_` API, `sdr_` run, `sda_` agent, `sdw_` worker, `sdt_` ticket)
//! and 64 hex characters; only their sha256 is stored. API tokens resolve to their holder, capped by
//! their scopes, through one fixed query (a system job) on the hash.

use anyhow::Result;
use ring::rand::{SecureRandom, SystemRandom};
use uuid::Uuid;

use super::{identity, OrgRole, Principal, SystemScope, TokenScope};
use crate::store::Store;

pub const API_PREFIX: &str = "sd_";

/// A token just made: shown once, then only its prefix.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Minted {
    pub id: Uuid,
    pub token: String,
    pub prefix: String,
}

/// `prefix` and 32 random bytes in hex.
pub fn new_secret(prefix: &'static str) -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system's random source works");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{prefix}{hex}")
}

/// The sha256 of `token`, in hex: what the database keeps.
pub fn hash(token: &str) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, token.as_bytes());
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The first 8 characters, shown in lists.
pub fn prefix_of(token: &str) -> String {
    token.chars().take(8).collect()
}

const LOOKUP: &str = "SELECT t.id, t.org_id, t.user_id, t.service_account_id, t.team_id, t.scopes
     FROM shakedown.api_tokens t JOIN identity.orgs o ON o.id = t.org_id AND o.deleted_at IS NULL
    WHERE t.token_hash = $1 AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at > now())";
const TOUCH: &str = "UPDATE shakedown.api_tokens SET last_used_at = now()
    WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')";
const SERVICE_ACCOUNT: &str =
    "SELECT role, team_id FROM identity.service_accounts WHERE id = $1 AND org_id = $2 AND disabled_at IS NULL";

/// The principal an `sd_` bearer token acts as; None when it is unknown, revoked, expired, of a
/// deleted org, or its holder left the org or was disabled. Marks it used (at most once a minute).
pub async fn resolve_api_token(store: &Store, bearer: &str) -> Result<Option<Principal>> {
    if !bearer.starts_with(API_PREFIX) {
        return Ok(None);
    }
    let tx = store.system_tx(&SystemScope::resolve_token()).await?;
    let Some(row) = tx.query_opt(LOOKUP, &[&hash(bearer)]).await? else {
        return Ok(None);
    };
    let (id, org): (Uuid, Uuid) = (row.get(0), row.get(1));
    let scopes: Vec<String> = row.get(5);
    let scopes = scopes.iter().filter_map(|s| TokenScope::parse(s)).collect();
    let team: Option<Uuid> = row.get(4);
    let holder = match (row.get::<_, Option<Uuid>>(2), row.get::<_, Option<Uuid>>(3)) {
        (Some(user), _) => identity::principal_of_user(store, user, org).await?,
        (None, Some(sa)) => service_account(&tx, sa, org).await?,
        (None, None) => None,
    };
    tx.execute(TOUCH, &[&id]).await?;
    tx.commit().await?;
    Ok(holder.map(|h| Principal::ApiToken {
        token: id,
        holder: Box::new(h),
        scopes,
        team,
    }))
}

async fn service_account(tx: &crate::store::Tx, sa: Uuid, org: Uuid) -> Result<Option<Principal>> {
    let Some(r) = tx.query_opt(SERVICE_ACCOUNT, &[&sa, &org]).await? else {
        return Ok(None);
    };
    let role = OrgRole::parse(r.get::<_, &str>(0)).unwrap_or(OrgRole::Viewer);
    Ok(Some(Principal::ServiceAccount {
        sa,
        org,
        role,
        team: r.get(1),
    }))
}
