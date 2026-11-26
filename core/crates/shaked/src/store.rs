//! The metadata store: [`Store`] over its pools and layer, and [`Scoped`], the record methods for
//! one caller, each in its own transaction. The records themselves are in the submodules.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use deadpool_postgres::Pool;
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio_postgres::Row;

use crate::{
    auth::Caller,
    layer::{Layer, LocalLayer},
    model::Event,
    pg::Admin,
};

mod analytics;
mod api_tokens;
mod connections;
mod control;
mod events;
mod experiments;
mod linked;
mod obs_exports;
mod obs_metrics;
mod observability;
mod open;
mod provider_keys;
mod reviews;
mod run_tokens;
mod runs;
mod saves;
mod search;
mod suites;
mod tx;
pub use analytics::{
    BoardRow, GroupPoint, Options, Pending, SeriesPoint, StatsRecord, Summary, Totals,
};
pub use api_tokens::ApiTokenRow;
pub use connections::{is_secret, Connection, NewConnection, REDACTED};
pub use control::Controlled;
pub(crate) use events::with_overhead;
pub use experiments::{NewExperiment, NewSetup};
pub use linked::Linked;
pub use obs_exports::{backoff, Due, ExportRow, RunRecord, Tried, MAX_ATTEMPTS};
pub use obs_metrics::{BranchLine, CallLine, Snapshot};
pub use observability::{
    target_context, NewTarget, OpenTarget, TargetFlags, TargetKind, TargetRow,
};
pub use open::{prepare_db, template_key};
pub use provider_keys::{KeyKind, KeyRow, NameTaken, NewKey, OpenKey};
pub use reviews::{ReviewDone, StoredReview};
pub(crate) use run_tokens::REVOKE;
pub(crate) use saves::{repoint_saves, saves_on};
pub use saves::{NewSave, SaveRow, SAVE_RETENTION};
pub use search::{FacetCount, SearchHit, SearchResults};
pub use suites::NewSuite;
use tx::Params;
pub use tx::Tx;

pub type RunId = String;
pub type BranchId = String;

/// The database shaked keeps its records in.
pub const DEFAULT_DB: &str = "shakedown";

/// `SHAKEDOWN_DB` (`get` reads a setting), else None (the default database). Under
/// `SHAKEDOWN_TEST_GUARD`, without `SHAKEDOWN_ALLOW_DEFAULT_DB`, a default database is refused,
/// before anything connects.
pub fn db_setting(get: impl Fn(&str) -> Option<String>) -> Result<Option<String>> {
    let named = get("SHAKEDOWN_DB");
    let guarded =
        get("SHAKEDOWN_TEST_GUARD").is_some() && get("SHAKEDOWN_ALLOW_DEFAULT_DB").is_none();
    let default = named.as_deref().is_none_or(|n| n == DEFAULT_DB);
    if guarded && default {
        anyhow::bail!(
            "SHAKEDOWN_TEST_GUARD is set: refusing the shared default database; set SHAKEDOWN_DB to a scratch database (or SHAKEDOWN_ALLOW_DEFAULT_DB=1)"
        );
    }
    Ok(named)
}

/// Branches joined to their runs: a branch of a run the caller cannot see (under a layer's
/// row-level security) is not found.
const BRANCHES_OF_RUNS: &str = "branches b JOIN runs r ON r.id = b.run_id";
/// True when the branch `$1` is one of a run the caller can see.
const VISIBLE_BRANCH: &str =
    "SELECT 1 FROM branches b JOIN runs r ON r.id = b.run_id WHERE b.id = $1";

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub struct Store {
    /// Request paths' connections.
    pool: Pool,
    /// Background jobs' connections, when they need their own (a hosting layer's); else `pool`.
    system: Option<Pool>,
    /// The deployment's hooks (`crate::layer`): [`LocalLayer`] unless [`Store::with_layer`].
    layer: Arc<dyn Layer>,
    tx: broadcast::Sender<Event>,
    // Per-branch locks make `max(seq)+1` appends atomic within this process.
    locks: Mutex<HashMap<String, std::sync::Arc<AsyncMutex<()>>>>,
}

impl Store {
    /// Opens (creating if needed) the metadata database [`DEFAULT_DB`].
    ///
    /// `SHAKEDOWN_DB` names another database. Under `SHAKEDOWN_TEST_GUARD` (set by tests that start
    /// shaked) the shared default databases are refused unless `SHAKEDOWN_ALLOW_DEFAULT_DB` is set,
    /// so a test never migrates someone's records ([`db_setting`]).
    pub async fn open(admin: &Admin) -> Result<Store> {
        let set = db_setting(|k| parts::settings::var(k).ok().filter(|v| !v.is_empty()))?;
        let db = set.unwrap_or_else(|| DEFAULT_DB.to_string());
        Store::open_named(admin, &db).await
    }

