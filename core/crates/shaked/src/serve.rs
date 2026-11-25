//! Serving shaked: what `main` read from the environment ([`ServeOptions`]) and the start-up
//! ([`run`]), in the library so a binary that embeds shaked with its own layer starts it the same
//! way. The environment is documented in `main.rs`.

use std::{path::PathBuf, sync::Arc};

use parts::{proto::BenchDescribe, Hub, Registry};

use crate::{
    app::{self, AppState, Config},
    bind::Bind,
    layer::Policy,
    store::Store,
};

/// What a start needs from the environment.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub bind: Bind,
    /// The Postgres server's URL, handed to command-mode programs.
    pub pg_url: String,
    /// `SHAKEDOWN_UI_DIST`.
    pub ui_dist: Option<PathBuf>,
    /// Where new runs' branches run, for a control box that runs them in sandboxes (never from
    /// the environment: the embedding binary sets it).
    pub hosting: Option<Arc<crate::hosting::Hosting>>,
}

impl ServeOptions {
    /// The options from the environment, the bind checked against `policy`.
    pub async fn from_env(policy: &Policy) -> anyhow::Result<ServeOptions> {
        Ok(ServeOptions {
            bind: bind(policy)?,
            pg_url: crate::pg::configured_url().await,
            ui_dist: ui_dist_from_env(),
            hosting: None,
        })
    }
}

