use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::Router;
use parts::{proto::BenchDescribe, Hub};

use crate::{
    agents::{
        claude::ClaudeDriver, codex::CodexDriver, http::HttpDriver, open::OpenDriver, Driver,
    },
    live::LiveBranches,
    model::AgentKind,
    providers::ProviderConfig,
    store::Store,
};

pub type Drivers = HashMap<AgentKind, Arc<dyn Driver>>;

#[derive(Debug, Clone)]
pub struct Config {
    /// The model providers open agents can use, and their keys (redacted in `Debug`).
    pub providers: ProviderConfig,
    /// This service's own base URL (e.g. `http://127.0.0.1:8787`), used by the open agent loop.
    pub self_base: String,
    /// New runs and forks are refused (507) once the Shakedown's saves use at least this many bytes.
    pub max_db_bytes: u64,
    /// The wait before an open agent retries a model call its provider turned down as
    /// overloaded or rate-limited, doubling for each further retry (unless it says how long).
    pub retry_base: Duration,
    /// Web origins (`scheme://host[:port]`) other than shaked's own that may call `/api` from a
    /// browser, such as a UI served from elsewhere: env `SHAKEDOWN_CORS_ORIGIN`, comma-separated.
    /// Empty: same origin only.
    pub cors_origins: Vec<String>,
    /// The base URL an `http` agent is told to reach its MCP endpoint at: env
    /// `SHAKEDOWN_AGENT_BASE` (for an agent elsewhere, through a tunnel); empty means `self_base`.
    pub agent_base: String,
    /// How long shaked waits for an `http` agent to answer a run (time paused left out).
    pub http_agent_timeout: Duration,
    /// How long a Code connection's driver may run on after its run ended before shaked kills it.
    pub driver_grace: Duration,
    /// How long a Code connection's driver may drive its run before shaked ends the run
    /// `failed` (scored) for it (time paused counted too).
    pub code_run_limit: Duration,
    /// Where built worlds are written and the current benchmark is remembered (`SHAKEDOWN_WORLDS`,
    /// set by shaked's main). None: no world can be built and nothing is remembered.
    pub worlds_dir: Option<std::path::PathBuf>,
    /// How long a request through the recording proxy may take, answer read whole, before it is
    /// answered 504 (recorded) and the branch goes on: env `SHAKEDOWN_PROXY_TIMEOUT_SECS`.
    pub proxy_timeout: Duration,
    /// The benchmark to start on (`SHAKEDOWN_BENCHMARK`); it wins over the one remembered in
    /// `worlds_dir`. None: the remembered one, else the only one installed.
    pub start_benchmark: Option<String>,
    /// The base URL every URL handed to an agent or a caller starts with (`SHAKEDOWN_PUBLIC_URL`,
    /// set by shaked's main). Empty means `self_base`.
    pub public_url: String,
    /// The Host names `/api` answers where the layer serves others (`SHAKEDOWN_ALLOWED_HOSTS`
    /// plus the public URL's host). Empty, or a layer that does not serve others, means this
    /// machine's loopback names only.
    pub allowed_hosts: Vec<String>,
}

pub const DEFAULT_DRIVER_GRACE: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_DB_BYTES: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_RETRY_BASE: Duration = Duration::from_secs(1);
pub const DEFAULT_PROXY_TIMEOUT: Duration = Duration::from_secs(300);

pub use crate::providers::{is_openrouter_url, OPENROUTER_BASE};

/// The number setting `key`, when it is set and parses.
fn env_number(key: &str) -> Option<u64> {
    parts::settings::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
}

impl Default for Config {
    fn default() -> Config {
        Config::new()
    }
}

impl Config {
    /// Defaults plus the providers' keys and local servers from the environment.
    pub fn new() -> Config {
        Config {
            providers: ProviderConfig::from_env(),
            self_base: String::new(),
            max_db_bytes: env_number("SHAKEDOWN_MAX_DB_BYTES").unwrap_or(DEFAULT_MAX_DB_BYTES),
            retry_base: DEFAULT_RETRY_BASE,
            cors_origins: cors_origins_from(
                &parts::settings::var("SHAKEDOWN_CORS_ORIGIN").unwrap_or_default(),
            ),
            agent_base: parts::settings::var("SHAKEDOWN_AGENT_BASE").unwrap_or_default(),
            http_agent_timeout: Duration::from_secs(crate::agents::CLI_TIMEOUT_SECS),
            driver_grace: DEFAULT_DRIVER_GRACE,
            code_run_limit: Duration::from_secs(
                env_number("SHAKEDOWN_CODE_RUN_SECS").unwrap_or(crate::agents::CLI_TIMEOUT_SECS),
            ),
            worlds_dir: None,
            proxy_timeout: env_number("SHAKEDOWN_PROXY_TIMEOUT_SECS")
                .map_or(DEFAULT_PROXY_TIMEOUT, Duration::from_secs),
            start_benchmark: parts::settings::var("SHAKEDOWN_BENCHMARK")
                .ok()
                .filter(|b| !b.is_empty()),
            public_url: String::new(),
            allowed_hosts: Vec::new(),
        }
    }

