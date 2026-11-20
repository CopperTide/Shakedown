//! Code connections in the metadata database: a saved command that starts a team's driver, which
//! shaked launches for each run that uses it (`crate::driver`).

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tokio_postgres::Row;

use super::{new_id, Store};

const COLS: &str = "id, name, kind, command, workdir, env, created_at";

/// What a value of an env var whose name says it is a secret reads as in answers.
pub const REDACTED: &str = "••••";

/// A Code connection: the command (its program and arguments), the folder it runs in and the
/// environment it gets, besides the run's `ARENA_*` ids.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    /// Only `code` for now.
    pub kind: String,
    pub command: Vec<String>,
    pub workdir: Option<String>,
    pub env: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
}

/// True for an env var's name that says it holds a secret (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`).
pub fn is_secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD"]
        .iter()
        .any(|w| n.contains(w))
}

impl Connection {
    /// The connection as answers show it: secret env values replaced by [`REDACTED`].
    pub fn redacted(&self) -> Connection {
        let mut c = self.clone();
        for (k, v) in c.env.iter_mut() {
            if is_secret(k) {
                *v = REDACTED.to_string();
            }
        }
        c
    }
}

/// A new connection.
pub struct NewConnection<'a> {
    pub name: &'a str,
    pub command: &'a [String],
    pub workdir: Option<&'a str>,
    pub env: &'a BTreeMap<String, String>,
}

fn connection_from(r: &Row) -> Result<Connection> {
    let command: Value = r.get("command");
    let env: Value = r.get("env");
    Ok(Connection {
        id: r.get("id"),
        name: r.get("name"),
        kind: r.get("kind"),
        command: serde_json::from_value(command)?,
        workdir: r.get("workdir"),
        env: serde_json::from_value(env)?,
        created_at: r.get("created_at"),
    })
}

impl Store {
    pub async fn create_connection(&self, new: NewConnection<'_>) -> Result<Connection> {
        let id = new_id();
        let command = serde_json::to_value(new.command)?;
        let env = serde_json::to_value(new.env)?;
        let sql = format!(
            "INSERT INTO connections (id, name, command, workdir, env) VALUES ($1, $2, $3, $4, $5)
             RETURNING {COLS}"
        );
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 5] =
            [&id, &new.name, &command, &new.workdir, &env];
        let row = self.db().await?.query_one(&sql, &params).await?;
        connection_from(&row)
    }

    /// Every connection, newest first.
    pub async fn connections(&self) -> Result<Vec<Connection>> {
        let sql = format!("SELECT {COLS} FROM connections ORDER BY created_at DESC, id");
        let rows = self.db().await?.query(&sql, &[]).await?;
        rows.iter().map(connection_from).collect()
    }

    pub async fn connection(&self, id: &str) -> Result<Option<Connection>> {
        let sql = format!("SELECT {COLS} FROM connections WHERE id = $1");
        let row = self.db().await?.query_opt(&sql, &[&id]).await?;
        row.as_ref().map(connection_from).transpose()
    }

    /// Deletes the connection: false when there was none. Runs that used it keep its id.
    pub async fn delete_connection(&self, id: &str) -> Result<bool> {
        let n = self
            .db()
            .await?
            .execute("DELETE FROM connections WHERE id = $1", &[&id])
            .await?;
        Ok(n > 0)
    }
}
