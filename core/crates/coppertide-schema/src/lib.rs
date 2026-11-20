//! CopperTide's one database schema, used by the Cloud gateway, hosted shaked and local shaked.
//!
//! The SQL lives in `migrations/<stream>/NNNN_name.sql`, one folder per owning service: `identity`
//! (schemas `authz` and `identity`), `cloud` and `shakedown`. Each binary embeds every file and
//! applies the streams it owns with [`migrate`], which records each version with its sha256 in
//! `schema_meta.migrations` and refuses a database whose applied file differs. Migrations are
//! append-only: a destructive statement needs `-- contract: unused since <sha>` (`tests/files.rs`).
//!
//! The migrator talks to Postgres through the two-method [`Conn`] trait, so any driver can call it;
//! the `tokio-postgres` feature (on by default) implements it for `tokio_postgres::Client`.

mod migrator;
#[cfg(feature = "tokio-postgres")]
mod pg;
mod stream;

pub use migrator::{check, migrate, migrate_set, wait_ready, Applied, Conn, Error, Plan, Report};
pub use stream::{migrations, parse, Migration, Stream};

/// Section 0 of the schema: the four NOLOGIN roles and the schemas. Run once per cluster and
/// database as a superuser before migrating (a local shaked on a superuser login runs it itself).
pub const BOOTSTRAP_SQL: &str = include_str!("../bootstrap.sql");
