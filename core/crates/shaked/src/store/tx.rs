//! A store transaction: `BEGIN` on a pooled connection. A transaction dropped without `commit`
//! closes its connection, so it never goes back to the pool mid-transaction.

use anyhow::{Context, Result};
use deadpool_postgres::Object;
use tokio_postgres::{types::ToSql, Row};

use super::Store;
use crate::{
    auth::{Caller, SystemScope},
    layer::TxFor,
};

/// An open transaction.
///
/// Only shaked's own jobs open a system transaction (every owner's rows, on the system pool): code
/// outside the crate, a layer among it, cannot.
///
/// ```compile_fail,E0624
/// async fn every_org(store: &shaked::store::Store, job: shaked::auth::SystemJob) {
///     let _ = store.system_tx(job).await;
/// }
/// ```
pub struct Tx {
    conn: Option<Object>,
    committed: bool,
}

type Params<'a> = &'a [&'a (dyn ToSql + Sync)];

impl Tx {
    fn conn(&self) -> &Object {
        self.conn
            .as_ref()
            .expect("a Tx holds its connection until it commits or drops")
    }

    pub async fn query(&self, sql: &str, p: Params<'_>) -> Result<Vec<Row>> {
        Ok(self.conn().query(sql, p).await?)
    }

    pub async fn query_opt(&self, sql: &str, p: Params<'_>) -> Result<Option<Row>> {
        Ok(self.conn().query_opt(sql, p).await?)
    }

    pub async fn query_one(&self, sql: &str, p: Params<'_>) -> Result<Row> {
        Ok(self.conn().query_one(sql, p).await?)
    }

    pub async fn execute(&self, sql: &str, p: Params<'_>) -> Result<u64> {
        Ok(self.conn().execute(sql, p).await?)
    }

    /// Runs statements without parameters (simple-query protocol).
    pub async fn batch(&self, sql: &str) -> Result<()> {
        Ok(self.conn().batch_execute(sql).await?)
    }

    pub async fn commit(mut self) -> Result<()> {
        self.conn()
            .batch_execute("COMMIT")
            .await
            .context("committing")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if let (false, Some(conn)) = (self.committed, self.conn.take()) {
            // Detached from the pool and dropped: the connection closes, its transaction with it.
            drop(Object::take(conn));
        }
    }
}

impl Store {
    /// A transaction for `c`'s request: `BEGIN`, then the layer's `begin`.
    pub async fn tx(&self, c: &Caller) -> Result<Tx> {
        self.begin(&self.pool, TxFor::Caller(c)).await
    }

    /// A transaction of a system job: on the system pool when there is one. Only shaked's own
    /// jobs open one (a [`SystemScope`] is built only in this crate).
    pub(crate) async fn system_tx(&self, scope: SystemScope) -> Result<Tx> {
        let pool = self.system.as_ref().unwrap_or(&self.pool);
        self.begin(pool, TxFor::System(scope.job())).await
    }

    async fn begin(&self, pool: &deadpool_postgres::Pool, on: TxFor<'_>) -> Result<Tx> {
        let tx = Tx {
            conn: Some(pool.get().await?),
            committed: false,
        };
        tx.batch("BEGIN").await?;
        self.layer.begin(&tx, on).await?;
        Ok(tx)
    }

    /// The request pool's size and idle connections.
    pub fn pool_status(&self) -> deadpool_postgres::Status {
        self.pool.status()
    }
}
