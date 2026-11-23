//! shaked's process state: [`Config`] (defaults and the environment), [`AppState`] (the store,
//! hub, live branches, clients and the benchmark new runs use), the agent drivers, and the
//! environment probes. The HTTP service around it is [`router`] (`app/http.rs`).

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

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

mod http;

pub use http::{cors_origins_from, router};

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
    /// The project folder whose worlds, scenarios, setups and experiments shaked serves
    /// (`SHAKEDOWN_PROJECT`, set by shaked's main). None: the project operations answer 409.
    pub project_dir: Option<std::path::PathBuf>,
    /// How long a worker's link may take to say hello before it is refused.
    pub link_hello_timeout: Duration,
    /// How long a worker's link may stay silent (not even answering a ping) before it is closed.
    pub link_idle_timeout: Duration,
    /// How long a worker told to stop its branch has to report its end before the control box
    /// ends the branch itself.
    pub link_end_grace: Duration,
    /// How often the control box sweeps the branches sandboxes' workers run (see
    /// `worker::holds::sweep`).
    pub link_sweep_every: Duration,
    /// A `shaked worker`'s only: the control box's gateway its agents' model calls go on to,
    /// with its worker token. None: the gateway calls the providers itself.
    pub forward_to: Option<crate::gateway::ForwardTo>,
}

pub const DEFAULT_DRIVER_GRACE: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_DB_BYTES: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_RETRY_BASE: Duration = Duration::from_secs(1);
pub const DEFAULT_PROXY_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEFAULT_LINK_HELLO_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_LINK_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_LINK_END_GRACE: Duration = Duration::from_secs(30);
pub const DEFAULT_LINK_SWEEP_EVERY: Duration = Duration::from_secs(60);

pub use crate::providers::{is_openrouter_url, OPENROUTER_BASE};

/// The number setting `key`, when it is set and parses.
fn env_number(key: &str) -> Option<u64> {
    parts::settings::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
}

/// True when an executable `name` is in a folder of `PATH`.
pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| {
            let f = d.join(name);
            f.is_file() && {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::metadata(&f).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
                }
                #[cfg(not(unix))]
                {
                    true
                }
            }
        })
    })
}

impl Default for Config {
    fn default() -> Config {
        Config::new()
    }
}

impl Config {
    /// Defaults plus the providers' keys and local servers from the environment.
    pub fn new() -> Config {
        Config::with_providers(ProviderConfig::from_env())
    }

    /// [`Config::new`] on `providers`, the environment's model keys left unread.
    pub fn with_providers(providers: ProviderConfig) -> Config {
        Config {
            providers,
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
            project_dir: None,
            link_hello_timeout: DEFAULT_LINK_HELLO_TIMEOUT,
            link_idle_timeout: DEFAULT_LINK_IDLE_TIMEOUT,
            link_end_grace: DEFAULT_LINK_END_GRACE,
            link_sweep_every: DEFAULT_LINK_SWEEP_EVERY,
            forward_to: None,
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
            true => crate::bind::loopback_hosts(),
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
    pub builds: crate::world_builds::Builds,
    /// Branches currently running, by branch id.
    pub live: LiveBranches,
    pub config: Config,
    /// Shared HTTP client (gateway upstream, open agent loop, key tests and model lists), guarded
    /// by the layer's policy ([`crate::providers::reach::guard`]).
    pub http: reqwest::Client,
    /// The client of calls whose answer may stream for long (the gateway's pass-through and a
    /// worker's forwarding): a connect timeout and an idle timeout per read
    /// ([`STREAM_IDLE`]), no limit on the whole answer. Guarded as `http` is.
    pub stream_http: reqwest::Client,
    /// Set once, after the `Arc<AppState>` exists (the open driver holds a `Weak` to it).
    pub drivers: OnceLock<Drivers>,
    /// Set to true when shutdown starts; SSE streams end on it.
    pub shutdown: tokio::sync::watch::Sender<bool>,
    /// The project folders callers work in (`Config::project_dir` locally), each re-read when its
    /// files change.
    pub projects: crate::project_state::Projects,
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
    /// The branches workers in sandboxes run for this control box, and their tickets.
    pub links: crate::worker::Links,
}

/// How long a streamed answer may stay silent before the gateway gives it up.
pub const STREAM_IDLE: Duration = Duration::from_secs(300);

/// The streaming client ([`AppState::stream_http`]), guarded by `policy`.
fn stream_client(policy: &crate::layer::Policy) -> reqwest::Client {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(STREAM_IDLE);
    crate::providers::reach::guard(http, policy)
        .build()
        .expect("http client")
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

/// The benchmark chosen at start and the one new runs use: env `SHAKEDOWN_BENCHMARK`, else the
/// one remembered in `worlds_dir`, else the only one installed.
fn start_benchmarks(hub: &Hub, config: &Config) -> (String, String) {
    let named = config.start_benchmark.clone();
    let benchmark = named.clone().unwrap_or_else(|| only_benchmark(hub));
    let remembered = || crate::world_builds::remembered(hub, config.worlds_dir.as_deref());
    let current = named
        .or_else(remembered)
        .unwrap_or_else(|| benchmark.clone());
    if current != benchmark {
        eprintln!("shaked: using the remembered benchmark {current}");
    }
    (benchmark, current)
}

impl AppState {
    pub fn new(store: Arc<Store>, hub: Arc<Hub>, mut config: Config) -> AppState {
        let policy = store.layer().policy();
        let (http, stream_http) = (shared_client(policy), stream_client(policy));
        if !policy.environment_keys {
            config.providers = ProviderConfig::without_environment();
        }
        let (benchmark, current) = start_benchmarks(&hub, &config);
        AppState {
            projects: crate::project_state::Projects::new(config.project_dir.clone()),
            current: current.into(),
            builds: Default::default(),
            store,
            hub,
            benchmark,
            live: LiveBranches::default(),
            config,
            http,
            stream_http,
            drivers: OnceLock::new(),
            shutdown: tokio::sync::watch::channel(false).0,
            cleanup: Arc::default(),
            proxy: crate::proxy::Proxy::default(),
            vault: OnceLock::new(),
            objects: None,
            links: Default::default(),
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
        crate::world_builds::remember(self.config.worlds_dir.as_deref(), name);
        Ok(true)
    }
}

impl AppState {
    /// The vault for your model keys, read (or made) on first use.
    pub fn vault(&self) -> Result<Arc<crate::vault::Vault>, String> {
        if let Some(v) = self.vault.get() {
            return Ok(v.clone());
        }
        let layer = self.store.layer();
        if !layer.keeps_secrets() {
            return Err(
                "this shaked keeps no secrets (a sandbox's worker): it has no vault".into(),
            );
        }
        let made = Arc::new(crate::vault::Vault::for_policy(layer.policy())?);
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
    // In a worker, the CLI agents' model calls go through its gateway too.
    let routed = state.config.forward_to.is_some();
    let mut m: Drivers = HashMap::new();
    m.insert(AgentKind::Open, Arc::new(OpenDriver::new(state.clone())));
    let mut claude = ClaudeDriver::new(base.clone());
    claude.via_gateway = routed;
    m.insert(AgentKind::ClaudeCode, Arc::new(claude));
    let http = HttpDriver::new(
        state,
        state.config.http_agent_base(),
        state.config.http_agent_timeout,
    );
    m.insert(AgentKind::Http, Arc::new(http));
    let mut codex = CodexDriver::new(base);
    codex.via_gateway = routed;
    m.insert(AgentKind::Codex, Arc::new(codex));
    state.install_drivers(m)
}