    /// The base of every URL handed out: `public_url`, else `self_base`, without a trailing `/`.
    pub fn public_base(&self) -> &str {
        match self.public_url.trim() {
            "" => self.self_base.trim_end_matches('/'),
            p => p.trim_end_matches('/'),
        }
    }

    /// The base an `http` agent is told to reach shaked at: `agent_base` (the old
    /// `SHAKEDOWN_AGENT_BASE`, for a tunnel), else the public base.
    pub fn http_agent_base(&self) -> String {
        match self.agent_base.trim() {
            "" => self.public_base().to_string(),
            b => b.to_string(),
        }
    }

    /// The Host names `/api` answers.
    pub fn hosts(&self) -> Vec<String> {
        match self.allowed_hosts.is_empty() {
            true => crate::mode::loopback_hosts(),
            false => self.allowed_hosts.clone(),
        }
    }

    /// Open-mode agents can run: some provider has what it needs.
    pub fn open_available(&self) -> bool {
        self.providers.any_available()
    }
}

pub struct AppState {
    pub store: Arc<Store>,
    /// The installed integrations and benchmarks, their programs started on first use.
    pub hub: Arc<Hub>,
    /// The benchmark chosen at start: env `SHAKEDOWN_BENCHMARK`, else the only one installed; empty
    /// when neither names one. New runs use [`AppState::benchmark`], which may since have changed.
    pub benchmark: String,
    /// The benchmark new runs use: env `SHAKEDOWN_BENCHMARK`, else the one remembered in
    /// `worlds_dir`, else `benchmark` (also when the remembered one can't describe itself at
    /// start); changed by [`AppState::use_benchmark`].
    current: std::sync::RwLock<String>,
    /// Built worlds whose base is being prepared, or was since shaked started.
    pub builds: crate::worlds::Builds,
    /// Branches currently running, by branch id.
    pub live: LiveBranches,
    pub config: Config,
    /// Shared HTTP client (gateway upstream, open agent loop, key tests and model lists), guarded
    /// by the layer's policy ([`crate::providers::reach::guard`]).
    pub http: reqwest::Client,
    /// Set once, after the `Arc<AppState>` exists (the open driver holds a `Weak` to it).
    pub drivers: OnceLock<Drivers>,
    /// Set to true when shutdown starts; SSE streams end on it.
    pub shutdown: tokio::sync::watch::Sender<bool>,
    /// Held for writing by a cleanup (`POST /api/gc`) and for reading while a run or fork
    /// starts, so a cleanup never drops saves a starting branch is about to name.
    pub cleanup: Arc<tokio::sync::RwLock<()>>,
    /// The recording proxy agents' own HTTP clients reach the world's hosts through.
    pub proxy: crate::proxy::Proxy,
    /// The vault your model keys are sealed with: set on first use from the environment
    /// ([`crate::vault::Vault::for_policy`]), or beforehand (tests).
    pub vault: OnceLock<Arc<crate::vault::Vault>>,
    /// The object stores `SHAKEDOWN_OBJECT_STORE` names (None: unset).
    pub objects: Option<Arc<crate::objects::ObjectStores>>,
}

/// The shared HTTP client, guarded by `policy` ([`crate::providers::reach::guard`]).
fn shared_client(policy: &crate::layer::Policy) -> reqwest::Client {
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300));
    crate::providers::reach::guard(http, policy)
        .build()
        .expect("http client")
}

