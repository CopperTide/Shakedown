//! What the integration tests share: one runtime to run them on, an app served on a free port
//! (`AppFx`), waiting for what the app does on its own (`poll_until`), and the support modules
//! (Postgres, packages, the live model, the worker link, the example project, old records).
#![allow(dead_code)]

pub mod app;
pub mod driven;
pub mod fake_apis;
pub mod fake_llm;
pub mod legacy;
pub mod link;
pub mod live;
pub mod live_world;
pub mod packages;
pub mod pg;
pub mod project;

use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::Duration,
};

use serde_json::{json, Value};
use shaked::{
    app::{AppState, Config},
    model::{AgentKind, AgentSpec, Event, EventKind, Team},
    providers::ProviderConfig,
};

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

/// How long a test waits for the app's own background work (a branch ended, a call recorded, a
/// driver killed): three times the 10 s these waits allowed by counting polls.
pub const WAIT: Duration = Duration::from_secs(30);
/// How long a test waits for a run of task 27 to start or be scored on its own hub. Those runs
/// build the task's base and gold saves (each hub its own, `world::standard`), so on a busy box
/// that alone can take minutes. Such a test checks what it means in world time on the world clock.
pub const SAVES_BUILT: Duration = Duration::from_secs(300);
/// How late (world ms) a window's `time_up` may come after the window's end: the watch looks at
/// the clock every 100 ms of real time, and a busy box wakes it late.
pub const TIME_UP_LATE_MS: i64 = 3000;
/// How often a test looks again at what it waits for.
pub const STEP: Duration = Duration::from_millis(100);

/// Polls `ready` every `step` until it gives a value, for at most `limit` of wall time (a
/// deadline, so slow polls never stretch it): the value, or None when the time ran out.
pub async fn poll_until<T>(
    limit: Duration,
    step: Duration,
    mut ready: impl AsyncFnMut() -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if let Some(v) = ready().await {
            return Some(v);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(step).await;
    }
}

/// Writes the executable `sh` script `path` running `body`. A child process writes it, never a
/// descriptor of this one: a test thread forking meanwhile would hold that descriptor open for
/// writing until its child execs, and running the script then fails "Text file busy" (ETXTBSY),
/// the more often the busier the box.
pub fn write_script(path: &std::path::Path, body: &str) {
    let write = r#"printf '#!/bin/sh\n%s\n' "$1" > "$0" && chmod 755 "$0""#;
    let status = std::process::Command::new("sh")
        .args(["-c", write])
        .arg(path)
        .arg(body)
        .status()
        .unwrap();
    assert!(status.success(), "writing the script {}", path.display());
}

/// The team `t` ("T") of `agents`.
pub fn team_of(agents: Vec<AgentSpec>) -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents,
    }
}

/// The team `t` of one open agent `a` ("A", prompt "p"), for records no agent runs.
pub fn team() -> Team {
    team_of(vec![AgentSpec::new("a", AgentKind::Open)
        .with_name("A")
        .with_prompt("p")])
}

/// The events of `evs` of `kind`.
pub fn of_kind(evs: &[Event], kind: EventKind) -> Vec<&Event> {
    evs.iter().filter(|e| e.kind == kind).collect()
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
    hub: Option<TestHub>,
    root: Option<ToyRoot>,
    bench: Option<String>,
    layer: Option<Arc<dyn shaked::layer::Layer>>,
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
            hub: None,
            root: None,
            bench: None,
            layer: None,
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

    /// `POST /api/branches/{branch}/fork` with `body`.
    pub async fn fork(&self, branch: &str, body: Value) -> (u16, Value) {
        self.post(&format!("/api/branches/{branch}/fork"), body)
            .await
    }

    /// An API token of `scopes`, as the person mints it.
    pub async fn mint_token(&self, scopes: &[&str]) -> String {
        let body = json!({"name": scopes.join(","), "scopes": scopes});
        let (_, minted) = self.post("/api/tokens", body).await;
        minted["token"].as_str().expect("a token").to_string()
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

    /// The store calls `layer` at its hooks.
    pub fn layer(mut self, layer: Arc<dyn shaked::layer::Layer>) -> AppSetup {
        self.layer = Some(layer);
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

    /// Opens the project folder `dir` (as `SHAKEDOWN_PROJECT` would).
    pub fn project(self, dir: &std::path::Path) -> AppSetup {
        let dir = dir.to_path_buf();
        self.config(move |c| c.project_dir = Some(dir))
    }

    /// Like [`config`](Self::config), setting the providers.
    pub fn providers(self, providers: ProviderConfig) -> AppSetup {
        self.config(|c| c.providers = providers)
    }

    pub async fn start(self) -> AppFx {
        let hub = self.hub.unwrap_or_else(|| TestHub::new(&[]));
        let (db, store, clean) = app::meta_store_with(&self.kind, &self.tag, self.layer).await;
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
            root: self.root,
            _clean: clean,
        }
    }
}

/// The person at this machine: what every local request runs as.
pub fn local() -> &'static shaked::auth::Caller {
    static LOCAL: LazyLock<shaked::auth::Caller> = LazyLock::new(shaked::auth::Caller::person);
    &LOCAL
}

/// `store`'s records as the person at this machine.
pub fn records(store: &shaked::store::Store) -> shaked::store::Scoped<'_> {
    store.at(local())
}

/// A hand-made branch recording into `store` for its caller, as `begin_run` gives every branch.
pub fn in_store(
    ctx: shaked::exec::BranchCtx,
    store: &std::sync::Arc<shaked::store::Store>,
) -> shaked::exec::BranchCtx {
    let (caller, id) = (ctx.caller.clone(), ctx.branch_id.clone());
    let records = shaked::exec::StoreRecords::new(store.clone(), caller, id);
    ctx.with_records(std::sync::Arc::new(records))
}
