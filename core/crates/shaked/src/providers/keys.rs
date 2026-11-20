//! Your own keys over the environment's providers. Lookup for an agent's provider: your key for
//! it, then the environment's key, then the layer's fallback (`Layer::fallback_provider`; none
//! locally). A key shared with you (another member's, `Layer::own_keys`) serves only an agent that
//! names it (`key:<name>`), and never when it names its own server. A key's own server is checked against the layer's policy before each call
//! ([`super::reach`]). Test makes a one-token call with the key and records the outcome.

use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    bedrock, listing, reach, Endpoint, LocalEndpoint, Provider, ProviderConfig, ProviderKind,
    Secret, BEDROCK_PREFIX, CUSTOM_PREFIX, KEY_PREFIX, LOCAL_PREFIX,
};
use crate::{
    app::AppState,
    auth::Caller,
    layer::{KeySource, ModelWant},
    store::{KeyKind, KeyRow, OpenKey},
};

/// The providers with your keys applied, and which provider ids they key.
pub struct Overlay {
    pub config: ProviderConfig,
    /// `(provider id, key, the key names its own server)`, later entries winning.
    own: Vec<(String, Uuid, bool)>,
    /// The names of keys shared with you that name their own server: never used for you.
    withheld: Vec<String>,
}

impl Overlay {
    /// Your key behind provider `id`, and whether it names its own server.
    fn own_of(&self, id: &str) -> Option<(Uuid, bool)> {
        let id = match id.strip_prefix(CUSTOM_PREFIX) {
            Some(name) => format!("{LOCAL_PREFIX}{name}"),
            None => id.to_string(),
        };
        (self.own.iter().rev()).find_map(|(p, k, url)| (*p == id).then_some((*k, *url)))
    }
}

/// The provider id key `row` stands for: `openai`, `bedrock:<region>`, `custom:<name>`.
pub fn provider_id(row: &KeyRow) -> String {
    match row.kind {
        KeyKind::Anthropic => super::ANTHROPIC.into(),
        KeyKind::OpenAi => super::OPENAI.into(),
        KeyKind::OpenRouter => super::OPENROUTER.into(),
        KeyKind::Bedrock => format!(
            "{BEDROCK_PREFIX}{}",
            row.base_url.as_deref().unwrap_or_default()
        ),
        KeyKind::Custom => format!("{CUSTOM_PREFIX}{}", row.name),
    }
}

/// `config` with `keys` applied, oldest first (a newer key of a provider wins), all of them yours.
pub fn apply(config: ProviderConfig, keys: &[OpenKey]) -> Overlay {
    apply_owned(config, keys, |_| true)
}

/// `config` with `keys` applied, oldest first: each of yours (`is_own`) by its kind, and every
/// key by its name (`key:<name>`), except a shared one that names its own server: it is
/// `withheld`, since that server would see your agents' prompts.
fn apply_owned(
    mut config: ProviderConfig,
    keys: &[OpenKey],
    is_own: impl Fn(Uuid) -> bool,
) -> Overlay {
    let mut own = vec![];
    let mut withheld = vec![];
    for k in keys {
        let mine = is_own(k.row.id);
        let own_url = names_its_server(&k.row);
        if !mine && own_url {
            withheld.push(k.row.name.clone());
            continue;
        }
        if mine {
            own.push(apply_one(&mut config, k));
        }
        if let Ok(p) = by_name(&config, k) {
            config.named.retain(|(n, _)| *n != k.row.name);
            config.named.push((k.row.name.clone(), p));
            own.push((format!("{KEY_PREFIX}{}", k.row.name), k.row.id, own_url));
        }
    }
    Overlay {
        config,
        own,
        withheld,
    }
}

/// True when key `row` names a server of its own (a Bedrock key's `base_url` is its region).
fn names_its_server(row: &KeyRow) -> bool {
    row.kind != KeyKind::Bedrock && row.base_url.is_some()
}

/// The provider key `k` alone keys, for `key:<name>`.
fn by_name(config: &ProviderConfig, k: &OpenKey) -> Result<Provider, String> {
    let mut one = ProviderConfig {
        anthropic_max_tokens: config.anthropic_max_tokens,
        ..ProviderConfig::default()
    };
    apply_one(&mut one, k);
    one.resolve(Some(&provider_id(&k.row)))
}

