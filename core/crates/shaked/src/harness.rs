//! For tests of shaked and of crates that embed it with a layer of their own (feature `harness`):
//! the route table, a layer that records every hook call, scratch databases and a served app.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use axum::{http::request::Parts, Router};
use futures_util::FutureExt;
use shakedown_schema::{migrate, Migration, Plan, Stream};

pub use crate::api::routes::{On, RouteCase, ROUTES};
use crate::{
    app::{AppState, Config},
    auth::{Action, Caller, Decision, SystemJob, Target},
    layer::*,
    pg::Admin,
    store::{Store, Tx},
};

/// One hook call a [`SpyLayer`] saw.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Authenticate,
    /// A transaction began: for a caller (None) or a system job.
    Begin(Option<SystemJob>),
    Authorize(Action),
    Admit(String),
    Meter(Meter, String),
    Fallback(String),
    /// An audit event: its action and its detail as JSON text.
    Audit(&'static str, String),
    Me,
}

/// What makes a spy's extra routes.
type MakeRoutes = fn() -> Router<Arc<AppState>>;

#[derive(Default)]
struct Spy {
    calls: Mutex<Vec<Call>>,
    decision: Mutex<Option<Decision>>,
    refuse: Mutex<Option<Refusal>>,
    admission: Mutex<Option<Admission>>,
    fallback: Mutex<Option<Fallback>>,
    routes: Mutex<Option<MakeRoutes>>,
    policy: std::sync::OnceLock<Policy>,
    headers: Mutex<Vec<String>>,
    shares_keys: std::sync::atomic::AtomicBool,
}

/// A layer that records every hook call; it allows everything unless told otherwise.
#[derive(Clone, Default)]
pub struct SpyLayer(Arc<Spy>);

impl SpyLayer {
    pub fn new() -> SpyLayer {
        SpyLayer::default()
    }

    /// `authorize` answers `d` to everything.
    pub fn deny(d: Decision) -> SpyLayer {
        let s = SpyLayer::new();
        *s.0.decision.lock().unwrap() = Some(d);
        s
    }

    /// `authenticate` refuses every request with 401.
    pub fn refuse_everyone() -> SpyLayer {
        let s = SpyLayer::new();
        let no = Refusal::new(axum::http::StatusCode::UNAUTHORIZED, "the spy refuses");
        *s.0.refuse.lock().unwrap() = Some(no);
        s
    }

    /// `admit` answers `a`.
    pub fn admitting(self, a: Admission) -> SpyLayer {
        *self.0.admission.lock().unwrap() = Some(a);
        self
    }

    /// `fallback_provider` lends `f`.
    pub fn lending(self, f: Fallback) -> SpyLayer {
        *self.0.fallback.lock().unwrap() = Some(f);
        self
    }

    /// `policy` answers `p` (else [`Policy::LOCAL`]); set once.
    pub fn with_policy(self, p: Policy) -> SpyLayer {
        let _ = self.0.policy.set(p);
        self
    }

    /// `own_keys` answers none: every key is someone else's, shared with the caller.
    pub fn sharing_keys(self) -> SpyLayer {
        (self.0.shares_keys).store(true, std::sync::atomic::Ordering::Relaxed);
        self
    }

    /// `routes` serves what `r` makes.
    pub fn with_routes(self, r: MakeRoutes) -> SpyLayer {
        *self.0.routes.lock().unwrap() = Some(r);
        self
    }

    /// The header names `authenticate` was shown, over every request.
    pub fn seen_headers(&self) -> Vec<String> {
        self.0.headers.lock().unwrap().clone()
    }

    pub fn calls(&self) -> Vec<Call> {
        self.0.calls.lock().unwrap().clone()
    }

    /// True when `authorize` was asked about `a`.
    pub fn asked(&self, a: Action) -> bool {
        self.calls().contains(&Call::Authorize(a))
    }

    /// How many transactions began.
    pub fn begins(&self) -> usize {
        let calls = self.calls();
        calls.iter().filter(|c| matches!(c, Call::Begin(_))).count()
    }

    fn saw(&self, c: Call) {
        self.0.calls.lock().unwrap().push(c);
    }
}

impl Layer for SpyLayer {
    fn policy(&self) -> &Policy {
        self.0.policy.get().unwrap_or(&Policy::LOCAL)
    }

