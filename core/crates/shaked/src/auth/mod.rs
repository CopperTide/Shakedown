//! Who is calling and what they may do: the principals the auth layer builds, and one
//! `can(principal, action, target)` (the accounts spec §3). Row-level security in Postgres is the
//! second wall; levels finer than "may see" live here.

pub mod audit;
pub mod can;
pub mod extract;
pub mod identity;
pub mod middleware;
pub mod principal;
pub mod run_tokens;
pub mod scope;
pub mod signing;
pub mod system;
pub mod tokens;

pub use can::{
    can, level, required, Action, Grant, Grantee, OrgFlags, Resource, ResourceKind, Target,
};
pub use extract::Caller;
pub use principal::{Level, OrgRole, Principal, RunTokenKind, SeatRole, TokenScope};
#[cfg(any(test, feature = "testkit"))]
pub use scope::test_scope;
pub use scope::{local_owner, Scope};
pub use system::{SystemJob, SystemScope};
