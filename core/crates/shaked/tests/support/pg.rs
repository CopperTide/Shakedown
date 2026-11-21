//! Postgres for tests: the server URL, an admin handle, and a guard that drops the databases a
//! test made when it goes out of scope, also after a panic.
#![allow(dead_code)]

use std::{future::Future, sync::Mutex};

use shaked::pg::Admin;

/// The server the tests use: as shaked finds it (`SHAKEDOWN_PG_URL`, else the default user
/// `shakedown`, else the old `shakedown`), once per test binary.
pub fn pg_url() -> String {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| on_own_runtime(shaked::pg::configured_url))
        .clone()
}

pub fn admin() -> Admin {
    Admin::lazy(&pg_url())
}

/// Runs the future `f` makes on a thread and runtime of its own, so it works from inside a
/// test's runtime and from `Drop`, after a panic too.
pub fn on_own_runtime<T: Send, Fut: Future<Output = T>>(f: impl FnOnce() -> Fut + Send) -> T {
    std::thread::scope(|s| {
        s.spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(f())
        })
        .join()
        .unwrap()
    })
}

/// Drops `names` (a missing one is fine); failures are logged.
pub async fn drop_databases(names: &[String]) {
    let admin = admin();
    for n in names {
        if let Err(e) = admin.drop_db(n).await {
            eprintln!("test cleanup: could not drop {n}: {e:#}");
        }
    }
}

/// Drops every database it was told about when it goes out of scope.
#[derive(Default)]
pub struct Cleanup {
    names: Mutex<Vec<String>>,
}

impl Cleanup {
    pub fn new() -> Cleanup {
        Cleanup::default()
    }

    /// Drop this database at the end.
    pub fn add(&self, name: &str) {
        self.names.lock().unwrap().push(name.to_string());
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let names = std::mem::take(&mut *self.names.lock().unwrap());
        on_own_runtime(|| async move { drop_databases(&names).await });
    }
}

/// A unique `shakedown_test_<tag>_<id>` database name.
pub fn db_name(tag: &str) -> String {
    let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
    format!("shakedown_test_{tag}_{id}")
}

/// The template every test store is copied from: migrated and seeded once per schema
/// (`shaked::store::template_key`), under an advisory lock so test binaries share it. It is built
/// under a `_wip` name and renamed when done, so a killed build never leaves a half-made template;
/// other templates nobody uses are dropped.
async fn template() -> String {
    static NAME: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    NAME.get_or_init(|| async {
        let tpl = format!("shakedown_tpl_{}", shaked::store::template_key());
        let lock = admin_client().await;
        lock.batch_execute("SELECT pg_advisory_lock(hashtext('shakedown-test-template'))")
            .await
            .unwrap();
        let admin = admin();
        if !admin.exists(&tpl).await.unwrap() {
            build_template(&admin, &lock, &tpl).await;
        }
        lock.batch_execute("SELECT pg_advisory_unlock(hashtext('shakedown-test-template'))")
            .await
            .unwrap();
        tpl
    })
    .await
    .clone()
}

async fn build_template(admin: &Admin, lock: &tokio_postgres::Client, tpl: &str) {
    let wip = format!("{tpl}_wip");
    admin.drop_db(&wip).await.unwrap();
    admin.create_db_from(&wip, "template1").await.unwrap();
    shaked::store::prepare_db(admin, &wip).await.unwrap();
    lock.batch_execute(&format!("ALTER DATABASE \"{wip}\" RENAME TO \"{tpl}\""))
        .await
        .unwrap();
    let stale = "SELECT datname FROM pg_database WHERE datname LIKE 'shakedown\\_tpl\\_%' AND datname <> $1";
    for r in lock.query(stale, &[&tpl]).await.unwrap() {
        let name: String = r.get(0);
        // Without FORCE: a template another test binary is copying stays.
        let _ = lock
            .batch_execute(&format!("DROP DATABASE IF EXISTS \"{name}\""))
            .await;
    }
}

