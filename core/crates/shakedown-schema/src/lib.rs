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
mod retired;
mod stream;

pub use migrator::{check, migrate, wait_ready, Applied, Conn, Error, Plan, Report};
pub use retired::{retired_in, RETIRED};
pub use stream::{base, load, parse, Files, Migration, Stream, BASE_FILES, SHAKEDOWN};