impl AppState {
    pub fn new(store: Arc<Store>, hub: Arc<Hub>, mut config: Config) -> AppState {
        let policy = store.layer().policy();
        let http = shared_client(policy);
        if !policy.environment_keys {
            config.providers = ProviderConfig::without_environment();
        }
        let named = config.start_benchmark.clone();
        let benchmark = named.clone().unwrap_or_else(|| only_benchmark(&hub));
        let remembered = || crate::worlds::remembered(&hub, config.worlds_dir.as_deref());
        let current = named
            .or_else(remembered)
            .unwrap_or_else(|| benchmark.clone());
        if current != benchmark {
            eprintln!("shaked: using the remembered benchmark {current}");
        }
        AppState {
            current: current.into(),
            builds: Default::default(),
            store,
            hub,
            benchmark,
            live: LiveBranches::default(),
            config,
            http,
            drivers: OnceLock::new(),
            shutdown: tokio::sync::watch::channel(false).0,
            cleanup: Arc::default(),
            proxy: crate::proxy::Proxy::default(),
            vault: OnceLock::new(),
            objects: None,
        }
    }

    /// Uses `name` as the benchmark of new runs.
    pub fn with_benchmark(mut self, name: &str) -> AppState {
        self.benchmark = name.to_string();
        self.current = name.to_string().into();
        self
    }

    /// The benchmark new runs use (empty when none is chosen).
    pub fn benchmark(&self) -> String {
        self.current.read().unwrap().clone()
    }

    /// True when `name` is an installed benchmark.
    pub fn has_benchmark(&self, name: &str) -> bool {
        let registry = self.hub.registry();
        registry
            .get(name)
            .is_some_and(|m| m.kind == parts::Kind::Benchmark)
    }

    /// Makes `name` the benchmark of new runs, and remembers it across a restart.
    pub fn use_benchmark(&self, name: &str) -> Result<(), String> {
        self.switch_benchmark(name, None).map(|_| ())
    }

    /// Like [`use_benchmark`](Self::use_benchmark), only while the benchmark of new runs is still
    /// `was` (a choice made since stands): true when `name` was made current.
    pub fn use_benchmark_if(&self, name: &str, was: &str) -> Result<bool, String> {
        self.switch_benchmark(name, Some(was))
    }

    fn switch_benchmark(&self, name: &str, was: Option<&str>) -> Result<bool, String> {
        if !self.has_benchmark(name) {
            return Err(format!("no benchmark named {name}"));
        }
        let mut current = self.current.write().unwrap();
        if was.is_some_and(|w| *current != w) {
            return Ok(false);
        }
        *current = name.to_string();
        crate::worlds::remember(self.config.worlds_dir.as_deref(), name);
        Ok(true)
    }
}

impl AppState {
    /// The vault for your model keys, read (or made) on first use.
    pub fn vault(&self) -> Result<Arc<crate::vault::Vault>, String> {
        if let Some(v) = self.vault.get() {
            return Ok(v.clone());
        }
        let policy = self.store.layer().policy();
        let made = Arc::new(crate::vault::Vault::for_policy(policy)?);
        Ok(self.vault.get_or_init(|| made).clone())
    }

    /// The `describe` of the benchmark new runs use: its world, tasks, instructions and tools.
    pub async fn bench(&self) -> anyhow::Result<BenchDescribe> {
        self.bench_named(&self.benchmark()).await
    }

    /// The `describe` of the benchmark shaked starts on: shaked doesn't start without one (none
    /// chosen, its package missing or not built).
    pub async fn startup_bench(&self) -> anyhow::Result<BenchDescribe> {
        let name = self.benchmark();
        let described = match self.bench_named(&name).await {
            Err(e) if name != self.benchmark => {
                eprintln!(
                    "shaked: the remembered benchmark {name} can't be used ({e:#}); using {:?}",
                    self.benchmark
                );
                *self.current.write().unwrap() = self.benchmark.clone();
                self.bench_named(&self.benchmark).await
            }
            described => described,
        };
        described.map_err(|e| match self.benchmark().as_str() {
            "" => e,
            name => anyhow::anyhow!("benchmark {name}: {e:#}"),
        })
    }

    /// The `describe` of the benchmark `name` (empty: none is chosen, which is an error saying
    /// why).
    pub async fn bench_named(&self, name: &str) -> anyhow::Result<BenchDescribe> {
        if name.is_empty() {
            anyhow::bail!(no_benchmark_chosen(&self.hub));
        }
        self.hub.bench_describe(name).await
    }

    /// True when the Shakedown's saves use at least the configured limit: new runs are refused.
    pub async fn saves_over_limit(&self) -> anyhow::Result<bool> {
        let bytes = crate::gc::usage(&self.hub, &self.benchmark()).await?;
        Ok(bytes >= self.config.max_db_bytes)
    }

    /// Installs the driver map; false if one was already installed.
    pub fn install_drivers(&self, drivers: Drivers) -> bool {
        self.drivers.set(drivers).is_ok()
    }
}

