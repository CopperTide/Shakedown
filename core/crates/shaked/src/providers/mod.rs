//! Model providers: where an open agent's model calls go, and how the gateway talks to each.
//!
//! The recorded, canonical format is OpenAI chat completions. Each provider takes a canonical
//! request and gives back a canonical response plus its own raw response ([`Provider::send`]);
//! the agent loop, replay and fork only ever see the canonical form.

mod anthropic;
pub(crate) mod bedrock;
mod canonical;
mod config;
mod key_check;
pub mod keys;
mod listing;
mod openai;
pub mod reach;
mod status;
mod transport;

use serde::Serialize;
use serde_json::Value;

pub use config::{
    is_openrouter_url, local_key_env, local_price_env, Endpoint, LocalEndpoint, ProviderConfig,
    Rate, Secret, ANTHROPIC_BASE, ANTHROPIC_MAX_TOKENS_ENV, DEFAULT_ANTHROPIC_MAX_TOKENS,
    LOCAL_KEY_PREFIX, LOCAL_MODELS_ENV, OPENAI_BASE, OPENROUTER_BASE,
};
pub use listing::LIST_TIMEOUT;
pub use status::{statuses, summary, ProviderStatus, KEY_REJECTED, NOT_CHECKED};

pub const OPENROUTER: &str = "openrouter";
pub const OPENAI: &str = "openai";
pub const ANTHROPIC: &str = "anthropic";
pub const LOCAL_PREFIX: &str = "local:";
/// One of your own OpenAI-compatible servers (Settings → Models): `custom:<name>`. It takes over
/// from a `local:<name>` of `SHAKEDOWN_LOCAL_MODELS` with the same name.
pub const CUSTOM_PREFIX: &str = "custom:";
/// One of the model keys you can see, by its name: `key:<name>`. The only way a key shared with
/// you (another member's) serves your agents.
pub const KEY_PREFIX: &str = "key:";
/// Amazon Bedrock in one AWS region: `bedrock:<region>`.
pub const BEDROCK_PREFIX: &str = "bedrock:";
/// The local provider the old single-upstream override becomes.
pub const CUSTOM_LOCAL: &str = "custom";
/// The model of an OpenRouter agent that names none.
pub const DEFAULT_OPENROUTER_MODEL: &str = "deepseek/deepseek-chat-v3.1";
pub const LOCAL_COST_SOURCE: &str = "local (no API charge)";
/// Where the cost of a call to a local server with a declared rate comes from.
pub const LOCAL_RATE_COST_SOURCE: &str = "rate set for this local server";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenRouter,
    OpenAi,
    Anthropic,
    Local,
    Bedrock,
}

/// Why a provider can't take calls, judged from the configuration alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// The env var holding its key is not set.
    MissingKey(&'static str),
    /// No AWS credentials in the environment and no AWS CLI to find them.
    NoAwsCredentials,
    /// Bedrock with no key of your own where the machine's AWS credentials serve no caller.
    NoOwnBedrockKey,
}

impl Unavailable {
    /// The plain reason, as `/api/providers` shows it.
    pub fn why(&self) -> String {
        match self {
            Unavailable::MissingKey(env) => format!("{env} is not set"),
            Unavailable::NoAwsCredentials => bedrock::credentials::NO_CREDENTIALS.into(),
            Unavailable::NoOwnBedrockKey => bedrock::credentials::NO_OWN_KEY.into(),
        }
    }

    /// Why a run or fork that needs the provider is refused.
    pub fn refusal(&self) -> String {
        match self {
            Unavailable::MissingKey(env) => format!("open mode needs {env}"),
            Unavailable::NoAwsCredentials => {
                format!("Bedrock needs {}", bedrock::credentials::NO_CREDENTIALS)
            }
            Unavailable::NoOwnBedrockKey => bedrock::credentials::NO_OWN_KEY.into(),
        }
    }
}

/// What a provider answered: the HTTP status, the response in the canonical (OpenAI
/// chat-completions) shape, and the provider's own response as it came.
#[derive(Debug, Clone, PartialEq)]
pub struct Upstream {
    pub status: u16,
    pub response: Value,
    pub raw: Value,
    /// The seconds the provider asked to wait before trying again (its `retry-after`).
    pub retry_after: Option<u64>,
}

