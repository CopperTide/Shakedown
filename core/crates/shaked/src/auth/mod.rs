//! Who is calling and what they may do: the principals the auth layer builds, and one
//! `can(principal, action, target)` (the accounts spec §3). Row-level security in Postgres is the
//! second wall; levels finer than "may see" live here.

pub mod can;
pub mod principal;

pub use can::{
    can, level, required, Action, Grant, Grantee, OrgFlags, Resource, ResourceKind, Target,
};
pub use principal::{Level, OrgRole, Principal, RunTokenKind, SeatRole, TokenScope};