/// Why no benchmark is chosen: none is installed, or several are and `SHAKEDOWN_BENCHMARK` names none.
fn no_benchmark_chosen(hub: &Hub) -> String {
    let installed: Vec<String> = hub
        .registry()
        .benchmarks()
        .map(|m| m.name.clone())
        .collect();
    if installed.is_empty() {
        return "no benchmark is installed: no folder in SHAKEDOWN_PACKAGES holds a benchmark package"
            .into();
    }
    format!(
        "several benchmarks are installed: set SHAKEDOWN_BENCHMARK to one of: {}",
        installed.join(", ")
    )
}

/// The name of the one installed benchmark; empty when there are none or several.
fn only_benchmark(hub: &Hub) -> String {
    let registry = hub.registry();
    let mut names = registry
        .benchmarks()
        .filter(|m| !m.opt_in)
        .map(|m| m.name.clone());
    match (names.next(), names.next()) {
        (Some(one), None) => one,
        _ => String::new(),
    }
}

/// The real drivers for every agent kind. Call once, after `config.self_base` and
/// `config.public_url` are final: the agents get URLs on the public base.
pub fn install_default_drivers(state: &Arc<AppState>) -> bool {
    let base = state.config.public_base().to_string();
    let mut m: Drivers = HashMap::new();
    m.insert(AgentKind::Open, Arc::new(OpenDriver::new(state.clone())));
    m.insert(
        AgentKind::ClaudeCode,
        Arc::new(ClaudeDriver::new(base.clone())),
    );
    let http = HttpDriver::new(
        state.config.http_agent_base(),
        state.store.clone(),
        state.config.http_agent_timeout,
    );
    m.insert(AgentKind::Http, Arc::new(http));
    m.insert(AgentKind::Codex, Arc::new(CodexDriver::new(base)));
    state.install_drivers(m)
}

/// True when `host` (a Host header value) names this machine's loopback: `127.0.0.1`, `localhost`
/// or `[::1]`, with an optional port.
#[cfg(test)]
fn is_loopback_host(host: &str) -> bool {
    crate::mode::host_allowed(host, &crate::mode::loopback_hosts())
}

/// Where the layer serves others, a 500's text (a query's error, a path, a constraint) stays in
/// the log: the answer is a generic message with a reference to find the detail by.
async fn hide_internal_errors(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = req.uri().path().to_string();
    let r = next.run(req).await;
    if r.status() != axum::http::StatusCode::INTERNAL_SERVER_ERROR {
        return r;
    }
    let detail = axum::body::to_bytes(r.into_body(), 64 * 1024).await;
    let detail = detail.unwrap_or_default();
    let reference = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let detail = String::from_utf8_lossy(&detail);
    eprintln!("shaked: 500 on {path} (ref {reference}): {detail}");
    let error = format!("something went wrong on our side (ref {reference})");
    let status = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
    (status, axum::Json(serde_json::json!({ "error": error }))).into_response()
}

/// DNS-rebinding guard: a web page on another origin can resolve its own name to this machine, so
/// the browser would send that name in `Host`. Only the allowed names reach the API.
async fn allowed_host(
    axum::extract::State(hosts): axum::extract::State<Arc<Vec<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let ok = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| crate::mode::host_allowed(h, &hosts));
    if !ok {
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"error": "forbidden host"})),
        )
            .into_response();
    }
    next.run(req).await
}

/// The origins of `SHAKEDOWN_CORS_ORIGIN` (comma-separated, each `http(s)://host[:port]` without a
/// path); anything else, `*` included, is skipped with a warning, since a wildcard would let any
/// web page drive this machine's arena.
pub fn cors_origins_from(value: &str) -> Vec<String> {
    let mut out = vec![];
    for o in value.split(',').map(|o| o.trim().trim_end_matches('/')) {
        if o.is_empty() {
            continue;
        }
        let rest = o
            .strip_prefix("http://")
            .or_else(|| o.strip_prefix("https://"));
        match rest {
            Some(host) if !host.is_empty() && !host.contains(['/', '*', ' ']) => {
                out.push(o.to_string())
            }
            _ => eprintln!(
                "shaked: SHAKEDOWN_CORS_ORIGIN: skipped {o:?} (expected http(s)://host[:port])"
            ),
        }
    }
    out
}

