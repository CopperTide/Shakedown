//! What the integration tests share: one runtime to run them on, an app served on a free port
//! (`AppFx`), and the support modules (Postgres, packages, the live model, old records).
#![allow(dead_code)]

pub mod app;
pub mod legacy;
pub mod live;
pub mod live_world;
pub mod packages;
pub mod pg;

use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use arenad::{
    app::{AppState, Config},
    providers::ProviderConfig,
};
use serde_json::Value;

use live::Live;
use packages::{TestHub, ToyRoot};
use pg::Cleanup;

static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

/// Runs `f` to the end on the tests' shared runtime.
pub fn run<F: Future>(f: F) -> F::Output {
    RT.block_on(f)
}

/// An event's payload as its writer made it: without the `overhead_ms` the store adds.
pub fn written(payload: &Value) -> Value {
    let mut p = payload.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("overhead_ms");
    }
    p
}

/// The keys of an event's payload as its writer made it, sorted.
pub fn keys(payload: &Value) -> Vec<String> {
    let p = written(payload);
    p.as_object()
        .map_or(vec![], |o| o.keys().cloned().collect())
}

/// Runs `body` on the live model, when there is one (else the test was skipped, said why).
pub fn run_live<F: Future<Output = ()>>(live: Option<Live>, body: impl FnOnce(Live) -> F) {
    if let Some(live) = live {
        run(body(live));
    }
}

/// An app with the real drivers, served on a free port, on a fresh metadata store and a hub of
/// its own. Dropping it drops the state, then the hub's instances and saves, the toy packages it
/// was given and last the app's databases.
pub struct AppFx {
    pub state: Arc<AppState>,
    pub base: String,
    pub http: reqwest::Client,
    pub hub: TestHub,
    root: Option<ToyRoot>,
    _clean: Cleanup,
}

type Tweak = Box<dyn FnOnce(&mut Config) + Send>;

/// How to make an [`AppFx`]: see [`AppFx::idle`] and [`AppFx::configured`].
pub struct AppSetup {
    kind: String,
    tag: String,
    configured: bool,
    hub: Option<TestHub>,
    root: Option<ToyRoot>,
    bench: Option<String>,
    tweak: Tweak,
}

impl AppFx {
    /// An app that runs no agent (open mode offered on [`app::NOWHERE`] only); its store is
    /// `arena_test_<kind>_<tag>_…`.
    pub fn idle(kind: &str, tag: &str) -> AppSetup {
        AppSetup {
            kind: kind.into(),
            tag: tag.into(),
            configured: false,
            hub: None,
            root: None,
            bench: None,
            tweak: Box::new(|_| {}),
        }
    }

    /// An app on the providers the environment configures (for tests that run agents).
    pub fn configured(kind: &str, tag: &str) -> AppSetup {
        AppSetup {
            configured: true,
            ..AppFx::idle(kind, tag)
        }
    }

    pub async fn get(&self, path: &str) -> (u16, Value) {
        app::send(self.http.get(format!("{}{path}", self.base))).await
    }

    pub async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        app::send(self.http.post(format!("{}{path}", self.base)).json(&body)).await
    }

    pub async fn delete(&self, path: &str) -> (u16, Value) {
        app::send(self.http.delete(format!("{}{path}", self.base))).await
    }
}

impl AppSetup {
    /// The hub to use (else one over the real packages).
    pub fn hub(mut self, hub: TestHub) -> AppSetup {
        self.hub = Some(hub);
        self
    }

    /// A hub over the real packages plus `extra` roots.
    pub fn packages(self, extra: &[PathBuf]) -> AppSetup {
        self.hub(TestHub::new(extra))
    }

    /// A hub over the real packages plus the toy packages of `root`, kept as long as the app.
    pub fn root(mut self, root: ToyRoot) -> AppSetup {
        self = self.packages(std::slice::from_ref(&root.0));
        self.root = Some(root);
        self
    }

    /// New runs use the benchmark `bench` (else the only one installed).
    pub fn bench(mut self, bench: &str) -> AppSetup {
        self.bench = Some(bench.into());
        self
    }

    /// Adjusts the app's config before it starts (after the adjustments asked for before).
    pub fn config(mut self, tweak: impl FnOnce(&mut Config) + Send + 'static) -> AppSetup {
        let before = self.tweak;
        self.tweak = Box::new(move |c| {
            before(c);
            tweak(c);
        });
        self
    }

    /// Like [`config`](Self::config), setting the providers.
    pub fn providers(self, providers: ProviderConfig) -> AppSetup {
        self.config(|c| c.providers = providers)
    }

    pub async fn start(self) -> AppFx {
        let hub = self.hub.unwrap_or_else(|| TestHub::new(&[]));
        let (store, clean) = app::meta_store(&self.kind, &self.tag).await;
        let (h, bench) = (hub.hub.clone(), self.bench.as_deref());
        let (state, base) = match self.configured {
            true => app::app_for(store, h, bench, self.tweak).await,
            false => app::idle_app(store, h, bench, self.tweak).await,
        };
        AppFx {
            state,
            base,
            http: reqwest::Client::new(),
            hub,
            root: self.root,
            _clean: clean,
        }
    }
}
