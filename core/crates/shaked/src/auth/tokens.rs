//! Secrets with a prefix per kind (`sd_` API, `sdr_` run, `sda_` agent, `sdw_` worker, `sdt_` ticket)
//! and 64 hex characters; only their sha256 is stored. An API token resolves to a caller capped by
//! its scopes, through one fixed query (a system job) on the hash.

use anyhow::Result;
use ring::rand::{SecureRandom, SystemRandom};
use uuid::Uuid;

use super::{Caller, SystemJob, TokenScope};
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

const LOOKUP: &str = "SELECT id, scopes FROM shakedown.api_tokens
    WHERE token_hash = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())";
const TOUCH: &str = "UPDATE shakedown.api_tokens SET last_used_at = now()
    WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')";

/// The caller an `sd_` bearer token is; None when it is unknown, revoked or expired. Marks it used
/// (at most once a minute).
pub async fn resolve_api_token(store: &Store, bearer: &str) -> Result<Option<Caller>> {
    if !bearer.starts_with(API_PREFIX) {
        return Ok(None);
    }
    let tx = store.system_tx(SystemJob::ResolveToken).await?;
    let Some(row) = tx.query_opt(LOOKUP, &[&hash(bearer)]).await? else {
        return Ok(None);
    };
    let id: Uuid = row.get(0);
    let scopes: Vec<String> = row.get(1);
    let scopes = scopes.iter().filter_map(|s| TokenScope::parse(s)).collect();
    tx.execute(TOUCH, &[&id]).await?;
    tx.commit().await?;
    Ok(Some(Caller::api_token(id, scopes)))
}

/// `text` with every token shaked makes (`sd_`, `sdr_`, `sda_`, `sdw_`, `sdt_` and 64 hex digits)
/// replaced by its prefix and `••••`: what a driver's output or error is recorded as.
pub fn redact_tokens(text: &str) -> String {
    const PREFIXES: [&str; 5] = ["sd_", "sdr_", "sda_", "sdw_", "sdt_"];
    let (mut out, mut rest) = (String::with_capacity(text.len()), text);
    while let Some(i) = rest.find("sd") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let token = PREFIXES.iter().find_map(|p| {
            let hex = rest.strip_prefix(p)?.get(..64)?;
            hex.bytes()
                .all(|b| b.is_ascii_hexdigit())
                .then_some(p.len() + 64)
                .map(|n| (*p, n))
        });
        match token {
            Some((prefix, n)) => {
                out.push_str(prefix);
                out.push_str("••••");
                rest = &rest[n..];
            }
            None => {
                out.push_str("sd");
                rest = &rest[2..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_in_text_are_redacted_and_the_rest_kept() {
        let t = new_secret("sda_");
        let r = new_secret("sdr_");
        let text = format!(
            "Authorization: Bearer {t}\nrun {r} ok; sd_short and sdx_{} stay",
            "0".repeat(64)
        );
        let got = redact_tokens(&text);
        assert!(!got.contains(&t[4..]) && !got.contains(&r[4..]), "{got}");
        assert!(
            got.contains("Bearer sda_••••") && got.contains("run sdr_•••• ok"),
            "{got}"
        );
        assert!(got.contains("sd_short") && got.contains("sdx_"), "{got}");
    }
}
