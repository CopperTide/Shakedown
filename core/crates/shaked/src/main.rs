//! shaked, the Shakedown service (`cargo run -p shaked` from `core`). `core/crates/arenad-alias` builds
//! this same file as `arenad`, its name until 2026-11-19.
//!
//! Environment: `SHAKEDOWN_PG_URL`, `SHAKEDOWN_BIND` (`127.0.0.1:4510`; the old `SHAKEDOWN_PORT`
//! still binds `127.0.0.1:<port>`), `SHAKEDOWN_PUBLIC_URL` (the base of every URL handed out;
//! default `http://<bind>`), `SHAKEDOWN_ALLOWED_HOSTS` (more Host names `/api` answers; see
//! `shaked::mode`), `SHAKEDOWN_MODE=hosted` or `--hosted` (a bind beyond loopback needs it),
//! `SHAKEDOWN_PROXY_PORT` (the recording proxy,
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
    mode::{Bind, Mode},
    pg::Admin,
    store::{DbUrls, Store},
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
async fn app_state(url: &str, bind: &Bind, mode: Mode) -> anyhow::Result<Arc<AppState>> {
    let store = Arc::new(match mode {
        Mode::Local => Store::open(&Admin::lazy(url)).await?,
        Mode::Hosted => {
            Store::open_hosted(&DbUrls::from_env(|k| parts::settings::var(k).ok())).await?
        }
    });
    let worlds = worlds_dir()?;
    let hub = Arc::new(hub(url, &worlds)?);
    let mut config = Config::new();
    config.self_base = format!("http://{}", loopback_of(bind.addr));
    config.public_url = bind.public_url.to_string();
    config.allowed_hosts = bind.allowed_hosts.clone();
    config.mode = mode;
    config.worlds_dir = Some(worlds);
    if mode == Mode::Hosted {
        config.gateway_secret = Some(gateway_secret()?);
        config.switch_org_url = parts::settings::var("SHAKEDOWN_SWITCH_ORG_URL").ok();
    }
    Ok(Arc::new(AppState::new(store, hub, config)))
}

/// The Cloud gateway's signing secret, from `SHAKEDOWN_GATEWAY_SECRET_FILE` (hosted mode needs it).
fn gateway_secret() -> anyhow::Result<shaked::auth::signing::GatewaySecret> {
    use anyhow::Context;
    let path = parts::settings::var("SHAKEDOWN_GATEWAY_SECRET_FILE")
        .context("hosted mode needs SHAKEDOWN_GATEWAY_SECRET_FILE")?;
    let secret = std::fs::read(&path).with_context(|| format!("reading {path}"))?;
    let secret = String::from_utf8_lossy(&secret).trim().as_bytes().to_vec();
    anyhow::ensure!(
        secret.len() >= 32,
        "the gateway secret in {path} is shorter than 32 bytes"
    );
    Ok(shaked::auth::signing::GatewaySecret(secret))
}

/// Where shaked reaches itself: `addr`, or loopback on its port when it listens on every address.
fn loopback_of(addr: std::net::SocketAddr) -> std::net::SocketAddr {
    match addr.ip().is_unspecified() {
        true => std::net::SocketAddr::from(([127, 0, 0, 1], addr.port())),
        false => addr,
    }
}

/// The mode (`--hosted`, `SHAKEDOWN_MODE`) and where to listen.
fn mode_and_bind() -> anyhow::Result<(Mode, Bind)> {
    let mode = Mode::from_args_and_env(
        std::env::args(),
        parts::settings::var("SHAKEDOWN_MODE").ok(),
    );
    let bind = shaked::mode::bind_from(|k| parts::settings::var(k).ok(), mode == Mode::Hosted)
        .map_err(anyhow::Error::msg)?;
    if mode == Mode::Hosted {
        // Hosted needs the org-aware store and request auth, which land later in milestone 1.
        anyhow::bail!(
            "hosted mode is not available in this build yet: {}",
            shaked::mode::HOSTED_REFUSAL
        );
    }
    Ok((mode, bind))
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

/// The startup line: where shaked listens, as which URL, its benchmark, modes and UI.
fn listening_line(
    state: &AppState,
    bind: &Bind,
    bench: Option<&BenchDescribe>,
    ui: Option<&std::path::Path>,
) -> String {
    let bench = bench.map_or("none (hosted)".to_string(), |b| {
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

/// A local shaked's start: its benchmark, and before any run can save, dropping the saves no
/// event names (left by crashes and aborted tests).
async fn local_start(state: &AppState) -> anyhow::Result<BenchDescribe> {
    let bench = state.startup_bench().await?;
    shaked::gc::run_startup_gc(&state.store, &state.hub, &state.benchmark()).await?;
    Ok(bench)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(done) = shaked::cli::run(&args).await {
        return done;
    }
    let (mode, bind) = mode_and_bind()?;
    let url = shaked::pg::configured_url().await;
    let ui_dist = ui_dist_from_env();

    let state = app_state(&url, &bind, mode).await?;
    let bench = match mode {
        Mode::Local => Some(local_start(&state).await?),
        // A hosted shaked starts no package program on the control box (they run in sandboxes).
        Mode::Hosted => None,
    };
    if let (Some(bench), Some("base")) = (&bench, std::env::args().nth(1).as_deref()) {
        return build_base(&state, bench).await;
    }
    app::install_default_drivers(&state);

    let mut router = app::router(state.clone());
    if let Some(dist) = &ui_dist {
        router = shaked::api::with_ui(router, dist.clone());
    }
    let listener = tokio::net::TcpListener::bind(bind.addr).await?;
    start_proxy(&state).await?;
    println!(
        "{}",
        listening_line(&state, &bind, bench.as_ref(), ui_dist.as_deref())
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_on_ctrl_c(state))
        .await?;
    Ok(())
}