/// One provider, resolved from the configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    /// `openrouter`, `openai`, `anthropic` or `local:<name>`.
    pub id: String,
    pub kind: ProviderKind,
    pub endpoint: Endpoint,
    /// The `max_tokens` of a call that names none, for an API that requires one (Claude's).
    pub max_tokens: u64,
    /// A local server's declared token rate (`SHAKEDOWN_LOCAL_PRICE_<NAME>`).
    pub rate: Option<Rate>,
    /// Bedrock without a key of its own may sign with the machine's AWS credentials (env, the aws
    /// CLI, an instance role); false where the layer lends no environment keys.
    pub machine_credentials: bool,
    /// Limits every call through this provider must keep (a layer's lent provider sets them).
    pub cap: Option<CallCap>,
}

/// Limits a call is held to before it is sent: its `max_tokens` (set when the call names none),
/// and on OpenRouter the most it may cost per million tokens (`provider.max_price`), so the
/// provider itself refuses a dearer route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CallCap {
    pub max_tokens: u64,
    /// (prompt, completion) in USD per million tokens.
    pub max_price_per_mtok: Option<(f64, f64)>,
}

impl CallCap {
    /// `body` held to the cap (see [`CallCap`]) for a call to a provider of `kind`.
    pub fn apply(&self, kind: ProviderKind, mut body: serde_json::Value) -> serde_json::Value {
        let Some(obj) = body.as_object_mut() else {
            return body;
        };
        let mut named = false;
        for field in ["max_tokens", "max_completion_tokens"] {
            if let Some(n) = obj.get(field).and_then(serde_json::Value::as_u64) {
                obj.insert(field.into(), n.min(self.max_tokens).into());
                named = true;
            } else if obj.contains_key(field) {
                obj.insert(field.into(), self.max_tokens.into());
                named = true;
            }
        }
        if !named {
            obj.insert("max_tokens".into(), self.max_tokens.into());
        }
        if let (ProviderKind::OpenRouter, Some((prompt, completion))) =
            (kind, self.max_price_per_mtok)
        {
            let routing = obj
                .entry("provider")
                .or_insert_with(|| serde_json::json!({}));
            if !routing.is_object() {
                *routing = serde_json::json!({});
            }
            routing["max_price"] = serde_json::json!({"prompt": prompt, "completion": completion});
        }
        body
    }
}

impl Provider {
    fn new(id: impl Into<String>, kind: ProviderKind, endpoint: &Endpoint) -> Provider {
        Provider {
            id: id.into(),
            kind,
            endpoint: endpoint.clone(),
            max_tokens: DEFAULT_ANTHROPIC_MAX_TOKENS,
            rate: None,
            machine_credentials: true,
            cap: None,
        }
    }

    /// A local server's provider `id`, with its rate.
    fn local(id: impl Into<String>, l: &LocalEndpoint) -> Provider {
        Provider {
            rate: l.rate,
            ..Provider::new(id, ProviderKind::Local, &l.endpoint)
        }
    }

    /// Sends one canonical chat-completions `request`. `Err` is a transport failure (no answer).
    pub async fn send(&self, http: &reqwest::Client, request: &Value) -> Result<Upstream, String> {
        match self.kind {
            ProviderKind::Anthropic => anthropic::send(self, http, request).await,
            ProviderKind::Bedrock => bedrock::send(self, http, request).await,
            _ => openai::send(self, http, request).await,
        }
    }

    /// A `POST` to `path` under the provider's base with its own key, as its API takes it
    /// (`x-api-key` for Claude's, else a bearer token), and nothing else: for a request passed
    /// on as an agent sent it.
    pub fn raw_post(&self, http: &reqwest::Client, path: &str) -> reqwest::RequestBuilder {
        let post = reqwest::Method::POST;
        match self.kind {
            ProviderKind::Anthropic => anthropic::request(self, http, post, path),
            _ => openai::request(self, http, post, path),
        }
    }

    /// Why the provider can't take calls (no request is made to find out).
    pub fn unavailable(&self) -> Option<Unavailable> {
        let missing = |env| {
            self.endpoint
                .key
                .is_none()
                .then_some(Unavailable::MissingKey(env))
        };
        match self.kind {
            ProviderKind::OpenRouter => missing("OPENROUTER_API_KEY"),
            ProviderKind::OpenAi => missing("OPENAI_API_KEY"),
            ProviderKind::Anthropic => missing("ANTHROPIC_API_KEY"),
            ProviderKind::Local => None,
            ProviderKind::Bedrock if self.endpoint.key.is_some() => None,
            ProviderKind::Bedrock if !self.machine_credentials => {
                Some(Unavailable::NoOwnBedrockKey)
            }
            ProviderKind::Bedrock => {
                (!bedrock::credentials::some_source()).then_some(Unavailable::NoAwsCredentials)
            }
        }
    }

