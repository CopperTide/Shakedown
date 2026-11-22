//! The layer a `shaked worker` runs on: personal use's ([`LocalLayer`]) in everything but its
//! secrets. A sandbox holds no secret but its worker token, so the worker never reads or makes a
//! vault key, and never serves an agent with model keys from its environment.

use std::sync::Arc;

use axum::{http::request::Parts, Router};

use crate::{
    app::AppState,
    auth::{Action, Caller, Decision, Target},
    layer::*,
    store::Tx,
};

/// What a worker's shaked is: this sandbox only, packages run here, no keys of the environment.
pub const WORKER_POLICY: Policy = Policy {
    environment_keys: false,
    ..Policy::LOCAL
};

/// Where every hook but the secrets goes.
static LOCAL: LocalLayer = LocalLayer;

/// [`LocalLayer`] without secrets (see the module).
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkerLayer;

impl Layer for WorkerLayer {
    fn policy(&self) -> &Policy {
        &WORKER_POLICY
    }

    fn keeps_secrets(&self) -> bool {
        false
    }

    fn authenticate<'a>(
        &'a self,
        req: &'a Parts,
        found: Option<Caller>,
    ) -> BoxFuture<'a, Result<Caller, Refusal>> {
        LOCAL.authenticate(req, found)
    }

    fn begin<'a>(&'a self, tx: &'a Tx, on: TxFor<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        LOCAL.begin(tx, on)
    }

    fn authorize<'a>(&'a self, c: &'a Caller, a: Action, t: Target<'a>) -> BoxFuture<'a, Decision> {
        LOCAL.authorize(c, a, t)
    }

    fn admit<'a>(&'a self, c: &'a Caller, op: Costly<'a>) -> BoxFuture<'a, Admission> {
        LOCAL.admit(c, op)
    }

    fn meter<'a>(&'a self, c: &'a Caller, u: Usage<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        LOCAL.meter(c, u)
    }

    fn fallback_provider<'a>(
        &'a self,
        c: &'a Caller,
        want: ModelWant<'a>,
    ) -> BoxFuture<'a, Option<Fallback>> {
        LOCAL.fallback_provider(c, want)
    }

    fn audit<'a>(
        &'a self,
        tx: &'a Tx,
        c: &'a Caller,
        e: AuditEvent<'a>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        LOCAL.audit(tx, c, e)
    }

    fn routes(&self) -> Router<Arc<AppState>> {
        Router::new()
    }

    fn me<'a>(&'a self, c: &'a Caller) -> BoxFuture<'a, MeExtra> {
        LOCAL.me(c)
    }

    fn object_prefix(&self, c: &Caller) -> String {
        LOCAL.object_prefix(c)
    }

    fn own_keys<'a>(
        &'a self,
        c: &'a Caller,
        keys: &'a [uuid::Uuid],
    ) -> BoxFuture<'a, anyhow::Result<Vec<uuid::Uuid>>> {
        LOCAL.own_keys(c, keys)
    }

    fn principal(&self, c: &Caller) -> String {
        LOCAL.principal(c)
    }
}
