//! A branch a sandbox's worker runs, as the control box's gateway sees it: the worker token
//! that opens it, its agents as the store has them, and the control box's own hold on it. The
//! sandbox is not trusted: the run's spend limit, a pause and a stop are enforced here, before
//! any call reaches a provider, whatever the worker does, whether its worker is connected or not,
//! and after a control-box restart (all of it is kept in the store). The worker's own checks
//! come first, for a well-behaved worker, and never instead.

use std::{sync::Arc, time::Duration};

use axum::{
    http::{HeaderValue, StatusCode},
    response::Response,
};
use serde_json::json;
use tokio::sync::OwnedSemaphorePermit;

use super::{error_response, target::Kind, Refused};
use crate::{
    app::AppState,
    auth::{run_tokens::worker_caller, Action, Caller},
    model::*,
    providers::Provider,
    worker::{holds, links::Link},
};

/// What a call with a worker token that doesn't open its route is told.
pub const WORKER_TOKEN_WANTED: &str =
    "this route wants the agent's token, or the worker token of its branch";
/// What a linked branch's call is told while the control box has no link to its worker (it
/// restarted and the worker has not rejoined yet): 503, to be tried again.
pub const NOT_LINKED: &str = "the branch's worker is not linked to the control box now; try again";
/// What a linked branch's call is told when the branch's rules are not on record (a branch from
/// before they were kept): 503, never unlimited.
pub const NO_LIMITS: &str = "this branch has no limits on record";
/// What a call is told when the branch's pool of calls in flight (4 times its agents) is in use,
/// and none ended in [`INFLIGHT_WAIT`] (429).
pub const TOO_MANY: &str =
    "this branch has as many model calls in flight as it may (4 times its agents)";
/// How long a call waits for one of its branch's calls in flight to end.
const INFLIGHT_WAIT: Duration = Duration::from_secs(10);
/// What a call of a paused linked branch is told (409).
pub const PAUSED: &str = "the run is paused";
/// The header of a 409 that says the branch is paused (`paused`): its worker waits for the
/// resume and sends the call again.
pub const HELD_HEADER: &str = "x-shakedown-held";

/// A linked branch's call: for `caller`, on `branch`, held by `link`, one of its calls in flight
/// for as long as it lives.
pub(super) struct Linked {
    pub caller: Arc<Caller>,
    pub branch: String,
    pub link: Arc<Link>,
    _inflight: Option<OwnedSemaphorePermit>,
}

fn internal(doing: &str, e: anyhow::Error) -> Refused {
    eprintln!("gateway: {doing}: {e:#}");
    let why = "the gateway could not check the call";
    (StatusCode::INTERNAL_SERVER_ERROR, why)
}

/// The call of `agent` on `branch` with worker token `token`, and the agent's provider: 401
/// unless the token is `branch`'s live worker token, 503 while its worker is not linked or its
/// limits are not on record, 429 while its pool of calls in flight is in use (a model call takes
/// one; a token count none), 404 for an agent not in the branch's team, 409 once the branch is no
/// longer running.
pub(super) async fn open(
    state: &AppState,
    token: &str,
    (branch, agent): (&str, &str),
    kind: Kind,
) -> Result<(Linked, Option<String>), Refused> {
    worker_token_opens(state, token, branch).await?;
    let unavailable = |why| (StatusCode::SERVICE_UNAVAILABLE, why);
    let link = state.links.started(branch).ok_or(unavailable(NOT_LINKED))?;
    if !link.limits_known {
        return Err(unavailable(NO_LIMITS));
    }
    let caller = (link.caller()).expect("a started link records for its caller");
    let spec = agent_of(state, &caller, branch, agent).await?;
    let permit = match kind {
        Kind::ModelCall => Some(in_flight(&link).await?),
        Kind::TokenCount => None,
    };
    let linked = Linked {
        caller,
        branch: branch.to_string(),
        link,
        _inflight: permit,
    };
    Ok((linked, spec.provider))
}

/// One of `link`'s calls in flight, once one is free (within [`INFLIGHT_WAIT`], else 429).
async fn in_flight(link: &Link) -> Result<OwnedSemaphorePermit, Refused> {
    let waited = tokio::time::timeout(INFLIGHT_WAIT, link.inflight.clone().acquire_owned());
    let permit = waited.await.ok().and_then(Result::ok);
    permit.ok_or((StatusCode::TOO_MANY_REQUESTS, TOO_MANY))
}

