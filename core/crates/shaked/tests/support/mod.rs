//! What the integration tests share: one runtime to run them on, an app served on a free port
//! (`AppFx`), and the support modules (Postgres, packages, the live model, old records).
#![allow(dead_code)]

pub mod app;
pub mod fake_llm;
pub mod legacy;
pub mod live;
pub mod live_world;
pub mod orgs;
pub mod packages;
pub mod pg;

use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use serde_json::Value;
use shaked::{
    app::{AppState, Config},
    providers::ProviderConfig,
};

use live::Live;
use packages::{TestHub, ToyRoot};
use pg::Cleanup;

static RT: LazyLock<tokio::runtime::Runtime> =
    LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

/// The gateway secret hosted test apps verify signatures with (a test value).
pub const TEST_GATEWAY_SECRET: &[u8] = b"shakedown-test-gateway-secret";

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
    /// The metadata database's name.
    pub db: String,
    /// The org fixtures, seeded into a hosted app's database.
    orgs: Option<orgs::Orgs>,
    root: Option<ToyRoot>,
    _clean: Cleanup,
    _cas: CaCleanup,
}

type Tweak = Box<dyn FnOnce(&mut Config) + Send>;

/// How to make an [`AppFx`]: see [`AppFx::idle`] and [`AppFx::configured`].
pub struct AppSetup {
    kind: String,
    tag: String,
    configured: bool,
    hosted: bool,
    hub: Option<TestHub>,
    root: Option<ToyRoot>,
    bench: Option<String>,
    tweak: Tweak,
}

impl AppFx {
    /// An app that runs no agent (open mode offered on [`app::NOWHERE`] only); its store is
    /// `shakedown_test_<kind>_<tag>_…`.
    pub fn idle(kind: &str, tag: &str) -> AppSetup {
        AppSetup {
            kind: kind.into(),
            tag: tag.into(),
            configured: false,
            hosted: false,
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

    /// The org fixtures of a hosted app.
    pub fn orgs(&self) -> &orgs::Orgs {
        self.orgs.as_ref().expect("a hosted app (AppSetup::hosted)")
    }

    /// `method path` with `body`, as the gateway signs it for fixture user `user` in their org now.
    pub async fn signed(
        &self,
        method: &str,
        path: &str,
        user: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let ts = chrono::Utc::now().timestamp();
        app::send(self.signed_request(method, path, user, ts, TEST_GATEWAY_SECRET, body)).await
    }

    /// `signed` at the unix time `ts`, without a body.
    pub async fn signed_at(&self, method: &str, path: &str, user: &str, ts: i64) -> (u16, Value) {
        app::send(self.signed_request(method, path, user, ts, TEST_GATEWAY_SECRET, None)).await
    }

    /// `signed` with another secret, without a body.
    pub async fn signed_with(
        &self,
        method: &str,
        path: &str,
        user: &str,
        secret: &[u8],
    ) -> (u16, Value) {
        let ts = chrono::Utc::now().timestamp();
        app::send(self.signed_request(method, path, user, ts, secret, None)).await
    }

    /// `method path` with no signature or token.
    pub async fn unsigned(&self, method: &str, path: &str) -> (u16, Value) {
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        app::send(self.http.request(m, format!("{}{path}", self.base))).await
    }

    fn signed_request(
        &self,
        method: &str,
        path: &str,
        user: &str,
        ts: i64,
        secret: &[u8],
        body: Option<Value>,
    ) -> reqwest::RequestBuilder {
        use shaked::auth::signing::*;
        let (u, org) = (
            self.orgs().user(user),
            self.orgs().org(&self.orgs().user(user).org),
        );
        let (uid, oid) = (u.id.to_string(), org.id.to_string());
        let sig = sign_v2(secret, method, path, &uid, &oid, ts);
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let req = self.http.request(m, format!("{}{path}", self.base));
        let req = req.header(USER_HEADER, uid).header(ORG_HEADER, oid);
        let req = req
            .header(TS_HEADER, ts.to_string())
            .header(SIG_HEADER, sig);
        match body {
            Some(b) => req.json(&b),
            None => req,
        }
    }
}

/// The app's state outlives the test (its servers run on the shared runtime): its branches' CA
/// folders are removed when the fixture drops.
struct CaCleanup(Arc<AppState>);

impl Drop for CaCleanup {
    fn drop(&mut self) {
        self.0.proxy.cas.clear();
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

    /// A hosted app: the org fixtures seeded, requests signed with [`TEST_GATEWAY_SECRET`].
    pub fn hosted(mut self) -> AppSetup {
        self.hosted = true;
        self.config(|c| {
            c.mode = shaked::mode::Mode::Hosted;
            c.gateway_secret = Some(shaked::auth::signing::GatewaySecret(
                TEST_GATEWAY_SECRET.to_vec(),
            ));
        })
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
        let (db, store, clean) = app::meta_store_named(&self.kind, &self.tag).await;
        let orgs = match self.hosted {
            true => Some(orgs::seed(&db).await),
            false => None,
        };
        let (h, bench) = (hub.hub.clone(), self.bench.as_deref());
        let (state, base) = match self.configured {
            true => app::app_for(store, h, bench, self.tweak).await,
            false => app::idle_app(store, h, bench, self.tweak).await,
        };
        AppFx {
            _cas: CaCleanup(state.clone()),
            state,
            base,
            http: reqwest::Client::new(),
            hub,
            db,
            orgs,
            root: self.root,
            _clean: clean,
        }
    }
}

/// The local owner's scope: what every local request runs as.
pub fn local() -> &'static shaked::auth::Scope {
    static LOCAL: LazyLock<shaked::auth::Scope> = LazyLock::new(shaked::auth::local_owner);
    &LOCAL
}

/// `store`'s records as the local owner.
pub fn records(store: &shaked::store::Store) -> shaked::store::Scoped<'_> {
    store.at(local())
}
