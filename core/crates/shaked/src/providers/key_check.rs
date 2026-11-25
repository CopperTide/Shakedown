//! Checking a key before it is stored: its name, kind, secret and server; and testing a stored
//! key with a one-token call, recording the outcome.

use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    bedrock,
    keys::{apply, names_its_server, provider_id},
    listing, reach, Provider,
};
use crate::{
    app::AppState,
    auth::Caller,
    store::{KeyKind, KeyRow, OpenKey},
};

/// What a key to add looks like before it is sealed: its kind, and what lists show of it.
pub struct Checked {
    pub kind: KeyKind,
    pub last4: String,
}

/// The last four characters of `s`.
fn last4(s: &str) -> String {
    let n = s.chars().count();
    s.chars().skip(n.saturating_sub(4)).collect()
}

/// A key name: 1 to 60 of lowercase letters, digits, `-` and `_` (it becomes `custom:<name>`).
fn valid_name(name: &str) -> bool {
    (1..=60).contains(&name.len())
        && (name.bytes())
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Checks a key about to be added (or a secret about to replace one's), against the layer's
/// `policy` for its server.
pub async fn check_new(
    name: &str,
    kind: &str,
    secret: &str,
    base_url: Option<&str>,
    policy: &crate::layer::Policy,
) -> Result<Checked, String> {
    if !valid_name(name) {
        return Err("name: 1 to 60 of lowercase letters, digits, - and _".into());
    }
    let kinds = "anthropic, openai, openrouter, bedrock or custom";
    let kind = KeyKind::parse(kind).ok_or_else(|| format!("kind: one of {kinds}"))?;
    if base_url.is_some_and(|u| u.len() > 500) {
        return Err("base_url: at most 500 characters".into());
    }
    let last4 = check_secret(kind, secret)?;
    match (kind, base_url) {
        (KeyKind::Bedrock, Some(region)) if bedrock::valid_region(region) => {}
        (KeyKind::Bedrock, _) => {
            return Err("a Bedrock key needs its region in base_url (for example us-east-1)".into())
        }
        (KeyKind::Custom, None) => return Err("a custom server needs its URL in base_url".into()),
        (_, Some(url)) => reach::reach(url, policy).await?,
        (_, None) => {}
    }
    Ok(Checked { kind, last4 })
}

/// What lists show of `secret`, once it is the right shape for `kind`.
pub fn check_secret(kind: KeyKind, secret: &str) -> Result<String, String> {
    match kind {
        KeyKind::Bedrock => {
            bedrock::credentials::from_json(secret)?;
            let v: Value = serde_json::from_str(secret).unwrap_or_default();
            Ok(last4(v["access_key_id"].as_str().unwrap_or_default()))
        }
        KeyKind::Custom => Ok(last4(secret)),
        _ if secret.trim().is_empty() => Err("secret: the key is empty".into()),
        _ => Ok(last4(secret)),
    }
}

/// Tests key `id` with a one-token call and records the outcome: the row, None when there is no
/// such key.
pub async fn test_key(
    state: &AppState,
    caller: &Caller,
    id: Uuid,
) -> Result<Option<KeyRow>, String> {
    let vault = state.vault()?;
    let records = state.store.at(caller);
    let Some(key) = records
        .open_key(&vault, id)
        .await
        .map_err(|e| format!("{e:#}"))?
    else {
        return Ok(None);
    };
    let (ok, detail) = one_token(state, &key).await;
    let detail = redact(&detail, key.secret_text());
    records
        .record_key_check(id, ok, &detail)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Whether a one-token call with `key` succeeds, and the provider's reason when not.
async fn one_token(state: &AppState, key: &OpenKey) -> (bool, String) {
    let ov = apply(state.config.providers.clone(), std::slice::from_ref(key));
    let p = match ov.config.resolve(Some(&provider_id(&key.row))) {
        Ok(p) => p,
        Err(e) => return (false, e),
    };
    if let (true, Some(url)) = (names_its_server(&key.row), &key.row.base_url) {
        if let Err(e) = reach::reach(url, state.store.layer().policy()).await {
            return (false, e);
        }
    }
    let model = match model_to_test(state, key, &p).await {
        Ok(m) => m,
        Err(e) => return (false, e),
    };
    let ask = json!({"model": model, "max_tokens": 1,
                     "messages": [{"role": "user", "content": "Say OK."}]});
    match p.send(&state.http, &ask).await {
        Ok(u) if (200..300).contains(&u.status) => (true, String::new()),
        Ok(u) if matches!(u.status, 401 | 403) => (
            false,
            format!("the provider turned the key down ({})", u.status),
        ),
        Ok(u) => (
            false,
            format!("the provider answered {}: {}", u.status, message_of(&u.raw)),
        ),
        Err(e) => (false, e),
    }
}

/// The key's model, else the provider's default, else the first model the server lists.
async fn model_to_test(state: &AppState, key: &OpenKey, p: &Provider) -> Result<String, String> {
    if let Some(m) = key.row.model.as_deref().filter(|m| !m.trim().is_empty()) {
        return Ok(m.to_string());
    }
    if let Some(m) = p.default_model() {
        return Ok(m.to_string());
    }
    let listed = listing::list_models(p, &state.http)
        .await
        .ok()
        .unwrap_or_default();
    listed
        .into_iter()
        .next()
        .ok_or_else(|| "name a model to test with".to_string())
}

/// The error message of a provider's answer, if it has one.
fn message_of(raw: &Value) -> String {
    let e = &raw["error"];
    let m = e["message"]
        .as_str()
        .or(e.as_str())
        .or(raw["message"].as_str());
    m.unwrap_or("no message").chars().take(300).collect()
}

/// `text` without `secret` (nor its long-enough pieces a provider might quote).
fn redact(text: &str, secret: &str) -> String {
    let mut out = text.to_string();
    if secret.len() >= 8 {
        out = out.replace(secret, "[your key]");
        let head: String = secret.chars().take(8).collect();
        out = out.replace(&head, "[your key]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detail_never_holds_the_key() {
        let d = redact("bad key sk-bad-secret-wxyz here", "sk-bad-secret-wxyz");
        assert!(!d.contains("sk-bad-s"), "{d}");
        assert_eq!(last4("abcdef"), "cdef");
        assert_eq!(last4("ab"), "ab");
        assert!(valid_name("my-pod_2") && !valid_name("My Pod") && !valid_name(""));
    }
}
