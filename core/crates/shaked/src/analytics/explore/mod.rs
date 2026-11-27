//! Explore: analytics you ask for. The builder takes measures × dimensions with filters
//! ([`spec`], [`catalog`]) and makes their SQL from fixed fragments ([`query`]); the SQL tab runs
//! one read-only SELECT over the views of schema [`SCHEMA`] ([`guard`] checks it, [`sql`] runs it).
//! Both run in the caller's transaction, so a caller sees only the runs it may; neither
//! reads a model call's body (each call's usage is kept in `call_usage` by the analytics job).
//!
//! ```text
//!  explore.query ─► Spec ─► Plan ─► query::query_sql ─► caller tx ─► runs, events, call_usage
//!  explore.drill ─────────────────► query::drill_sql ─┘
//!  explore.sql ──► guard::check ─► sql::run ─► caller tx: READ ONLY, 5 s, search_path explore,
//!                  one SELECT,                 SET ROLE shakedown_explore ─► explore.* views
//!                  views + allowed functions
//! ```

pub mod catalog;
pub mod docs;
pub mod guard;
pub mod query;
pub mod spec;
pub mod sql;

pub use spec::{Filters, Plan, Spec};

/// The schema of the views SQL may query (migration `shakedown 9`).
pub const SCHEMA: &str = "explore";

/// The views of [`SCHEMA`]: the only relations a query may name.
pub const VIEWS: &[&str] = &["runs", "steps", "model_calls", "tool_calls", "agents"];
