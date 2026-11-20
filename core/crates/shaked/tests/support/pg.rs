//! Postgres for tests: the server URL, an admin handle, and a guard that drops the databases a
//! test made when it goes out of scope, also after a panic.
#![allow(dead_code)]

use std::{future::Future, sync::Mutex};

use shaked::pg::Admin;

/// The server the tests use: as shaked finds it (`SHAKEDOWN_PG_URL`, else the default user
/// `shakedown`, else the old `arena`), once per test binary.
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
