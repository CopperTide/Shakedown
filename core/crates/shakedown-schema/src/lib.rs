//! Shakedown's database schema and a generic migrator.
//!
//! The SQL lives in `migrations/<stream>/NNNN_name.sql`. This crate owns stream [`SHAKEDOWN`]
//! ([`base`]); a stream is only a name, so another repo embeds its own [`Files`], [`load`]s them
//! with the base and passes the set to [`migrate`], which applies the streams a plan names, records
//! each version with its sha256 in `schema_meta.migrations` and refuses a database whose applied
//! file differs. Migrations are
//! append-only: a destructive statement needs `-- contract: unused since <sha>` (`tests/files.rs`).
//!
//! The migrator talks to Postgres through the two-method [`Conn`] trait, so any driver can call it;
//! the `tokio-postgres` feature (on by default) implements it for `tokio_postgres::Client`.

mod migrator;
#[cfg(feature = "tokio-postgres")]
mod pg;
mod stream;

pub use migrator::{check, migrate, wait_ready, Applied, Conn, Error, Plan, Report};
pub use stream::{
    base, load, parse, retiring_identity, Files, Migration, Stream, BASE_FILES, SHAKEDOWN,
};

/// Section 0 of the schema: the four NOLOGIN roles and the schemas. Run once per cluster and
/// database as a superuser before migrating (a local shaked on a superuser login runs it itself).
pub const BOOTSTRAP_SQL: &str = include_str!("../bootstrap.sql");
