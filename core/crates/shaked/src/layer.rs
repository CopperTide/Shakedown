//! The layer: the points where the engine asks its deployment (who calls, every transaction,
//! every route, costly steps, metering, fallback models, audit, extra routes, `/api/me`, object
//! keys). The open-source binary passes [`LocalLayer`], every hook a personal-use default; a hosting
//! layer (CopperTide Cloud's) implements the same trait.

use std::path::Path;
use std::sync::Arc;

use axum::{http::request::Parts, Router};
use futures_util::FutureExt;

use crate::{
    app::AppState,
    auth::{Action, Caller, Decision, Target},
    store::Tx,
};

mod types;
pub use crate::project_state::{Change, Files, NoProject, ProjectDb, ProjectHome};
pub use types::*;

/// A deployment's hooks. Async hooks return boxed futures.
pub trait Layer: Send + Sync + 'static {
    /// Static facts about this deployment.
    fn policy(&self) -> &Policy;
    /// Who calls; `found` is what the engine resolved from a Bearer (`sd_`, `sdr_`, `sdw_`), if any.
    fn authenticate<'a>(
        &'a self,
        req: &'a Parts,
        found: Option<Caller>,
    ) -> BoxFuture<'a, Result<Caller, Refusal>>;
    /// Runs right after `BEGIN` in every transaction the engine opens.
    fn begin<'a>(&'a self, tx: &'a Tx, on: TxFor<'a>) -> BoxFuture<'a, anyhow::Result<()>>;
    /// Route-level authorization, after the engine's own checks passed.
    fn authorize<'a>(&'a self, c: &'a Caller, a: Action, t: Target<'a>) -> BoxFuture<'a, Decision>;
    /// Quota check before a costly operation; `Refuse` stops it before anything starts.
    fn admit<'a>(&'a self, c: &'a Caller, op: Costly<'a>) -> BoxFuture<'a, Admission>;
    /// Usage after the fact; idempotent on `Usage::key`.
    fn meter<'a>(&'a self, c: &'a Caller, u: Usage<'a>) -> BoxFuture<'a, anyhow::Result<()>>;
    /// A model provider when the caller has no key of its own for what an agent asks.
    fn fallback_provider<'a>(
        &'a self,
        c: &'a Caller,
        want: ModelWant<'a>,
    ) -> BoxFuture<'a, Option<Fallback>>;
    /// An auditable change, inside the transaction that made it.
    fn audit<'a>(
        &'a self,
        tx: &'a Tx,
        c: &'a Caller,
        e: AuditEvent<'a>,
    ) -> BoxFuture<'a, anyhow::Result<()>>;
    /// Routes only this layer serves, merged under `/api` behind `authenticate`.
    fn routes(&self) -> Router<Arc<AppState>>;
    /// The layer's fields of `GET /api/me`.
    fn me<'a>(&'a self, c: &'a Caller) -> BoxFuture<'a, MeExtra>;
    /// The first segment of every object key the caller's records name.
    fn object_prefix(&self, c: &Caller) -> String;
    /// Which of the model keys `keys` (all of them visible to `c`) are `c`'s own: those apply to
    /// `c`'s agents by kind. The rest are shared with `c`; an agent uses one only by naming it
    /// (provider `key:<name>`), and never one that names its own server.
    fn own_keys<'a>(
        &'a self,
        c: &'a Caller,
        keys: &'a [uuid::Uuid],
    ) -> BoxFuture<'a, anyhow::Result<Vec<uuid::Uuid>>>;
    /// Who `c` is within its `object_prefix` scope: the person, an API token's holder, a user.
    /// Header values a caller types into a team are kept for the scope and this principal, so
    /// a colleague in the same scope cannot cite them. Required, with no default, so that a layer
    /// serving several users cannot forget to tell them apart.
    fn principal(&self, c: &Caller) -> String;
    /// Where `c`'s project lives; `configured` is `SHAKEDOWN_PROJECT`. Personal use has the one
    /// folder; a hosting layer may give callers folders of their own, or keep their projects in
    /// its database ([`ProjectHome::Db`]).
    fn project_home(&self, _c: &Caller, configured: Option<&Path>) -> Option<ProjectHome> {
        configured.map(|dir| ProjectHome::Folder(dir.to_path_buf()))
    }
    /// False where no secret may be decrypted, such as a sandbox's worker: the vault is refused
    /// and its key never read or made.
    fn keeps_secrets(&self) -> bool {
        true
    }
    /// The database role Explore's SQL runs as, after `begin` (`SET LOCAL ROLE`), when the
    /// database has it and the session may take it: one that reads the explore views and nothing
    /// else (migration `shakedown 9` makes `shakedown_explore`). None: the caller's own role, the
    /// guard, the read-only transaction and the pinned search_path being the walls.
    fn explore_role(&self) -> Option<&'static str> {
        Some(EXPLORE_ROLE)
    }
}

/// The role migration `shakedown 9` makes for Explore's SQL.
pub const EXPLORE_ROLE: &str = "shakedown_explore";

/// A deployment on personal use's hooks (everything allowed, nothing metered), told apart only by
/// its policy and whether it keeps secrets: [`LocalLayer`] and a sandbox's [`WorkerLayer`].
pub trait PersonalUse: Send + Sync + 'static {
    const POLICY: &'static Policy;
    /// See [`Layer::keeps_secrets`].
    const KEEPS_SECRETS: bool;
}

/// Personal use: this machine only, everything allowed, nothing metered.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalLayer;

impl PersonalUse for LocalLayer {
    const POLICY: &'static Policy = &Policy::LOCAL;
    const KEEPS_SECRETS: bool = true;
}

