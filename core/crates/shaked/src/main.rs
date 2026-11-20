//! shaked, the Shakedown service (`cargo run -p shaked` from `core`). `core/crates/arenad-alias` builds
//! this same file as `arenad`, its name until 2026-11-19.
//!
//! Environment: `SHAKEDOWN_PG_URL`, `SHAKEDOWN_PORT` (4510), `SHAKEDOWN_PROXY_PORT` (the recording proxy,
//! on 127.0.0.1; default: a free port), `SHAKEDOWN_PROXY_TIMEOUT_SECS` (300: how long a proxied
//! request may take before it is answered 504),
//! the model providers of open agents (`OPENROUTER_API_KEY`, `OPENAI_API_KEY`,
//! `ANTHROPIC_API_KEY`, `SHAKEDOWN_LOCAL_MODELS="name=url,…"`, `SHAKEDOWN_LOCAL_KEY_<NAME>`,
//! `SHAKEDOWN_ANTHROPIC_MAX_TOKENS` (16000); see
//! `providers::ProviderConfig`),
//! `SHAKEDOWN_UI_DIST` (serve the built UI at `/`),
//! `SHAKEDOWN_MAX_DB_BYTES` (default 5 GiB: new runs and forks are refused above it),
//! `SHAKEDOWN_BENCHMARK` (default: the benchmark remembered in `SHAKEDOWN_WORLDS`, else the only installed
//! benchmark; a remembered one that can't describe itself is logged and passed over), and where the
//! parts are:
//! - `SHAKEDOWN_PACKAGES`: `:`-separated folders whose subfolders hold `shakedown-package.toml`
//!   (default `<repo>/integrations:<repo>/benchmarks`);
//! - `SHAKEDOWN_BIN_DIR`: substituted for `${SHAKEDOWN_BIN_DIR}` in package commands
//!   (default `<repo>/core/target/release`; build with `cargo build --release` in `<repo>`);
//!   `${SHAKEDOWN_DIR}` stands for `<repo>`;
//! - `SHAKEDOWN_SAVES`: save storage (default `<repo>/core/saves`).
//! - `SHAKEDOWN_WORLDS`: where built worlds are written, installed from at start like a package
//!   root, and where the current benchmark is remembered (default `<repo>/core/worlds`).
//! - `SHAKEDOWN_TEMPLATES`: more template folders for the world builder (passed on to it).
//! - `SHAKEDOWN_RUN_<PACKAGE>`: a command line (`/bin/sh -c`) run in place of a package's `run`,
//!   e.g. `ssh <host> -- <command>` for a part on another machine (`docs/protocol.md`).
//!
//! `<repo>` is the folder above `core`, fixed when shaked is built (from `CARGO_MANIFEST_DIR`,
//! which is `<repo>/core/crates/shaked`, or `<repo>/core/crates/arenad-alias` for `arenad`), so
//! the defaults hold wherever shaked is started from.

use std::{path::PathBuf, sync::Arc};

use parts::{proto::BenchDescribe, Hub, Registry};
use shaked::{
    app::{self, AppState, Config},
    pg::Admin,
    store::Store,
};

fn repo_dir() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    here.canonicalize().unwrap_or(here)
}

fn env_path(key: &str, default: PathBuf) -> PathBuf {
    parts::settings::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

/// `SHAKEDOWN_WORLDS`, created, without the temporary folders a build left.
fn worlds_dir() -> anyhow::Result<PathBuf> {
    let dir = env_path("SHAKEDOWN_WORLDS", repo_dir().join("core/worlds"));
    std::fs::create_dir_all(&dir)?;
    shaked::worlds::remove_leftovers(&dir);
    Ok(dir)
}

/// What every command-mode program gets: `SHAKEDOWN_PG_URL`, and `SHAKEDOWN_TEMPLATES` when it is set.
fn program_env(pg_url: &str) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> =
        parts::settings::both("SHAKEDOWN_PG_URL", pg_url.to_string()).into();
    if let Some(t) = parts::settings::var("SHAKEDOWN_TEMPLATES")
        .ok()
        .filter(|t| !t.is_empty())
    {
        env.extend(parts::settings::both("SHAKEDOWN_TEMPLATES", t));
    }
    env
}

