//! Scratch databases for the migrator's tests: each test gets its own, dropped when the guard goes
//! out of scope, also after a panic. Named `shakedown_test_schema_…`, which `core/scripts/leakcheck.sh`
//! counts.
#![allow(dead_code)]

use std::future::Future;

use tokio::sync::OnceCell;
use tokio_postgres::{Client, NoTls};

/// The server URL (no database): `SHAKEDOWN_PG_URL`, else the compose server's user `shakedown`,
/// else its old user `arena`.
pub async fn server() -> String {
    static URL: OnceCell<String> = OnceCell::const_new();
    URL.get_or_init(|| async {
        if let Ok(u) = std::env::var("SHAKEDOWN_PG_URL") {
            return strip_db(&u);
        }
        for u in [
            "postgres://shakedown:shakedown@127.0.0.1:5544",
            "postgres://arena:arena@127.0.0.1:5544",
        ] {
            if tokio_postgres::connect(&format!("{u}/postgres"), NoTls)
                .await
                .is_ok()
            {
                return u.to_string();
            }
        }
        panic!("no Postgres on 127.0.0.1:5544 (docker start deploy-pg-1)")
    })
    .await
    .clone()
}

fn strip_db(url: &str) -> String {
    let url = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    let i = url.find("://").map_or(0, |i| i + 3);
    match url[i..].find('/') {
        Some(j) => url[..i + j].to_string(),
        None => url.to_string(),
    }
}

/// Connects to `db` on the test server; the connection task runs in the background.
pub async fn connect(db: &str) -> Client {
    let (client, conn) = tokio_postgres::connect(&format!("{}/{db}", server().await), NoTls)
        .await
        .unwrap_or_else(|e| panic!("connecting to {db}: {e}"));
    tokio::spawn(conn);
    client
}

/// A database of this test's own, dropped on drop.
pub struct Scratch {
    pub name: String,
    server: String,
}

impl Scratch {
    pub async fn new(tag: &str) -> Scratch {
        let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
        let name = format!("shakedown_test_schema_{tag}_{id}");
        let admin = connect("postgres").await;
        admin
            .batch_execute(&format!("CREATE DATABASE \"{name}\""))
            .await
            .unwrap();
        Scratch {
            name,
            server: server().await,
        }
    }

    /// `new`, with the bootstrap roles and schemas (the test login is a superuser).
    pub async fn bootstrapped(tag: &str) -> Scratch {
        let db = Scratch::new(tag).await;
        db.connect()
            .await
            .batch_execute(shakedown_schema::BOOTSTRAP_SQL)
            .await
            .unwrap();
        db
    }

    pub async fn connect(&self) -> Client {
        connect(&self.name).await
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let (server, name) = (self.server.clone(), self.name.clone());
        on_own_runtime(|| async move {
            let url = format!("{server}/postgres");
            match tokio_postgres::connect(&url, NoTls).await {
                Ok((c, conn)) => {
                    tokio::spawn(conn);
                    let sql = format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)");
                    if let Err(e) = c.batch_execute(&sql).await {
                        eprintln!("test cleanup: could not drop {name}: {e}");
                    }
                }
                Err(e) => eprintln!("test cleanup: could not connect to drop {name}: {e}"),
            }
        });
    }
}

/// Runs the future `f` makes on a thread and runtime of its own (works from `Drop` after a panic).
pub fn on_own_runtime<T: Send, Fut: Future<Output = T>>(f: impl FnOnce() -> Fut + Send) -> T {
    std::thread::scope(|s| {
        s.spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(f())
        })
        .join()
        .unwrap()
    })
}

/// The shape of schema `schema`: every column (name, type, nullable, default), constraint and
/// index, sorted, with names printed schema-qualified.
pub async fn fingerprint(c: &Client, schema: &str) -> Vec<String> {
    c.batch_execute("SET search_path = pg_catalog")
        .await
        .unwrap();
    let sql = "
      SELECT 'col ' || c.relname || '.' || a.attname || ' ' || format_type(a.atttypid, a.atttypmod)
             || CASE WHEN a.attnotnull THEN ' not null' ELSE '' END
             || coalesce(' default ' || pg_get_expr(d.adbin, d.adrelid), '')
        FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid
        LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
       WHERE c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1) AND c.relkind = 'r' AND a.attnum > 0 AND NOT a.attisdropped
      UNION ALL
      SELECT 'con ' || c.relname || '.' || k.conname || ' ' || pg_get_constraintdef(k.oid)
        FROM pg_constraint k JOIN pg_class c ON c.oid = k.conrelid
       WHERE c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1)
      UNION ALL
      SELECT 'idx ' || pg_get_indexdef(i.indexrelid)
        FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid
       WHERE c.relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = $1)
      ORDER BY 1";
    let rows = c.query(sql, &[&schema]).await.unwrap();
    c.batch_execute("RESET search_path").await.unwrap();
    rows.iter().map(|r| r.get(0)).collect()
}

/// `count(*)` of `sql`'s rows.
pub async fn count(c: &Client, sql: &str) -> i64 {
    c.query_one(&format!("SELECT count(*) FROM ({sql}) q"), &[])
        .await
        .unwrap()
        .get(0)
}

/// Applies CopperTide-Cloud's migrations 0001–0015 (copied from its main) into `public`.
pub async fn apply_cloud_0015(c: &Client) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cloud-0015");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 15);
    for f in &files {
        c.batch_execute(&std::fs::read_to_string(f).unwrap())
            .await
            .unwrap();
    }
}

/// True when the table `rel` (schema-qualified) exists.
pub async fn exists(c: &Client, rel: &str) -> bool {
    c.query_one("SELECT to_regclass($1) IS NOT NULL", &[&rel])
        .await
        .unwrap()
        .get(0)
}