/// A connection to the server's `postgres` database.
async fn admin_client() -> tokio_postgres::Client {
    let (c, conn) = tokio_postgres::connect(&admin().db_url("postgres"), tokio_postgres::NoTls)
        .await
        .unwrap();
    tokio::spawn(conn);
    c
}

/// A fresh database named like `db_name(tag)`, copied from the migrated template, dropped by `clean`.
pub async fn fresh_db(tag: &str, clean: &Cleanup) -> String {
    let name = db_name(tag);
    let tpl = template().await;
    clean.add(&name);
    admin().create_db_from(&name, &tpl).await.unwrap();
    name
}

/// A store on a `fresh_db`, its name and the guard that drops it.
pub async fn fresh_store(tag: &str) -> (String, shaked::store::Store, Cleanup) {
    let clean = Cleanup::new();
    let name = fresh_db(tag, &clean).await;
    let store = shaked::store::Store::open_named(&admin(), &name)
        .await
        .unwrap();
    (name, store, clean)
}

/// A non-superuser login owning a database of its own; both are dropped on
/// drop. The password is a test value made here.
pub struct TempLogin {
    pub role: String,
    pub password: String,
    pub db: String,
    pub url: String,
}

impl TempLogin {
    pub async fn new(tag: &str) -> TempLogin {
        let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
        let role = format!("shakedown_test_login_{tag}_{id}");
        let password = format!("test-{id}");
        let db = db_name(tag);
        let c = admin_client().await;
        let role_sql = format!("CREATE ROLE \"{role}\" LOGIN PASSWORD '{password}'");
        c.batch_execute(&role_sql).await.unwrap();
        let db_sql = format!("CREATE DATABASE \"{db}\" OWNER \"{role}\"");
        c.batch_execute(&db_sql).await.unwrap();
        let url = with_user(&pg_url(), &role, &password);
        TempLogin {
            role,
            password,
            db,
            url,
        }
    }
}

impl Drop for TempLogin {
    fn drop(&mut self) {
        let (db, role) = (self.db.clone(), self.role.clone());
        on_own_runtime(|| async move {
            let c = admin_client().await;
            let mut sqls = vec![];
            if !db.is_empty() {
                sqls.push(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"));
            }
            sqls.push(format!("DROP OWNED BY \"{role}\""));
            sqls.push(format!("DROP ROLE IF EXISTS \"{role}\""));
            for sql in sqls {
                if let Err(e) = c.batch_execute(&sql).await {
                    eprintln!("test cleanup: {sql}: {e}");
                }
            }
        });
    }
}

/// `url` with its user and password replaced.
fn with_user(url: &str, user: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a postgres:// URL");
    let host = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
    format!("{scheme}://{user}:{password}@{host}")
}

/// A connection to database `db` as the test login (a superuser: no row-level security).
async fn db_client(db: &str) -> (tokio_postgres::Client, tokio::task::JoinHandle<()>) {
    let (c, conn) = tokio_postgres::connect(&admin().db_url(db), tokio_postgres::NoTls)
        .await
        .unwrap();
    let task = tokio::spawn(async move {
        let _ = conn.await;
    });
    (c, task)
}

/// Runs `sql` (many statements are fine) in database `db` as the test login.
pub async fn execute(db: &str, sql: &str) {
    let (c, task) = db_client(db).await;
    c.batch_execute(sql)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    drop(c);
    let _ = task.await;
}

/// The first column of the one row `sql` returns, as text (None for SQL NULL).
pub async fn query_text(db: &str, sql: &str) -> Option<String> {
    let (c, task) = db_client(db).await;
    let row = c
        .query_one(&format!("SELECT ({sql})::text"), &[])
        .await
        .unwrap();
    drop(c);
    let _ = task.await;
    row.get(0)
}

/// The first column of the last row the statements in `sql` return (simple-query protocol).
pub async fn batch_last_text(db: &str, sql: &str) -> Option<String> {
    let (c, task) = db_client(db).await;
    let msgs = c.simple_query(sql).await.unwrap();
    drop(c);
    let _ = task.await;
    msgs.iter().rev().find_map(|m| match m {
        tokio_postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
        _ => None,
    })
}
