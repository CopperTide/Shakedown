//! The pieces every test app needs: a fresh metadata store (dropped by the returned `Cleanup`), a
//! listener on a free port, and serving the app's router on it.
#![allow(dead_code)]

use std::sync::Arc;

use parts::Hub;
use shaked::{
    app::{self, AppState, Config},
    providers::ProviderConfig,
    store::Store,
};

use super::pg::{admin, Cleanup};

/// A store in a fresh `shakedown_test_<kind>_<tag>_<id>` database, copied from the migrated
/// template.
pub async fn meta_store(kind: &str, tag: &str) -> (Arc<Store>, Cleanup) {
    let (_, store, clean) = meta_store_named(kind, tag).await;
    (store, clean)
}

/// `meta_store`, with the database's name.
pub async fn meta_store_named(kind: &str, tag: &str) -> (String, Arc<Store>, Cleanup) {
    let clean = Cleanup::new();
    let meta = super::pg::fresh_db(&format!("{kind}_{tag}"), &clean).await;
    let store = Store::open_named(&admin(), &meta).await.unwrap();
    (meta, Arc::new(store), clean)
}

/// A listener on a free loopback port, and its base URL.
pub async fn bind() -> (tokio::net::TcpListener, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    (listener, base)
}

/// Serves the app on `listener` for the rest of the test.
pub fn serve(listener: tokio::net::TcpListener, state: &Arc<AppState>) {
    let router = app::router(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    state.proxy.listening(proxy.local_addr().unwrap());
    let proxy = tokio::net::TcpListener::from_std(proxy).unwrap();
    tokio::spawn(shaked::proxy::serve(state.clone(), proxy));
}

/// A provider base nothing answers on: for apps that must offer open mode (a local provider
/// needs no key) but never call a model.
pub const NOWHERE: &str = "http://127.0.0.1:9";

/// An app on `hub` with the real drivers (Claude Code, Codex and shaked's own loop on the
/// providers the environment configures), served on a free port; `tweak` adjusts the config
/// first. The state and the app's base URL.
pub async fn app(
    store: Arc<Store>,
    hub: Arc<Hub>,
    tweak: impl FnOnce(&mut Config),
) -> (Arc<AppState>, String) {
    app_for(store, hub, None, tweak).await
}

/// `app` whose new runs use the benchmark `bench` (else the only one installed).
pub async fn app_for(
    store: Arc<Store>,
    hub: Arc<Hub>,
    bench: Option<&str>,
    tweak: impl FnOnce(&mut Config),
) -> (Arc<AppState>, String) {
    let (listener, base) = bind().await;
    let mut config = Config::new();
    config.self_base = base.clone();
    tweak(&mut config);
    let mut state = AppState::new(store, hub, config);
    if let Some(b) = bench {
        state = state.with_benchmark(b);
    }
    let state = Arc::new(state);
    assert!(app::install_default_drivers(&state));
    serve(listener, &state);
    (state, base)
}

/// `app` offering open mode on [`NOWHERE`] only, for tests that run no agent.
pub async fn idle_app(
    store: Arc<Store>,
    hub: Arc<Hub>,
    bench: Option<&str>,
    tweak: impl FnOnce(&mut Config),
) -> (Arc<AppState>, String) {
    app_for(store, hub, bench, |c| {
        c.providers = ProviderConfig::with_upstream(NOWHERE);
        tweak(c);
    })
    .await
}

/// The status and JSON body of a request to the app.
pub async fn send(req: reqwest::RequestBuilder) -> (u16, serde_json::Value) {
    let r = req.send().await.unwrap();
    (r.status().as_u16(), r.json().await.unwrap())
}
