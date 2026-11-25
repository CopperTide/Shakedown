//! Your own keys over the environment's providers. Lookup for an agent's provider: your key for
//! it, then the environment's key, then the layer's fallback (`Layer::fallback_provider`; none
//! locally). A key shared with you (another member's, `Layer::own_keys`) serves only an agent that
//! names it (`key:<name>`), and never when it names its own server. A key's own server is
//! checked against the layer's policy before each call ([`super::reach`]). Checking a key to add
//! and testing a stored one are in [`super::key_check`].

use uuid::Uuid;

pub use super::key_check::{check_new, check_secret, test_key, Checked};
use super::{
    reach, Endpoint, LocalEndpoint, NamedKey, Provider, ProviderConfig, ProviderKind, Secret,
    BEDROCK_PREFIX, CUSTOM_PREFIX, KEY_PREFIX, LOCAL_PREFIX,
};
use crate::{
    app::AppState,
    auth::Caller,
    layer::{KeySource, ModelWant},
    model::Team,
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
pub(super) fn names_its_server(row: &KeyRow) -> bool {
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

/// The keys `team`'s agents name (`key:<name>`) among those `caller` sees, each with the
/// provider it keys and no secret: what a sandbox's worker is told in `Start`.
pub async fn named_in(state: &AppState, caller: &Caller, team: &Team) -> Vec<NamedKey> {
    let agents = team.agents.iter();
    let names: Vec<&str> = agents
        .filter_map(|a| a.provider.as_deref()?.strip_prefix(KEY_PREFIX))
        .collect();
    if names.is_empty() {
        return vec![];
    }
    let rows = match state.store.at(caller).list_keys().await {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("shaked: listing your model keys: {e:#}");
            return vec![];
        }
    };
    let named = rows.iter().filter(|r| names.contains(&r.name.as_str()));
    let named = named.map(|r| NamedKey {
        name: r.name.clone(),
        provider: provider_id(r),
    });
    named.collect()
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

/// Why a key shared with you may not serve your agents: that server would see their prompts.
const SHARED_SERVER: &str = "is another member's and names its own server, which would see your \
                             agents' prompts: add a key of your own";

/// Why `asked` names a key you may not use: another member's that names its own server.
fn withheld(ov: &Overlay, asked: Option<&str>) -> Option<String> {
    let name = asked?.strip_prefix(KEY_PREFIX)?;
    (ov.withheld.iter().any(|w| w == name)).then(|| format!("the key {name:?} {SHARED_SERVER}"))
}

/// Why an agent may not name key `row` as its provider, if it may not: another member's key that
/// names its own server (`own`: the key is yours).
pub fn provider_refusal(row: &KeyRow, own: bool) -> Option<String> {
    (!own && names_its_server(row)).then(|| SHARED_SERVER.to_string())
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
