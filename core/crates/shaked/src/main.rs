//! shaked, the Shakedown service (`cargo run -p shaked` from `core`). Its other commands
//! (`base`, `token`, `store`, `mcp`, `worker`) are in `shaked::cli`.
//!
//! Environment: `SHAKEDOWN_PG_URL`, `SHAKEDOWN_BIND` (`127.0.0.1:4510`; the old `SHAKEDOWN_PORT`
//! still binds `127.0.0.1:<port>`), `SHAKEDOWN_PUBLIC_URL` (the base of every URL handed out;
//! default `http://<bind>`), `SHAKEDOWN_ALLOWED_HOSTS` (more Host names `/api` answers; see
//! `shaked::bind`; a bind beyond loopback is refused), `SHAKEDOWN_PROXY_PORT` (the recording proxy,
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
//! - `SHAKEDOWN_PROJECT`: the project folder whose worlds, scenarios, setups and experiments shaked
//!   serves (`shaked mcp` reads it too). Unset: the project operations answer 409.
//! - `SHAKEDOWN_WORLDS`: where built worlds are written, installed from at start like a package
//!   root, and where the current benchmark is remembered (default `<repo>/core/worlds`).
//! - `SHAKEDOWN_TEMPLATES`: more template folders for the world builder (passed on to it).
//! - `SHAKEDOWN_RUN_<PACKAGE>`: a command line (`/bin/sh -c`) run in place of a package's `run`,
//!   e.g. `ssh <host> -- <command>` for a part on another machine (`docs/protocol.md`).
//!
//! `<repo>` is the folder above `core`, fixed when shaked is built (from `CARGO_MANIFEST_DIR`,
//! which is `<repo>/core/crates/shaked`), so
//! the defaults hold wherever shaked is started from.

use std::sync::Arc;

use shaked::{layer::LocalLayer, pg::Admin, store::Store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(done) = shaked::cli::run(&args).await {
        return done;
    }
    let opts = shaked::serve::ServeOptions::from_env(&shaked::layer::Policy::LOCAL).await?;
    let store = Store::open(&Admin::lazy(&opts.pg_url)).await?;
    shaked::serve::run(store.with_layer(Arc::new(LocalLayer)), opts).await
}