/// The hub over `SHAKEDOWN_PACKAGES` and the built worlds, `SHAKEDOWN_BIN_DIR` and `SHAKEDOWN_SAVES`.
fn hub(pg_url: &str, worlds: &std::path::Path) -> anyhow::Result<Hub> {
    let arena = repo_dir();
    let mut roots: Vec<PathBuf> = match parts::settings::var("SHAKEDOWN_PACKAGES") {
        Ok(p) if !p.is_empty() => std::env::split_paths(&p).collect(),
        _ => vec![arena.join("integrations"), arena.join("benchmarks")],
    };
    roots.push(worlds.to_path_buf());
    let bin_dir = env_path("SHAKEDOWN_BIN_DIR", arena.join("core/target/release"));
    let saves = env_path("SHAKEDOWN_SAVES", arena.join("core/saves"));
    std::fs::create_dir_all(&saves)?;
    let registry = Registry::scan(&roots)?;
    Ok(Hub::new(registry, bin_dir, saves)
        .with_repo_dir(arena)
        .with_env(program_env(pg_url)))
}

/// The store and the hub.
async fn app_state(url: &str, port: u16) -> anyhow::Result<Arc<AppState>> {
    let admin = Admin::connect(url).await?;
    let store = Arc::new(Store::open(&admin).await?);
    let worlds = worlds_dir()?;
    let hub = Arc::new(hub(url, &worlds)?);
    let mut config = Config::new();
    config.self_base = format!("http://127.0.0.1:{port}");
    config.worlds_dir = Some(worlds);
    Ok(Arc::new(AppState::new(store, hub, config)))
}

/// `SHAKEDOWN_PORT`, or 4510.
fn port_from_env() -> anyhow::Result<u16> {
    match parts::settings::var("SHAKEDOWN_PORT") {
        Ok(p) => p
            .parse()
            .map_err(|_| anyhow::anyhow!("SHAKEDOWN_PORT {p:?} is not a port number")),
        Err(_) => Ok(4510),
    }
}

/// The modes this machine can run, for the startup line.
fn available_modes(state: &AppState) -> Vec<&'static str> {
    [
        ("claude-code", shaked::api::on_path("claude")),
        ("codex", shaked::api::on_path("codex")),
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
/// exists, and exits.
async fn build_base(state: &AppState, bench: &BenchDescribe) -> anyhow::Result<()> {
    let saves = shaked::world::ensure_base(&state.hub, bench).await?;
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
async fn start_proxy(state: &std::sync::Arc<shaked::app::AppState>) -> anyhow::Result<()> {
    let proxy = tokio::net::TcpListener::bind(("127.0.0.1", proxy_port_from_env()?)).await?;
    println!("shaked: proxy on http://{}", proxy.local_addr()?);
    tokio::spawn(shaked::proxy::serve(state.clone(), proxy));
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = shaked::pg::configured_url().await;
    let port = port_from_env()?;
    let ui_dist = ui_dist_from_env();

    let state = app_state(&url, port).await?;
    let bench = state.startup_bench().await?;
    if std::env::args().nth(1).as_deref() == Some("base") {
        return build_base(&state, &bench).await;
    }
    // Before any run can save: drop the saves no event names (left by crashes and aborted tests).
    shaked::gc::run_startup_gc(&state.store, &state.hub, &state.benchmark()).await?;
    app::install_default_drivers(&state);

    let mut router = app::router(state.clone());
    if let Some(dist) = &ui_dist {
        router = shaked::api::with_ui(router, dist.clone());
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    start_proxy(&state).await?;
    println!(
        "shaked: listening on http://127.0.0.1:{port} (benchmark: {} {}; modes: {}; ui: {})",
        bench.name,
        bench.version,
        available_modes(&state).join(", "),
        ui_dist.map_or("api only".to_string(), |d| d.display().to_string())
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_on_ctrl_c(state))
        .await?;
    Ok(())
}