    /// Like `open`, with a custom database name (tests use a fresh one each): makes it when
    /// missing, brings its schema up to date ([`prepare_db`]), and marks
    /// what was running when shaked last stopped as interrupted.
    pub async fn open_named(admin: &Admin, name: &str) -> Result<Store> {
        if !admin.exists(name).await? {
            // Concurrent creators may race; tolerate losing.
            if let Err(e) = admin.create_db_from(name, "template1").await {
                if !admin.exists(name).await? {
                    return Err(e).context("creating metadata database");
                }
            }
        }
        prepare_db(admin, name).await?;
        let url = admin.db_url(name);
        let store = Store::with_pools(open::pool(&url)?, None);
        crate::jobs::interrupt_at_start(&store).await?;
        Ok(store)
    }

    /// A store over pools a hosting binary opened (and migrated), with its layer.
    pub fn from_pools(pool: Pool, system: Option<Pool>, layer: Arc<dyn Layer>) -> Store {
        Store::with_pools(pool, system).with_layer(layer)
    }

    /// A store with no database behind it, for a `shaked worker`: a worker records through its
    /// link and holds no database, so every query fails at once (a path that still reads the
    /// store fails loudly, never writes somewhere else).
    pub fn detached(layer: Arc<dyn Layer>) -> Result<Store> {
        let pool = open::pool("host=/nonexistent user=worker dbname=none connect_timeout=1")?;
        Ok(Store::from_pools(pool, None, layer))
    }

    /// This store, calling `layer` at its hooks.
    pub fn with_layer(mut self, layer: Arc<dyn Layer>) -> Store {
        self.layer = layer;
        self
    }

    /// The deployment's hooks.
    pub fn layer(&self) -> &Arc<dyn Layer> {
        &self.layer
    }

    fn with_pools(pool: Pool, system: Option<Pool>) -> Store {
        let (tx, _) = broadcast::channel(1024);
        Store {
            pool,
            system,
            layer: Arc::new(LocalLayer),
            tx,
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// The per-branch lock every append of the branch takes: what the control box decides for a
    /// linked branch (its hold, its end, its rebuilt link) is decided under it too.
    pub fn branch_lock(&self, branch: &str) -> std::sync::Arc<AsyncMutex<()>> {
        self.lock_for(branch)
    }

    fn lock_for(&self, branch: &str) -> std::sync::Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .unwrap()
            .entry(branch.to_string())
            .or_default()
            .clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// The record methods, for `caller`: the only way to them.
    pub fn at<'a>(&'a self, caller: &'a Caller) -> Scoped<'a> {
        Scoped {
            store: self,
            caller,
        }
    }
}

/// The store for one caller: every record method runs in a transaction for it.
#[derive(Clone, Copy)]
pub struct Scoped<'a> {
    store: &'a Store,
    caller: &'a Caller,
}

impl Scoped<'_> {
    /// Runs `f` in a transaction for the caller, committed when `f` succeeds.
    async fn in_tx<T>(&self, f: impl AsyncFnOnce(&Tx) -> Result<T>) -> Result<T> {
        let tx = self.store.tx(self.caller).await?;
        let out = f(&tx).await?;
        tx.commit().await?;
        Ok(out)
    }

    async fn query(&self, sql: &str, p: Params<'_>) -> Result<Vec<Row>> {
        self.in_tx(async |tx| tx.query(sql, p).await).await
    }

    async fn query_opt(&self, sql: &str, p: Params<'_>) -> Result<Option<Row>> {
        self.in_tx(async |tx| tx.query_opt(sql, p).await).await
    }

    async fn query_one(&self, sql: &str, p: Params<'_>) -> Result<Row> {
        self.in_tx(async |tx| tx.query_one(sql, p).await).await
    }

    async fn execute(&self, sql: &str, p: Params<'_>) -> Result<u64> {
        self.in_tx(async |tx| tx.execute(sql, p).await).await
    }
}

#[cfg(test)]
mod tests {
    use super::db_setting;

    fn with<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn the_test_guard_refuses_the_default_databases() {
        assert_eq!(
            db_setting(with(&[])).unwrap(),
            None,
            "a user's shaked uses the default"
        );
        assert_eq!(
            db_setting(with(&[("SHAKEDOWN_DB", "mine")]))
                .unwrap()
                .as_deref(),
            Some("mine")
        );
        let guard = ("SHAKEDOWN_TEST_GUARD", "1");
        assert!(db_setting(with(&[guard])).is_err());
        assert!(db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown")])).is_err());
        let scratch = db_setting(with(&[guard, ("SHAKEDOWN_DB", "shakedown_test_x")]));
        assert_eq!(scratch.unwrap().as_deref(), Some("shakedown_test_x"));
        let allowed = [guard, ("SHAKEDOWN_ALLOW_DEFAULT_DB", "1")];
        assert_eq!(db_setting(with(&allowed)).unwrap(), None);
    }
}