    /// The cost of a successful call with `usage`: what OpenRouter reports, a local server's
    /// declared rate (else 0), unknown otherwise (no price table is kept).
    pub fn cost(&self, usage: &Value) -> Option<f64> {
        match self.kind {
            ProviderKind::OpenRouter => usage.get("cost").and_then(Value::as_f64),
            ProviderKind::Local => Some(self.rate.map_or(0.0, |r| r.cost(usage))),
            ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::Bedrock => None,
        }
    }

    /// The label of a cost [`Provider::cost`] knows; `None` where it never knows one.
    pub fn cost_source(&self) -> Option<&'static str> {
        match self.kind {
            ProviderKind::OpenRouter => Some(OPENROUTER),
            ProviderKind::Local if self.rate.is_some() => Some(LOCAL_RATE_COST_SOURCE),
            ProviderKind::Local => Some(LOCAL_COST_SOURCE),
            ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::Bedrock => None,
        }
    }

    /// The model an agent that names none gets: only OpenRouter has one.
    pub fn default_model(&self) -> Option<&'static str> {
        (self.kind == ProviderKind::OpenRouter).then_some(DEFAULT_OPENROUTER_MODEL)
    }
}

impl ProviderConfig {
    /// The provider `id` names (`None`: the default one); `Err` names the known ones.
    pub fn resolve(&self, id: Option<&str>) -> Result<Provider, String> {
        let id = self.id_of(id);
        let found = match id {
            OPENROUTER => Some(Provider::new(
                id,
                ProviderKind::OpenRouter,
                &self.openrouter,
            )),
            OPENAI => Some(Provider::new(id, ProviderKind::OpenAi, &self.openai)),
            ANTHROPIC => Some(self.anthropic()),
            _ if id.starts_with(BEDROCK_PREFIX) => self.bedrock(id)?,
            _ if id.starts_with(KEY_PREFIX) => (self.named.iter())
                .find(|(n, _)| Some(n.as_str()) == id.strip_prefix(KEY_PREFIX))
                .map(|(_, p)| p.clone()),
            _ => (id.strip_prefix(LOCAL_PREFIX))
                .or_else(|| id.strip_prefix(CUSTOM_PREFIX))
                .and_then(|name| self.local(name))
                .map(|l| Provider::local(id, l)),
        };
        found.ok_or_else(|| {
            let known: Vec<String> = self.all().into_iter().map(|p| p.id).collect();
            format!("unknown provider {id:?} (known: {})", known.join(", "))
        })
    }

    /// What prices calls to provider `id` where your keys are not at hand (an agent loop's cost
    /// label): [`ProviderConfig::resolve`], and for one of your `custom:<name>` servers the
    /// environment does not know, a local server without a declared rate (its calls cost 0).
    pub fn resolve_pricing(&self, id: Option<&str>) -> Result<Provider, String> {
        self.resolve(id).or_else(|e| match self.id_of(id) {
            custom if custom.starts_with(CUSTOM_PREFIX) => {
                let blank = Endpoint::new("", None);
                Ok(Provider::new(custom, ProviderKind::Local, &blank))
            }
            _ => Err(e),
        })
    }

    /// Bedrock in the region `id` names (`Ok(None)` never: a bad region is an error).
    fn bedrock(&self, id: &str) -> Result<Option<Provider>, String> {
        let region = id.strip_prefix(BEDROCK_PREFIX).unwrap_or_default();
        if !bedrock::valid_region(region) {
            return Err(format!(
                "{id:?} names no AWS region (for example bedrock:us-east-1)"
            ));
        }
        let own = (self.bedrock_keys.iter()).find_map(|(r, k)| (r == region).then(|| k.clone()));
        let endpoint = Endpoint::new(&bedrock::base_url(region), own);
        Ok(Some(Provider {
            machine_credentials: self.machine_credentials,
            ..Provider::new(id, ProviderKind::Bedrock, &endpoint)
        }))
    }

    fn anthropic(&self) -> Provider {
        Provider {
            max_tokens: self.anthropic_max_tokens,
            ..Provider::new(ANTHROPIC, ProviderKind::Anthropic, &self.anthropic)
        }
    }