/// Ok when `token` is the live worker token of `branch`.
async fn worker_token_opens(state: &AppState, token: &str, branch: &str) -> Result<(), Refused> {
    let found = worker_caller(&state.store, token, Action::CallModel, Some(branch)).await;
    let found = found.map_err(|e| internal("resolving a worker token", e))?;
    found
        .map(drop)
        .ok_or((StatusCode::UNAUTHORIZED, WORKER_TOKEN_WANTED))
}

/// `agent` of the running `branch` as the store has it (the control box wrote its team).
async fn agent_of(
    state: &AppState,
    caller: &Caller,
    branch: &str,
    agent: &str,
) -> Result<AgentSpec, Refused> {
    let unknown = (StatusCode::NOT_FOUND, "unknown branch or agent");
    let found = state.store.at(caller).find_branch(branch).await;
    let found = found.map_err(|e| internal("reading the branch", e))?;
    let b = found.ok_or(unknown)?;
    if b.status != BranchStatus::Running {
        return Err((StatusCode::CONFLICT, crate::flow::STOPPED));
    }
    let mut agents = b.team.agents.into_iter();
    agents.find(|a| a.id == agent).ok_or(unknown)
}

/// The 409 of a paused branch, saying so in [`HELD_HEADER`].
fn paused() -> Response {
    let mut r = error_response(StatusCode::CONFLICT, PAUSED);
    let held = HeaderValue::from_static("paused");
    r.headers_mut().insert(HELD_HEADER, held);
    r
}

/// Why a call of `model` on `p` can't be made for a linked branch: its cost could not be known,
/// so the run's spend limit could not hold (the provider reports no cost and the price table
/// has no price for the model).
pub(super) fn unpriced(p: &Provider, model: &str) -> Option<String> {
    let known = p.cost_source().is_some() || crate::prices::table().find(&p.id, model).is_some();
    let why = format!(
        "model {model:?} on {} has no price in the price table, and a sandbox's run needs one \
         to keep to its spend limit: pick a priced model",
        p.id
    );
    (!known).then_some(why)
}

impl Linked {
    /// Why the call may not go on, by the control box's own word: 409 while the branch is paused
    /// or stopping, 402 once it is stopping for its limit or its spend estimate passed it (then
    /// stopped for it at once, [`Linked::settle`]).
    pub(super) async fn held_back(&self, state: &Arc<AppState>) -> Option<Response> {
        let refused = |status, why: &str| Some(error_response(status, why));
        match self.link.hold() {
            Hold::Paused => return Some(paused()),
            Hold::Stopping(BranchStatus::BudgetExceeded) => {
                return refused(StatusCode::PAYMENT_REQUIRED, &self.link.limit_reason())
            }
            Hold::Stopping(_) => return refused(StatusCode::CONFLICT, crate::flow::STOPPED),
            Hold::Running => {}
        }
        let why = self.link.budget.over()?;
        self.settle(state).await;
        refused(StatusCode::PAYMENT_REQUIRED, &why)
    }

    /// Once a call put the run over its limit (or made its cost unknowable), the branch is
    /// stopped for it at once, not at its next call: its hold is kept first (a restart can't
    /// lose it), then the limit is recorded reached at the branch's last world clock, then its
    /// worker is told (or, none connected, the branch ended).
    pub(super) async fn settle(&self, state: &Arc<AppState>) {
        let Some(why) = self.link.budget.over() else {
            return;
        };
        if !self.link.budget.first_refusal() {
            return;
        }
        let (asked, branch) = (Hold::OVER_BUDGET, &self.branch);
        let kept = holds::keep(state, &self.caller, branch, asked).await;
        let at = state.store.at(&self.caller);
        let clock_ms = at.last_clock_ms(branch).await.unwrap_or(0);
        let ev = json!({"part": "budget", "kind": "limit_reached", "text": why, "data": null,
                        "clock_ms": clock_ms});
        let recorded = at.append(branch, None, EventKind::WorldEvent, ev).await;
        let told = match kept {
            Ok((changed, now)) => holds::tell(state, branch, (asked, now), (changed, &why)).await,
            Err(e) => Err(e),
        };
        if let Err(e) = recorded.map(drop).and(told) {
            eprintln!("gateway: stopping {branch} for its limit: {e:#}");
        }
    }
}