    fn authenticate<'a>(
        &'a self,
        req: &'a Parts,
        found: Option<Caller>,
    ) -> BoxFuture<'a, Result<Caller, Refusal>> {
        self.saw(Call::Authenticate);
        let names = req.headers.keys().map(|k| k.as_str().to_string());
        self.0.headers.lock().unwrap().extend(names);
        let refused = self.0.refuse.lock().unwrap().clone();
        async move {
            match refused {
                Some(r) => Err(r),
                None => Ok(found.unwrap_or_else(Caller::person)),
            }
        }
        .boxed()
    }

    fn begin<'a>(&'a self, _tx: &'a Tx, on: TxFor<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        self.saw(Call::Begin(match on {
            TxFor::Caller(_) => None,
            TxFor::System(j) => Some(j),
        }));
        async { Ok(()) }.boxed()
    }

    fn authorize<'a>(
        &'a self,
        _c: &'a Caller,
        a: Action,
        _t: Target<'a>,
    ) -> BoxFuture<'a, Decision> {
        self.saw(Call::Authorize(a));
        let d = self.0.decision.lock().unwrap().clone();
        async move { d.unwrap_or(Decision::Allow) }.boxed()
    }

    fn admit<'a>(&'a self, _c: &'a Caller, op: Costly<'a>) -> BoxFuture<'a, Admission> {
        self.saw(Call::Admit(format!("{op:?}")));
        let a = self.0.admission.lock().unwrap().clone();
        async move { a.unwrap_or(Admission::Go) }.boxed()
    }

    fn meter<'a>(&'a self, _c: &'a Caller, u: Usage<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        self.saw(Call::Meter(u.meter, u.key.clone()));
        async { Ok(()) }.boxed()
    }

    fn fallback_provider<'a>(
        &'a self,
        _c: &'a Caller,
        want: ModelWant<'a>,
    ) -> BoxFuture<'a, Option<Fallback>> {
        self.saw(Call::Fallback(want.model.to_string()));
        let f = self.0.fallback.lock().unwrap().clone();
        async move { f }.boxed()
    }

    fn audit<'a>(
        &'a self,
        _tx: &'a Tx,
        _c: &'a Caller,
        e: AuditEvent<'a>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        self.saw(Call::Audit(e.action, e.detail.to_string()));
        async { Ok(()) }.boxed()
    }

    fn routes(&self) -> Router<Arc<AppState>> {
        match *self.0.routes.lock().unwrap() {
            Some(make) => make(),
            None => Router::new(),
        }
    }

    fn me<'a>(&'a self, c: &'a Caller) -> BoxFuture<'a, MeExtra> {
        self.saw(Call::Me);
        LocalLayer.me(c)
    }

    fn object_prefix(&self, c: &Caller) -> String {
        LocalLayer.object_prefix(c)
    }

    fn own_keys<'a>(
        &'a self,
        _c: &'a Caller,
        keys: &'a [uuid::Uuid],
    ) -> BoxFuture<'a, anyhow::Result<Vec<uuid::Uuid>>> {
        let shared = self
            .0
            .shares_keys
            .load(std::sync::atomic::Ordering::Relaxed);
        async move { Ok(if shared { vec![] } else { keys.to_vec() }) }.boxed()
    }
}

/// A scratch database `shakedown_test_<tag>_<id>` with `sets` migrated (every stream they hold,
/// in name order), dropped on drop. The server is `SHAKEDOWN_PG_URL`'s.
pub struct ScratchDb {
    pub name: String,
    /// The server's URL (`SHAKEDOWN_PG_URL`).
    pub server: String,
}

impl ScratchDb {
    /// The database's URL for the server's login.
    pub fn url(&self) -> String {
        Admin::lazy(&self.server).db_url(&self.name)
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        let (server, name) = (self.server.clone(), self.name.clone());
        let dropped = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            rt.map(|rt| rt.block_on(Admin::lazy(&server).drop_db(&name)))
        });
        if let Ok(Ok(Err(e))) = dropped.join() {
            eprintln!("test cleanup: dropping a scratch database: {e:#}");
        }
    }
}

/// Makes and migrates a [`ScratchDb`].
pub async fn scratch_db(tag: &str, sets: &[Migration]) -> ScratchDb {
    let server = crate::pg::configured_url().await;
    let admin = Admin::lazy(&server);
    let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let name = format!("shakedown_test_{tag}_{id}");
    admin
        .create_db_from(&name, "template1")
        .await
        .expect("creating a scratch database");
    let db = ScratchDb { name, server };
    let mut streams: Vec<Stream> = sets.iter().map(|m| m.stream).collect();
    streams.dedup();
    let plan = Plan {
        apply: &streams,
        require: &[],
        applied_by: "test",
    };
    let (mut c, conn) = tokio_postgres::connect(&db.url(), tokio_postgres::NoTls)
        .await
        .expect("connecting to the scratch database");
    let task = tokio::spawn(conn);
    migrate(&mut c, &plan, sets)
        .await
        .expect("migrating the scratch database");
    drop(c);
    let _ = task.await;
    db
}

/// A served app: dropping it stops the server.
pub struct ServeHandle {
    pub state: Arc<AppState>,
    task: tokio::task::JoinHandle<()>,
    _saves: PathBuf,
}

impl Drop for ServeHandle {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_dir_all(&self._saves);
    }
}

/// Serves `store` (its layer with it) with no packages on a free loopback port: its base URL.
pub async fn serve_app(store: Store) -> (String, ServeHandle) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let saves =
        std::env::temp_dir().join(format!("shaked-harness-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&saves).unwrap();
    let registry = parts::Registry::scan(&[]).unwrap();
    let hub = Arc::new(parts::Hub::new(
        registry,
        PathBuf::from("/nonexistent"),
        saves.clone(),
    ));
    let mut config = Config::new();
    config.self_base = base.clone();
    let state = Arc::new(AppState::new(Arc::new(store), hub, config));
    let router = crate::app::router(state.clone());
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let handle = ServeHandle {
        state,
        task,
        _saves: saves,
    };
    (base, handle)
}

/// Who a system job's transaction runs as: the login and whether it bypasses row-level security
/// (one fixed query; a test cannot open a system transaction of its own).
pub async fn system_job_login(store: &Store) -> anyhow::Result<(String, bool)> {
    let tx = store
        .system_tx(crate::auth::SystemScope::startup_gc())
        .await?;
    let sql = "SELECT current_user::text, rolbypassrls FROM pg_roles WHERE rolname = current_user";
    let row = tx.query_one(sql, &[]).await?;
    tx.commit().await?;
    Ok((row.get(0), row.get(1)))
}
