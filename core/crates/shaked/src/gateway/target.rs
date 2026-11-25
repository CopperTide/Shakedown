//! Whom a model call through the gateway is for: a branch this shaked runs (the call carries its
//! agent's token, [`live_agent`]), or one a sandbox's worker runs (the call carries that branch's
//! worker token, and the branch must still be running, [`super::linked`]). [`CallFor`] does each
//! per-branch step of a call the way its branch wants: the hold, the save point, the record. A
//! linked branch's events are appended straight to the store, under the same per-branch lock as
//! the worker's own, so the two interleave in one seq order.

use std::{sync::Arc, time::Instant};

use axum::{
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::Response,
};
use serde_json::Value;

use super::{
    error_response,
    linked::{self, Linked},
    Api, Refused,
};
use crate::{
    app::AppState,
    auth::{run_tokens::prefix, Caller, RunTokenKind},
    exec::BranchCtx,
    model::*,
    overhead::Cat,
    providers::Provider,
    reasoning::{self, ReasoningItem},
};

/// The route a call came in on.
pub(super) struct Route {
    pub branch: String,
    pub agent: String,
    pub api: Api,
}

/// What a route's call is: a model call (in flight while it runs, saved before, admitted by the
/// layer, recorded) or a token count (only checked and held).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    ModelCall,
    TokenCount,
}

impl Route {
    pub(super) fn kind(&self) -> Kind {
        match self.api {
            Api::CountTokens => Kind::TokenCount,
            Api::Chat | Api::Messages | Api::Responses => Kind::ModelCall,
        }
    }
}

/// The branch a call is for.
pub(super) enum CallFor {
    /// A branch this shaked runs.
    Live(Arc<BranchCtx>),
    /// A branch a sandbox's worker runs.
    Linked(Linked),
}

/// The credential a call carries: its bearer token, else its `x-api-key` (Claude Code sends
/// its key that way).
fn credential(headers: &HeaderMap) -> Option<String> {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let bearer = header(AUTHORIZATION.as_str()).and_then(|a| a.strip_prefix("Bearer "));
    let given = bearer.or_else(|| header("x-api-key"))?;
    Some(given.trim().to_string())
}

/// Whom the call on `route` is for, and the agent's provider (None: the default). A worker's
/// gateway serves only its own agents: a worker token there is no one's (401).
pub(super) async fn resolve(
    state: &AppState,
    headers: &HeaderMap,
    route: &Route,
) -> Result<(CallFor, Option<String>), Box<Response>> {
    let refused = |(status, why)| Box::new(error_response(status, why));
    let (branch, agent) = (route.branch.as_str(), route.agent.as_str());
    match credential(headers) {
        Some(t)
            if t.starts_with(prefix(RunTokenKind::Worker)) && state.config.forward_to.is_none() =>
        {
            let opened = linked::open(state, &t, (branch, agent), route.kind()).await;
            let (l, provider) = opened.map_err(refused)?;
            Ok((CallFor::Linked(l), provider))
        }
        given => {
            let bearer = given.map(|t| format!("Bearer {t}"));
            let found = live_agent(state, branch, agent, bearer.as_deref());
            let (ctx, provider) = found.map_err(refused)?;
            Ok((CallFor::Live(ctx), provider))
        }
    }
}

/// The live `branch` and the provider of its `agent` (`None`: the default one): 404 when the
/// branch isn't live or has no such agent, 401 when `authorization` (the header's value,
/// `Bearer …`) isn't the agent's token. The gateway's and the agent's MCP endpoint's check.
pub(crate) fn live_agent(
    state: &AppState,
    branch: &str,
    agent: &str,
    authorization: Option<&str>,
) -> Result<(Arc<BranchCtx>, Option<String>), Refused> {
    let unknown = (StatusCode::NOT_FOUND, "unknown branch or agent");
    let ctx = state.live.get(branch).ok_or(unknown)?;
    let provider = ctx.roster.provider_of(agent).ok_or(unknown)?;
    if !ctx.roster.token_ok(agent, authorization) {
        let why = crate::external::TOKEN_WANTED;
        return Err((StatusCode::UNAUTHORIZED, why));
    }
    Ok((ctx, provider))
}

/// Why the call can't go on once the live branch lets it: 409 when the branch stopped (a paused
/// one holds the call until it resumes, a wait that is no part of the gateway's), 402 past the
/// spend limit of a run an external system drives.
pub(super) async fn held_back(ctx: &BranchCtx) -> Option<Response> {
    if ctx.flow.go().await.is_err() {
        return Some(error_response(StatusCode::CONFLICT, crate::flow::STOPPED));
    }
    let why = over_the_limit(ctx).await?;
    Some(error_response(StatusCode::PAYMENT_REQUIRED, why))
}