/// The layer a `shaked worker` runs on: personal use's in everything but its secrets. A sandbox
/// holds no secret but its worker token, so the worker never reads or makes a vault key, and never
/// serves an agent with model keys from its environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkerLayer;

impl PersonalUse for WorkerLayer {
    const POLICY: &'static Policy = &WORKER_POLICY;
    const KEEPS_SECRETS: bool = false;
}

impl Policy {
    /// Personal use: this machine only, private model servers fine, packages run here.
    pub const LOCAL: Policy = Policy {
        serves_others: false,
        private_model_urls: true,
        runs_packages_here: true,
        environment_keys: true,
    };
}

/// What a worker's shaked is: this sandbox only, packages run here, no keys of the environment.
pub const WORKER_POLICY: Policy = Policy {
    environment_keys: false,
    ..Policy::LOCAL
};

impl<L: PersonalUse> Layer for L {
    fn policy(&self) -> &Policy {
        L::POLICY
    }

    fn keeps_secrets(&self) -> bool {
        L::KEEPS_SECRETS
    }

    /// The person, or an API token's holder: each token is told apart from the person and from
    /// other tokens.
    fn principal(&self, c: &Caller) -> String {
        match &c.kind {
            crate::auth::CallerKind::ApiToken { id, .. } => id.to_string(),
            crate::auth::CallerKind::RunToken { branch, .. } => format!("run:{branch}"),
            crate::auth::CallerKind::Person => "person".to_string(),
        }
    }

    /// The token the engine found, else the person at this machine (the Host guard already kept
    /// `/api` to loopback).
    fn authenticate<'a>(
        &'a self,
        _req: &'a Parts,
        found: Option<Caller>,
    ) -> BoxFuture<'a, Result<Caller, Refusal>> {
        async move { Ok(found.unwrap_or_else(Caller::person)) }.boxed()
    }

    fn begin<'a>(&'a self, _tx: &'a Tx, _on: TxFor<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        async { Ok(()) }.boxed()
    }

    fn authorize<'a>(
        &'a self,
        _c: &'a Caller,
        _a: Action,
        _t: Target<'a>,
    ) -> BoxFuture<'a, Decision> {
        async { Decision::Allow }.boxed()
    }

    fn admit<'a>(&'a self, _c: &'a Caller, _op: Costly<'a>) -> BoxFuture<'a, Admission> {
        async { Admission::Go }.boxed()
    }

    fn meter<'a>(&'a self, _c: &'a Caller, _u: Usage<'a>) -> BoxFuture<'a, anyhow::Result<()>> {
        async { Ok(()) }.boxed()
    }

    /// None: the keys in the environment are all a local shaked has.
    fn fallback_provider<'a>(
        &'a self,
        _c: &'a Caller,
        _want: ModelWant<'a>,
    ) -> BoxFuture<'a, Option<Fallback>> {
        async { None }.boxed()
    }

    fn audit<'a>(
        &'a self,
        _tx: &'a Tx,
        _c: &'a Caller,
        _e: AuditEvent<'a>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        async { Ok(()) }.boxed()
    }

    fn routes(&self) -> Router<Arc<AppState>> {
        Router::new()
    }

    fn me<'a>(&'a self, _c: &'a Caller) -> BoxFuture<'a, MeExtra> {
        async {
            MeExtra {
                mode: "local",
                features: Vec::new(),
                account: None,
            }
        }
        .boxed()
    }

    fn object_prefix(&self, _c: &Caller) -> String {
        "local".to_string()
    }

    /// All of them: one person has every key here.
    fn own_keys<'a>(
        &'a self,
        _c: &'a Caller,
        keys: &'a [uuid::Uuid],
    ) -> BoxFuture<'a, anyhow::Result<Vec<uuid::Uuid>>> {
        async move { Ok(keys.to_vec()) }.boxed()
    }
}

/// What header values `c` types into a team are kept for: its scope and its principal.
pub fn secret_scope(layer: &dyn Layer, c: &Caller) -> String {
    format!("{}\u{1f}{}", layer.object_prefix(c), layer.principal(c))
}

/// Asks `store`'s layer about `op`: `Ok(Some(warning))` to go with a warning, `Err` to stop.
pub async fn admit(
    store: &crate::store::Store,
    c: &Caller,
    op: Costly<'_>,
) -> Result<Option<String>, Refusal> {
    match store.layer().admit(c, op).await {
        Admission::Go => Ok(None),
        Admission::Warn(w) => Ok(Some(w)),
        Admission::Refuse { status, message } => Err(Refusal { status, message }),
    }
}

/// Meters `u` through `store`'s layer; a failure is logged (the work it counts already happened).
pub async fn meter(store: &crate::store::Store, c: &Caller, u: Usage<'_>) {
    let key = u.key.clone();
    if let Err(e) = store.layer().meter(c, u).await {
        eprintln!("shaked: metering {key}: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_person_and_each_token_keep_header_values_apart_under_one_prefix() {
        let person = Caller::person();
        let a = Caller::api_token(uuid::Uuid::new_v4(), vec![]);
        let b = Caller::api_token(uuid::Uuid::new_v4(), vec![]);
        let scopes: Vec<String> = [&person, &a, &b]
            .iter()
            .map(|c| secret_scope(&LocalLayer, c))
            .collect();
        assert!(scopes.iter().all(|s| s.starts_with("local")));
        assert!(scopes[0] != scopes[1] && scopes[1] != scopes[2] && scopes[0] != scopes[2]);
        assert_eq!(scopes[0], secret_scope(&LocalLayer, &Caller::person()));
    }
}