/// Starts shaked over `store` (its layer with it) and serves until ctrl-c.
pub async fn run(store: Store, opts: ServeOptions) -> anyhow::Result<()> {
    let here = store.layer().policy().runs_packages_here;
    let state = app_state(store, &opts).await?;
    if state.store.layer().policy().serves_others {
        // Read now: a shaked that serves others never starts without its vault key.
        state.vault().map_err(anyhow::Error::msg)?;
    }
    let bench = match here {
        true => Some(local_start(&state).await?),
        false => None,
    };
    app::install_default_drivers(&state);
    let mut router = app::router(state.clone());
    if let Some(dist) = &opts.ui_dist {
        router = crate::api::with_ui(router, dist.clone());
    }
    let listener = tokio::net::TcpListener::bind(opts.bind.addr).await?;
    start_proxy(&state).await?;
    // The branches sandboxes' workers run: stops a restart interrupted, dead worker tokens.
    tokio::spawn(crate::worker::holds::sweep_every(state.clone()));
    // Ended runs to their observability targets, from the records (never from a sandbox).
    tokio::spawn(crate::otel::exporter::every(state.clone()));
    if state.hosting.get().is_some() {
        // This control box's sandboxes whose branch ended or that outlived their timeout.
        tokio::spawn(crate::hosting::sweep_every(state.clone()));
    }
    let line = listening_line(&state, &opts.bind, bench.as_ref(), opts.ui_dist.as_deref());
    println!("{line}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_on_ctrl_c(state))
        .await?;
    Ok(())
}

/// Registers the configured project's git imports; done before the benchmark is chosen, which
/// may be one of theirs.
async fn register_imports(state: &AppState) {
    let Some(root) = state.projects.configured() else {
        return;
    };
    for e in crate::git_import::register(&state.hub, root).await {
        eprintln!("shaked: {e}");
    }
}

/// `<repo>`: `SHAKEDOWN_DIR`, else the folder above `core` this shaked was built in.
pub(crate) fn repo_dir() -> PathBuf {
    let built = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    env_path("SHAKEDOWN_DIR", built.canonicalize().unwrap_or(built))
}

fn env_path(key: &str, default: PathBuf) -> PathBuf {
    path_or(parts::settings::var_os(key), default)
}

/// `set` as a path unless it is unset or empty, else `default`.
fn path_or(set: Option<std::ffi::OsString>, default: PathBuf) -> PathBuf {
    set.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

/// `SHAKEDOWN_WORLDS`, created, without the temporary folders a build left.
fn worlds_dir() -> anyhow::Result<PathBuf> {
    let dir = env_path("SHAKEDOWN_WORLDS", repo_dir().join("core/worlds"));
    std::fs::create_dir_all(&dir)?;
    crate::world_builds::remove_leftovers(&dir);
    Ok(dir)
}

/// What every command-mode program gets: `SHAKEDOWN_PG_URL`, and `SHAKEDOWN_TEMPLATES` when it is set.
fn program_env(pg_url: &str) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> =
        parts::settings::pair("SHAKEDOWN_PG_URL", pg_url.to_string()).into();
    if let Some(t) = parts::settings::var("SHAKEDOWN_TEMPLATES")
        .ok()
        .filter(|t| !t.is_empty())
    {
        env.extend(parts::settings::pair("SHAKEDOWN_TEMPLATES", t));
    }
    env
}

/// The hub over `SHAKEDOWN_PACKAGES` and the built worlds, `SHAKEDOWN_BIN_DIR` and `SHAKEDOWN_SAVES`;
/// it starts package programs when `programs` (else it describes benchmarks from their files).
fn hub(pg_url: &str, worlds: &std::path::Path, programs: bool) -> anyhow::Result<Hub> {
    let shakedown = repo_dir();
    let mut roots: Vec<PathBuf> = match parts::settings::var("SHAKEDOWN_PACKAGES") {
        Ok(p) if !p.is_empty() => std::env::split_paths(&p).collect(),
        _ => vec![shakedown.join("integrations"), shakedown.join("benchmarks")],
    };
    roots.push(worlds.to_path_buf());
    let bin_dir = env_path("SHAKEDOWN_BIN_DIR", shakedown.join("core/target/release"));
    let saves = env_path("SHAKEDOWN_SAVES", shakedown.join("core/saves"));
    std::fs::create_dir_all(&saves)?;
    let registry = Registry::scan(&roots)?;
    Ok(Hub::new(registry, bin_dir, saves)
        .with_repo_dir(shakedown)
        .with_env(program_env(pg_url))
        .with_programs(programs))
}

/// The hub a `shaked worker` runs its branch's world on: the one `shaked` would serve with, from
/// the same environment.
pub(crate) async fn worker_hub() -> anyhow::Result<Hub> {
    hub(&crate::pg::configured_url().await, &worlds_dir()?, true)
}

/// The stores `SHAKEDOWN_OBJECT_STORE` names; where the layer serves others, the write store is
/// probed first and a start without a working one is refused.
pub async fn object_stores(
    policy: &Policy,
) -> anyhow::Result<Option<Arc<crate::objects::ObjectStores>>> {
    let Some(stores) = crate::objects::ObjectStores::from_env().map_err(anyhow::Error::msg)? else {
        return Ok(None);
    };
    if policy.serves_others {
        stores.probe().await.map_err(anyhow::Error::msg)?;
    }
    Ok(Some(Arc::new(stores)))
}

/// The state over `store`, its object stores and the hub.
async fn app_state(store: Store, opts: &ServeOptions) -> anyhow::Result<Arc<AppState>> {
    let objects = object_stores(store.layer().policy()).await?;
    let (url, bind) = (&opts.pg_url, &opts.bind);
    let store = Arc::new(store);
    let worlds = worlds_dir()?;
    let here = store.layer().policy().runs_packages_here;
    let hub = Arc::new(hub(url, &worlds, here)?);
    let mut config = Config::new();
    config.self_base = format!("http://{}", loopback_of(bind.addr));
    config.public_url = bind.public_url.to_string();
    config.allowed_hosts = bind.allowed_hosts.clone();
    config.worlds_dir = Some(worlds);
    config.project_dir = parts::settings::var_os("SHAKEDOWN_PROJECT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let mut state = AppState::new(store, hub, config);
    state.objects = objects;
    if let Some(hosting) = &opts.hosting {
        let _ = state.hosting.set(hosting.clone());
    }
    Ok(Arc::new(state))
}

/// Where shaked reaches itself: `addr`, or loopback on its port when it listens on every address.
fn loopback_of(addr: std::net::SocketAddr) -> std::net::SocketAddr {
    match addr.ip().is_unspecified() {
        true => std::net::SocketAddr::from(([127, 0, 0, 1], addr.port())),
        false => addr,
    }
}

/// Where to listen: beyond this machine only for a layer that serves others.
fn bind(policy: &Policy) -> anyhow::Result<Bind> {
    let get = |k: &str| parts::settings::var(k).ok();
    crate::bind::bind_from(get, policy.serves_others).map_err(anyhow::Error::msg)
}

/// The modes this machine can run, for the startup line.
fn available_modes(state: &AppState) -> Vec<&'static str> {
    [
        ("claude-code", app::on_path("claude")),
        ("codex", app::on_path("codex")),
        ("open", state.config.open_available()),
    ]
    .into_iter()
    .filter(|m| m.1)
    .map(|m| m.0)
    .collect()
}

/// Ends the SSE streams on ctrl-c so graceful shutdown can finish; exits anyway after 5 s.
async fn shutdown_on_ctrl_c(state: Arc<AppState>) {
    let _ = tokio::signal::ctrl_c().await;
    let _ = state.shutdown.send(true);
    state.proxy.cas.clear();
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        std::process::exit(0);
    });
}