/// Why a run an external system drives may make no more model calls: its spend estimate passed
/// its limit (shaked's own runners end such a run themselves). The first refusal is recorded as
/// `budget/limit_reached`; nothing is sent upstream.
async fn over_the_limit(ctx: &BranchCtx) -> Option<String> {
    if ctx.rules.runner != Runner::External {
        return None;
    }
    let why = ctx.budget.over()?;
    if ctx.budget.first_refusal() {
        let ev = crate::budget::limit_reached(&why, ctx.clock.now_ms());
        if let Err(e) = ctx.append_now(None, EventKind::WorldEvent, ev).await {
            let branch = &ctx.branch_id;
            eprintln!("gateway: recording the spend limit on {branch}: {e:#}");
        }
    }
    Some(why)
}

impl CallFor {
    /// Whose keys pay and whom the call is metered for.
    pub(super) fn caller(&self) -> &Caller {
        match self {
            CallFor::Live(ctx) => &ctx.caller,
            CallFor::Linked(l) => &l.caller,
        }
    }

    pub(super) fn branch_id(&self) -> &str {
        match self {
            CallFor::Live(ctx) => &ctx.branch_id,
            CallFor::Linked(l) => &l.branch,
        }
    }

    /// The live branch, when this shaked runs it.
    pub(super) fn live(&self) -> Option<&BranchCtx> {
        match self {
            CallFor::Live(ctx) => Some(ctx),
            CallFor::Linked(_) => None,
        }
    }

    /// Why the call can't go on: the branch's pause, stop or spend limit (a live branch's pause
    /// holds the call until it resumes; a linked one's is refused).
    pub(super) async fn held_back(&self, state: &Arc<AppState>) -> Option<Response> {
        match self {
            CallFor::Live(ctx) => held_back(ctx).await,
            CallFor::Linked(l) => l.held_back(state).await,
        }
    }

    /// Why a call of `model` on `p` may not be made for this branch: on a linked branch, its
    /// cost could not be known (refused 400).
    pub(super) fn unpriced(&self, p: &Provider, model: &str) -> Option<String> {
        let CallFor::Linked(_) = self else {
            return None;
        };
        linked::unpriced(p, model)
    }

    /// A save point before the call on a live branch, when a part that saves at points changed:
    /// the world is then restorable where a fork at this call starts (a linked branch's worker
    /// makes its own). A failure is logged; the call goes on.
    pub(super) async fn save_point(&self, state: &AppState) {
        let Some(ctx) = self.live() else {
            return;
        };
        let exec = crate::exec::ToolExec::new(state.store.clone());
        if let Err(e) = exec.save_point(ctx, SAVE_BEFORE_MODEL_CALL).await {
            eprintln!("gateway: save point on {}: {e}", ctx.branch_id);
        }
    }

    pub(super) fn note_model_call(&self, agent: &str) {
        if let Some(ctx) = self.live() {
            ctx.roster.note_model_call(agent);
        }
    }

    /// The gateway's own time on a live branch: all since `since` but the provider's answer.
    pub(super) fn overhead(&self, since: Instant, upstream_ms: f64) {
        if let Some(ctx) = self.live() {
            ctx.overhead.add_besides(Cat::Gateway, since, upstream_ms);
        }
    }

    /// Stops a linked branch for its limit as soon as a charge put it over.
    pub(super) async fn settle(&self, state: &Arc<AppState>) {
        if let CallFor::Linked(l) = self {
            l.settle(state).await;
        }
    }

    /// Records an event of the branch for `agent`.
    pub(super) async fn append(
        &self,
        state: &AppState,
        agent: &str,
        kind: EventKind,
        payload: Value,
    ) -> anyhow::Result<Event> {
        match self {
            CallFor::Live(ctx) => ctx.append(Some(agent), kind, payload).await,
            CallFor::Linked(l) => {
                let at = state.store.at(&l.caller);
                at.append(&l.branch, Some(agent), kind, payload).await
            }
        }
    }

    /// Records `items`, the reasoning of the model call at `seq`.
    pub(super) async fn reasoning(
        &self,
        state: &AppState,
        agent: &str,
        items: Vec<ReasoningItem>,
        seq: i64,
    ) {
        for p in reasoning::payloads(items, reasoning::SOURCE_GATEWAY, Some(seq)) {
            let v = serde_json::to_value(p).unwrap_or_default();
            if let Err(e) = self.append(state, agent, EventKind::Reasoning, v).await {
                eprintln!("branch {}: recording reasoning: {e:#}", self.branch_id());
            }
        }
    }
}
