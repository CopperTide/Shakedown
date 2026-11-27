//! Runs a checked query as the caller: in the caller's own transaction (a hosting layer's `begin`
//! sets up whatever it scopes a caller to there), made READ ONLY, with a statement timeout,
//! the search_path pinned to the explore schema, and the layer's explore role when the database has
//! it. Rows come back as JSON arrays the database builds, capped by count and by size.

use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use tokio_postgres::{error::SqlState, types::Type, Row};

use super::{guard, SCHEMA};
use crate::{
    api::ApiError,
    auth::Caller,
    store::{Store, Tx},
};

/// The most rows a query returns; more are cut and reported.
pub const ROW_CAP: usize = 5_000;
/// The most bytes of rows (as JSON) a query returns.
pub const BYTE_CAP: usize = 8 * 1024 * 1024;
/// How long a query may run.
pub const TIMEOUT_SECS: u64 = 5;
/// The most columns a result may have.
pub const MAX_COLUMNS: usize = 100;

/// What a column holds, for choosing a chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Number,
    Time,
    Bool,
    Json,
    Text,
}

/// One result column: its name, its Postgres type and its kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub pg_type: String,
    pub kind: Kind,
}

/// Why rows were cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cut {
    Rows,
    Bytes,
}

/// A query's result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Answer {
    /// The query as it ran: the parsed SELECT printed back.
    pub sql: String,
    pub columns: Vec<Column>,
    /// Each row as an array, in column order.
    pub rows: Vec<Value>,
    /// Set when rows were cut at [`ROW_CAP`] or [`BYTE_CAP`].
    pub truncated: Option<Cut>,
    pub ms: u64,
}

/// Checks `sql` and runs it for `caller`: 400 for a query the guard refuses or the database
/// rejects (a timeout among them).
pub async fn run(store: &Store, caller: &Caller, sql: &str) -> Result<Answer, ApiError> {
    let checked = guard::check(sql).map_err(ApiError::bad_request)?;
    run_checked(store, caller, checked).await
}

/// Runs a query [`guard::check`] passed. The walls behind the guard (read-only, the timeout, the
/// pinned search_path, the role) hold for any query given here; the tests give it unchecked ones
/// to prove that.
pub async fn run_checked(
    store: &Store,
    caller: &Caller,
    checked: guard::Checked,
) -> Result<Answer, ApiError> {
    let started = Instant::now();
    let tx = store.tx(caller).await?;
    tx.batch(&setup(store.layer().explore_role())).await?;
    let columns = columns(&tx, &checked.sql).await?;
    let rows = (tx.query(&as_json(&checked.sql, columns.len()), &[]).await).map_err(refused)?;
    tx.commit().await?;
    let (rows, truncated) = capped(&rows)?;
    Ok(Answer {
        sql: checked.sql,
        columns,
        rows,
        truncated,
        ms: started.elapsed().as_millis() as u64,
    })
}

/// What the transaction is made before the query: read-only, timed, on the explore schema, then
/// as `role` when the database has it and this session may take it (else as it is).
fn setup(role: Option<&str>) -> String {
    let mut sql = format!(
        "SET TRANSACTION READ ONLY; SET LOCAL statement_timeout = '{TIMEOUT_SECS}s'; \
         SET LOCAL lock_timeout = '1s'; SET LOCAL search_path = {SCHEMA}, pg_catalog, pg_temp;"
    );
    if let Some(r) = role.filter(|r| r.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')) {
        sql.push_str(&format!(
            " DO $$ BEGIN SET LOCAL ROLE {r}; \
             EXCEPTION WHEN invalid_parameter_value OR undefined_object OR insufficient_privilege \
             THEN NULL; END $$;"
        ));
    }
    sql
}