/// Applies key `k` to `config`: the provider id it keys, its id, and whether it names its server.
fn apply_one(config: &mut ProviderConfig, k: &OpenKey) -> (String, Uuid, bool) {
    let (row, secret) = (&k.row, Secret::new(k.secret_text()));
    let keyed = |e: &mut Endpoint| {
        let base = row.base_url.clone().unwrap_or_else(|| e.base.clone());
        *e = Endpoint::new(&base, Some(secret.clone()));
    };
    match row.kind {
        KeyKind::OpenAi => keyed(&mut config.openai),
        KeyKind::Anthropic => keyed(&mut config.anthropic),
        KeyKind::OpenRouter => keyed(&mut config.openrouter),
        KeyKind::Bedrock => {
            let region = row.base_url.clone().unwrap_or_default();
            config.bedrock_keys.retain(|(r, _)| *r != region);
            config.bedrock_keys.push((region, secret.clone()));
        }
        KeyKind::Custom => {
            config.locals.retain(|l| l.name != row.name);
            let key = (!k.secret_text().is_empty()).then(|| secret.clone());
            let base = row.base_url.as_deref().unwrap_or_default();
            config.locals.push(LocalEndpoint {
                name: row.name.clone(),
                endpoint: Endpoint::new(base, key),
                rate: None,
            });
        }
    }
    let own_url = names_its_server(row);
    let id = provider_id(row).replace(CUSTOM_PREFIX, LOCAL_PREFIX);
    (id, row.id, own_url)
}

/// The environment's providers with `caller`'s keys applied.
pub async fn overlay(state: &AppState, caller: &Caller) -> Result<Overlay, String> {
    let base = state.config.providers.clone();
    let records = state.store.at(caller);
    let listed = records.list_keys().await.map_err(|e| format!("{e:#}"))?;
    if listed.is_empty() {
        return Ok(apply(base, &[]));
    }
    let vault = state.vault()?;
    let keys = records
        .open_keys(&vault)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let ids: Vec<Uuid> = keys.iter().map(|k| k.row.id).collect();
    let layer = state.store.layer();
    let mine = (layer.own_keys(caller, &ids).await).map_err(|e| format!("{e:#}"))?;
    Ok(apply_owned(base, &keys, |id| mine.contains(&id)))
}

/// The providers `caller`'s agents run on, for a run's checks: the environment's when the keys
/// cannot be read (the call itself then says why).
pub async fn config_for(state: &AppState, caller: &Caller) -> ProviderConfig {
    match overlay(state, caller).await {
        Ok(o) => o.config,
        Err(e) => {
            eprintln!("shaked: reading your model keys: {e}");
            state.config.providers.clone()
        }
    }
}

/// The provider `asked` names for `caller`, and whose key pays: your key, else the environment's,
/// else the layer's fallback when the environment has no key for it.
pub async fn provider_for(
    state: &AppState,
    caller: &Caller,
    asked: Option<&str>,
    model: &str,
) -> (Result<Provider, String>, KeySource) {
    let ov = match overlay(state, caller).await {
        Ok(o) => o,
        Err(e) => return (Err(e), KeySource::Environment),
    };
    if let Some(why) = withheld(&ov, asked) {
        return (Err(why), KeySource::Environment);
    }
    let found = ov.config.resolve(asked);
    let named = asked.filter(|a| a.starts_with(KEY_PREFIX));
    let own = |p: &Provider| ov.own_of(named.unwrap_or(&p.id));
    if let Some((id, own_url)) = found.as_ref().ok().and_then(own) {
        let policy = state.store.layer().policy();
        if let (true, Ok(p)) = (own_url, &found) {
            if let Err(e) = reach::reach(&p.endpoint.base, policy).await {
                return (Err(e), KeySource::Own(id));
            }
        }
        return (found, KeySource::Own(id));
    }
    let want = ModelWant {
        provider: asked,
        model,
    };
    environment_or_fallback(state, caller, found, want).await
}

/// Why `asked` names a key you may not use: another member's that names its own server.
fn withheld(ov: &Overlay, asked: Option<&str>) -> Option<String> {
    let name = asked?.strip_prefix(KEY_PREFIX)?;
    ov.withheld.iter().any(|w| w == name).then(|| {
        format!(
            "the key {name:?} is another member's and names its own server, which would see \
             your agents' prompts: add a key of your own"
        )
    })
}

/// `found` on the environment's key, else the layer's fallback when it needs a key the
/// environment does not have.
async fn environment_or_fallback(
    state: &AppState,
    caller: &Caller,
    found: Result<Provider, String>,
    want: ModelWant<'_>,
) -> (Result<Provider, String>, KeySource) {
    let keyed = [
        ProviderKind::OpenRouter,
        ProviderKind::OpenAi,
        ProviderKind::Anthropic,
    ];
    let keyless = matches!(&found, Ok(p) if keyed.contains(&p.kind) && p.endpoint.key.is_none());
    if keyless {
        if let Some(f) = state.store.layer().fallback_provider(caller, want).await {
            return (Ok(f.provider), KeySource::Fallback(f.source));
        }
    }
    (found, KeySource::Environment)
}

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
    if let (true, Some(url)) = (key.row.kind != KeyKind::Bedrock, &key.row.base_url) {
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
