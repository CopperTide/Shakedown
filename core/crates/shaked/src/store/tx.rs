//! A store transaction under a scope: `BEGIN`, the role, and the `app.*` settings every
//! row-level security policy reads. A transaction dropped without `commit` closes its connection,
//! so it never goes back to the pool mid-transaction.

use anyhow::{Context, Result};
use deadpool_postgres::Object;
use tokio_postgres::{types::ToSql, Row};

use super::Store;
use crate::auth::{Scope, SystemScope};

/// The four settings, transaction-local, so they reset at commit.
const SET_SCOPE: &str =
    "SELECT set_config('app.org_id', $1, true), set_config('app.user_id', $2, true),
        set_config('app.team_ids', $3, true), set_config('app.org_role', $4, true)";

/// An open transaction.
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
    /// A transaction as `s`: under `shaked_app` (set as the role when this login is not in it
    /// already) with `s`'s org, user, teams and role.
    pub async fn tx(&self, s: &Scope) -> Result<Tx> {
        let tx = Tx {
            conn: Some(self.app.get().await?),
            committed: false,
        };
        tx.batch(if self.set_role {
            "BEGIN; SET LOCAL ROLE shaked_app"
        } else {
            "BEGIN"
        })
        .await?;
        let [org, user, teams, role] = s.settings();
        tx.execute(SET_SCOPE, &[&org, &user, &teams, &role]).await?;
        Ok(tx)
    }

    /// A transaction of a system job, on the system pool as `shaked_system` (every org's rows).
    pub async fn system_tx(&self, _s: &SystemScope) -> Result<Tx> {
        let tx = Tx {
            conn: Some(self.system.get().await?),
            committed: false,
        };
        tx.batch("BEGIN; SET LOCAL ROLE shaked_system").await?;
        Ok(tx)
    }

    /// The request pool's size and idle connections.
    pub fn pool_status(&self) -> deadpool_postgres::Status {
        self.app.status()
    }
}