/// The result columns of `sql`, from preparing it (nothing runs).
async fn columns(tx: &Tx, sql: &str) -> Result<Vec<Column>, ApiError> {
    let stmt = (tx.prepare(&format!("SELECT * FROM ({sql}) AS q")).await).map_err(refused)?;
    let cols = stmt.columns();
    if cols.is_empty() || cols.len() > MAX_COLUMNS {
        let why = format!("a query returns 1 to {MAX_COLUMNS} columns");
        return Err(ApiError::bad_request(why));
    }
    Ok(cols
        .iter()
        .map(|c| Column {
            name: c.name().to_string(),
            pg_type: c.type_().name().to_string(),
            kind: kind_of(c.type_()),
        })
        .collect())
}

/// `sql` with each row as a JSON array (`n` columns, renamed so duplicate names are fine), at
/// most one row past the cap so a cut shows.
fn as_json(sql: &str, n: usize) -> String {
    let names: Vec<String> = (1..=n).map(|i| format!("c{i}")).collect();
    let names = names.join(", ");
    let limit = ROW_CAP + 1;
    format!("SELECT json_build_array({names})::text FROM ({sql}) AS q({names}) LIMIT {limit}")
}

fn kind_of(t: &Type) -> Kind {
    match t.name() {
        "bool" => Kind::Bool,
        "int2" | "int4" | "int8" | "float4" | "float8" | "numeric" | "oid" => Kind::Number,
        "timestamptz" | "timestamp" | "date" => Kind::Time,
        "json" | "jsonb" => Kind::Json,
        n if n.starts_with('_') => Kind::Json,
        _ => Kind::Text,
    }
}

/// The rows as JSON, up to the caps, and why they were cut.
fn capped(rows: &[Row]) -> Result<(Vec<Value>, Option<Cut>), ApiError> {
    let (mut out, mut bytes) = (Vec::new(), 0usize);
    for row in rows {
        if out.len() == ROW_CAP {
            return Ok((out, Some(Cut::Rows)));
        }
        let text: &str = row.get(0);
        bytes += text.len();
        if bytes > BYTE_CAP {
            return Ok((out, Some(Cut::Bytes)));
        }
        let value = serde_json::from_str(text).map_err(|e| ApiError::internal(e.into()))?;
        out.push(value);
    }
    Ok((out, None))
}

fn refused(e: anyhow::Error) -> ApiError {
    refused_after(e, TIMEOUT_SECS)
}

/// A database error as the caller sees it: a timeout after `secs`, or the database's own message,
/// as a 400.
pub fn refused_after(e: anyhow::Error, secs: u64) -> ApiError {
    let pg = (e.chain()).find_map(|c| c.downcast_ref::<tokio_postgres::Error>());
    if pg.and_then(|p| p.code()) == Some(&SqlState::QUERY_CANCELED) {
        return ApiError::bad_request(format!(
            "the query ran longer than {secs} s: narrow it (a shorter time range, fewer groups or a LIMIT)"
        ));
    }
    match pg.and_then(|p| p.as_db_error()) {
        Some(db) => {
            ApiError::bad_request(format!("the database refused the query: {}", db.message()))
        }
        None => ApiError::from(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_is_read_only_timed_pinned_and_takes_the_role_if_it_can() {
        let s = setup(None);
        assert!(s.starts_with("SET TRANSACTION READ ONLY;"));
        assert!(s.contains("statement_timeout = '5s'"));
        assert!(s.contains("search_path = explore, pg_catalog, pg_temp"));
        assert!(!s.contains("ROLE"));
        let s = setup(Some("shakedown_explore"));
        assert!(s.contains("SET LOCAL ROLE shakedown_explore;"), "{s}");
        assert!(!setup(Some("x; DROP")).contains("ROLE"));
    }

    #[test]
    fn rows_are_built_as_json_arrays_one_past_the_cap() {
        assert_eq!(
            as_json("SELECT a, a FROM runs", 2),
            "SELECT json_build_array(c1, c2)::text FROM (SELECT a, a FROM runs) AS q(c1, c2) LIMIT 5001"
        );
    }
}