/// `shaked base`: makes the benchmark's base world, the saves every run starts from, unless it
/// exists. `store`'s layer runs packages here and serves no one else.
pub(crate) async fn build_base(store: Store, opts: &ServeOptions) -> anyhow::Result<()> {
    let state = app_state(store, opts).await?;
    let bench = local_start(&state).await?;
    let saves = crate::world::ensure_base(&state.hub, &bench).await?;
    println!(
        "shaked: base of {} {}: {saves:?}",
        bench.name, bench.version
    );
    Ok(())
}

/// `SHAKEDOWN_PROXY_PORT`, or 0 (a free port).
fn proxy_port_from_env() -> anyhow::Result<u16> {
    match parts::settings::var("SHAKEDOWN_PROXY_PORT") {
        Ok(p) if !p.is_empty() => p
            .parse()
            .map_err(|_| anyhow::anyhow!("SHAKEDOWN_PROXY_PORT {p:?} is not a port number")),
        _ => Ok(0),
    }
}

/// `SHAKEDOWN_UI_DIST`, when set.
fn ui_dist_from_env() -> Option<PathBuf> {
    let dist = parts::settings::var("SHAKEDOWN_UI_DIST").ok();
    dist.filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// Serves the recording proxy on 127.0.0.1 (`SHAKEDOWN_PROXY_PORT`).
pub(crate) async fn start_proxy(state: &Arc<AppState>) -> anyhow::Result<()> {
    let proxy = tokio::net::TcpListener::bind(("127.0.0.1", proxy_port_from_env()?)).await?;
    println!("shaked: proxy on http://{}", proxy.local_addr()?);
    tokio::spawn(crate::proxy::serve(state.clone(), proxy));
    Ok(())
}

/// The startup line: where shaked listens, as which URL, its benchmark, modes and UI.
fn listening_line(
    state: &AppState,
    bind: &Bind,
    bench: Option<&BenchDescribe>,
    ui: Option<&std::path::Path>,
) -> String {
    let bench = bench.map_or("none (packages run in sandboxes)".to_string(), |b| {
        format!("{} {}", b.name, b.version)
    });
    format!(
        "shaked: listening on http://{} as {} (benchmark: {bench}; modes: {}; ui: {})",
        bind.addr,
        state.config.public_base(),
        available_modes(state).join(", "),
        ui.map_or("api only".to_string(), |d| d.display().to_string())
    )
}

/// The start where packages run here: the project's git imports registered, its benchmark, and
/// before any run can save, dropping the saves no event names (left by crashes and aborted
/// tests).
async fn local_start(state: &AppState) -> anyhow::Result<BenchDescribe> {
    register_imports(state).await;
    let bench = state.startup_bench().await?;
    crate::gc::run_startup_gc(&state.store, &state.hub, &state.benchmark()).await?;
    Ok(bench)
}

#[cfg(test)]
mod tests {
    use super::path_or;
    use std::path::PathBuf;

    #[test]
    fn a_set_folder_overrides_the_default_and_an_empty_one_does_not() {
        let built = PathBuf::from("/build/repo");
        let set = Some("/opt/shakedown".into());
        assert_eq!(path_or(set, built.clone()), PathBuf::from("/opt/shakedown"));
        assert_eq!(path_or(Some("".into()), built.clone()), built);
        assert_eq!(path_or(None, built.clone()), built);
    }
}