/// CORS for the origins in `SHAKEDOWN_CORS_ORIGIN`: a request from one of them gets
/// `Access-Control-Allow-Origin`, and its preflight (`OPTIONS`) is answered here. Requests from
/// any other origin are left as they are (the browser then refuses the answer).
async fn cors(
    axum::extract::State(origins): axum::extract::State<Arc<Vec<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue, Method};
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .filter(|o| origins.iter().any(|a| a == o))
        .and_then(|o| HeaderValue::from_str(o).ok());
    let Some(origin) = origin else {
        return next.run(req).await;
    };
    let preflight = req.method() == Method::OPTIONS
        && (req.headers()).contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    let mut res = match preflight {
        true => preflight_answer(),
        false => next.run(req).await,
    };
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    h.append(header::VARY, HeaderValue::from_static("origin"));
    res
}

/// The answer to an allowed origin's CORS preflight (its `Access-Control-Allow-Origin` is added
/// by [`cors`]).
fn preflight_answer() -> axum::response::Response {
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    let mut r = StatusCode::NO_CONTENT.into_response();
    let h = r.headers_mut();
    let methods = HeaderValue::from_static("GET, POST, DELETE, OPTIONS");
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, methods);
    let headers = HeaderValue::from_static("content-type");
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, headers);
    h.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    r
}

pub fn router(state: Arc<AppState>) -> Router {
    let origins = Arc::new(state.config.cors_origins.clone());
    // Locally a tokenless request is the person at this machine: only loopback names reach /api,
    // whatever the config says (a tunnel's Host never does).
    let hosts = Arc::new(match state.store.layer().policy().serves_others {
        true => state.config.hosts(),
        false => crate::mode::loopback_hosts(),
    });
    let authorize =
        axum::middleware::from_fn_with_state(state.clone(), crate::api::routes::authorize_route);
    let authenticate =
        axum::middleware::from_fn_with_state(state.clone(), crate::auth::middleware::authenticate);
    let api = crate::api::router().route_layer(authorize);
    let api = api.merge(state.store.layer().routes()).layer(authenticate);
    let api = api
        .layer(axum::middleware::from_fn_with_state(hosts, allowed_host))
        .layer(axum::middleware::from_fn_with_state(origins, cors));
    let all = crate::mcp::router()
        .merge(crate::gateway::router())
        .merge(api);
    let all = match state.store.layer().policy().serves_others {
        true => all.layer(axum::middleware::from_fn(hide_internal_errors)),
        false => all,
    };
    all.with_state(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cors_origins() {
        let got = super::cors_origins_from(
            " http://127.0.0.1:5173, https://ui.example/ ,*,ftp://x,http://a/b,,http://",
        );
        assert_eq!(got, ["http://127.0.0.1:5173", "https://ui.example"]);
    }

    /// A router with one `/api/x` route behind [`super::cors`] for `http://ui.test:5173`, and a
    /// request to it with `method` from `origin`, answered with its status and allowed origin.
    async fn cors_call(method: &str, origin: &str) -> (u16, Option<String>, bool) {
        use axum::{body::Body, http::Request, routing::get, Router};
        use tower::ServiceExt;
        let origins = std::sync::Arc::new(vec!["http://ui.test:5173".to_string()]);
        let app = Router::new()
            .route("/api/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(origins, super::cors));
        let req = Request::builder()
            .method(method)
            .uri("/api/x")
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap();
        let r = app.oneshot(req).await.unwrap();
        let h = r.headers();
        let allow = (h.get("access-control-allow-origin")).map(|v| v.to_str().unwrap().to_string());
        let methods = h.contains_key("access-control-allow-methods");
        (r.status().as_u16(), allow, methods)
    }

    #[tokio::test]
    async fn cors_answers_allowed_origins_only() {
        let ui = "http://ui.test:5173";
        assert_eq!(cors_call("GET", ui).await, (200, Some(ui.into()), false));
        assert_eq!(cors_call("OPTIONS", ui).await, (204, Some(ui.into()), true));
        assert_eq!(
            cors_call("GET", "http://evil.test").await,
            (200, None, false)
        );
        let (status, allow, _) = cors_call("OPTIONS", "http://evil.test").await;
        assert!(status != 204 && allow.is_none());
    }

    #[test]
    fn loopback_hosts() {
        use super::is_loopback_host as ok;
        for h in [
            "127.0.0.1",
            "127.0.0.1:4510",
            "localhost",
            "LOCALHOST:80",
            "[::1]",
            "[::1]:4510",
        ] {
            assert!(ok(h), "{h}");
        }
        for h in [
            "evil.example",
            "127.0.0.1.evil.example",
            "localhost.evil.com:80",
            "127.0.0.1:abc",
            "[::2]",
            "",
            "::1",
        ] {
            assert!(!ok(h), "{h}");
        }
    }
}