    /// The id `provider` stands for: itself, or the default provider when absent (what
    /// [`ProviderConfig::resolve`] goes by).
    pub fn id_of<'a>(&'a self, provider: Option<&'a str>) -> &'a str {
        provider.unwrap_or(&self.default)
    }

    /// Every configured provider: OpenRouter, OpenAI, Anthropic, then each local server.
    pub fn all(&self) -> Vec<Provider> {
        let mut out = vec![
            Provider::new(OPENROUTER, ProviderKind::OpenRouter, &self.openrouter),
            Provider::new(OPENAI, ProviderKind::OpenAi, &self.openai),
            self.anthropic(),
        ];
        out.extend(
            (self.locals.iter()).map(|l| Provider::local(format!("{LOCAL_PREFIX}{}", l.name), l)),
        );
        out
    }

    /// True when some provider can take calls, judged from the configuration alone.
    pub fn any_available(&self) -> bool {
        self.all().iter().any(|p| p.unavailable().is_none())
    }

    /// Why an open agent on `provider` with `model` can't run, if it can't.
    pub fn refusal(&self, provider: Option<&str>, model: Option<&str>) -> Option<String> {
        let p = match self.resolve(provider) {
            Ok(p) => p,
            Err(e) => return Some(e),
        };
        if let Some(u) = p.unavailable() {
            return Some(u.refusal());
        }
        let has_model = model.is_some_and(|m| !m.trim().is_empty());
        (!has_model && p.default_model().is_none())
            .then(|| format!("provider {} has no default model: name one", p.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ProviderConfig {
        let mut c = ProviderConfig::default();
        c.openrouter.key = Some(Secret::new("k"));
        c.locals.push(LocalEndpoint {
            name: "box".into(),
            endpoint: Endpoint::new("http://127.0.0.1:9/v1", None),
            rate: None,
        });
        c
    }

    #[test]
    fn ids_resolve_to_their_provider() {
        let c = config();
        assert_eq!(c.resolve(None).unwrap().kind, ProviderKind::OpenRouter);
        assert_eq!(
            c.resolve(Some("openai")).unwrap().kind,
            ProviderKind::OpenAi
        );
        let local = c.resolve(Some("local:box")).unwrap();
        assert_eq!(
            (local.kind, local.endpoint.base.as_str()),
            (ProviderKind::Local, "http://127.0.0.1:9/v1")
        );
        let e = c.resolve(Some("local:nope")).unwrap_err();
        assert!(e.contains("local:nope") && e.contains("local:box"), "{e}");
        assert!(c.resolve(Some("gemini")).is_err());
    }

    #[test]
    fn refusals_name_the_missing_piece() {
        let c = config();
        assert_eq!(c.refusal(None, None), None);
        assert_eq!(
            c.refusal(Some("openai"), Some("gpt-x")).as_deref(),
            Some("open mode needs OPENAI_API_KEY")
        );
        assert_eq!(
            c.refusal(Some("anthropic"), Some("m")).as_deref(),
            Some("open mode needs ANTHROPIC_API_KEY")
        );
        assert!(c
            .refusal(Some("local:box"), None)
            .unwrap()
            .contains("no default model"));
        assert_eq!(c.refusal(Some("local:box"), Some("llama3")), None);
        assert!(c.any_available());
        assert!(!ProviderConfig::default().any_available());
    }

    #[test]
    fn cost_is_reported_known_or_unknown_by_provider() {
        let c = config();
        let usage = serde_json::json!({"cost": 0.25});
        let cost = |id| c.resolve(Some(id)).unwrap().cost(&usage);
        assert_eq!(cost("openrouter"), Some(0.25));
        assert_eq!(cost("openai"), None);
        assert_eq!(cost("local:box"), Some(0.0));
        let source = |id| c.resolve(Some(id)).unwrap().cost_source();
        assert_eq!(source("local:box"), Some(LOCAL_COST_SOURCE));
        assert_eq!(source("openai"), None);
    }

    #[test]
    fn claude_calls_take_the_configured_budget() {
        let c = ProviderConfig {
            anthropic_max_tokens: 64_000,
            ..ProviderConfig::default()
        };
        assert_eq!(c.resolve(Some("anthropic")).unwrap().max_tokens, 64_000);
        assert_eq!(c.all()[2].max_tokens, 64_000);
    }
}
