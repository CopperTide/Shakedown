//! Whom a model call through the gateway is for, and what lets it through: a branch this shaked
//! runs (the call carries its agent's token), or one a sandbox's worker runs (the call carries
//! that branch's worker token, and the branch must still be running). Every route admits a call
//! the same way ([`admitted`]), finds its provider the same way ([`provider`]) and bills it the
//! same way ([`CallFor::bill`]). A linked branch's events are appended straight to the store,
//! under the same per-branch lock as the worker's own, so the two interleave in one seq order.

use std::{sync::Arc, time::Instant};

use axum::{
    body::{Body, Bytes},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::Response,
};
use serde_json::{json, Value};

use super::{
    call_cost,
    charge::{self, Charge},
    error_response, held_back,
    linked::{self, Linked},
    live_agent, model_call,
    raw_call::marked,
    save_before_model_call, Api,
};
use crate::{
    app::AppState,
    auth::{run_tokens::prefix, Caller, RunTokenKind},
    exec::BranchCtx,
    layer::KeySource,
    model::*,
    overhead::Cat,
    providers::{Provider, ProviderKind},
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

/// How a call to a provider ended, for its bill: it failed (nothing is charged), answered, or
/// its stream was cut before it said what it used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Failed,
    Answered,
    Cut,
}

impl Outcome {
    /// The outcome of an answer with `status`, cut for the reason `interrupted` when given.
    pub(super) fn of(status: u16, interrupted: Option<&str>) -> Outcome {
        match ((200..300).contains(&status), interrupted) {
            (false, _) => Outcome::Failed,
            (true, None) => Outcome::Answered,
            (true, Some(_)) => Outcome::Cut,
        }
    }
}

/// The credential a call carries: its bearer token, else its `x-api-key` (Claude Code sends
/// its key that way).
pub(super) fn credential(headers: &HeaderMap) -> Option<String> {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let bearer = header(AUTHORIZATION.as_str()).and_then(|a| a.strip_prefix("Bearer "));
    let given = bearer.or_else(|| header("x-api-key"))?;
    Some(given.trim().to_string())
}

/// Whom the call on `route` is for, and the agent's provider (None: the default). A worker's
/// gateway serves only its own agents: a worker token there is no one's (401).
async fn resolve(
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
        _ => {
            let (ctx, provider) = live_agent(state, branch, agent, headers).map_err(refused)?;
            Ok((CallFor::Live(ctx), provider))
        }
    }
}

/// The call on `route`, let through to its body: whom it is for, not held back by its branch,
/// and (a model call) saved before and noted. Its body is read only then: a call nobody may make
/// never has it read.
pub(super) async fn let_through(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    route: &Route,
) -> Result<(CallFor, Option<String>), Box<Response>> {
    let (target, provider) = resolve(state, headers, route).await?;
    if let Some(refused) = target.held_back(state).await {
        return Err(Box::new(refused));
    }
    if route.kind() == Kind::ModelCall {
        target.save_point(state).await;
        target.note_model_call(&route.agent);
    }
    Ok((target, provider))
}

/// The body of a call on `route`, up to its API's limit (413 past it).
pub(super) async fn read_body(body: Body, route: &Route) -> Result<Bytes, Box<Response>> {
    let limit = route.api.max_body();
    let read = axum::body::to_bytes(body, limit).await;
    read.map_err(|_| {
        let why = format!("request body over {} MiB, or unreadable", limit >> 20);
        Box::new(error_response(StatusCode::PAYLOAD_TOO_LARGE, why))
    })
}

/// A call ready to go to its provider: for whom, since when, the provider, whose key pays, and
/// the call as it is sent.
pub(super) struct Prepared {
    pub target: CallFor,
    pub since: Instant,
    pub provider: Provider,
    pub paid_by: KeySource,
    pub request: Value,
}

/// The call on `route` [`let_through`], its body a JSON object, (a model call) admitted by the
/// layer, and its provider found ([`provider`]).
pub(super) async fn prepared(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    route: &Route,
    body: Body,
) -> Result<Prepared, Box<Response>> {
    let (target, provider) = let_through(state, headers, route).await?;
    let since = Instant::now();
    let bytes = read_body(body, route).await?;
    let request = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object);
    let why = "request body must be a JSON object";
    let request = request.ok_or_else(|| error_response(StatusCode::BAD_REQUEST, why))?;
    let asked = provider.or_else(|| route.api.default_provider().map(str::to_string));
    if route.kind() == Kind::ModelCall {
        layer_admits(state, &target, route, (asked.as_deref(), &request)).await?;
    }
    let (provider, paid_by, request) =
        self::provider(state, &target, route, asked.as_deref(), request).await?;
    Ok(Prepared {
        target,
        since,
        provider,
        paid_by,
        request,
    })
}

/// Ok when the layer admits the model call `request` to the provider `asked` (its quotas).
async fn layer_admits(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    (asked, request): (Option<&str>, &Value),
) -> Result<(), Box<Response>> {
    let op = model_call(&route.branch, asked.unwrap_or_default(), request);
    let admitted = crate::layer::admit(&state.store, target.caller(), op).await;
    admitted
        .map(drop)
        .map_err(|r| Box::new(error_response(r.status, r.message)))
}

/// The provider `asked` names for `request` on `route`, with the key chosen for the branch's
/// caller: 502 when it can't be found (a model call is recorded as failed), 400 when it doesn't
/// speak the route's API or (on a linked branch) its cost could not be known. A model call is held
/// to a lent provider's cap, and OpenRouter is asked to report its cost.
async fn provider(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    asked: Option<&str>,
    request: Value,
) -> Result<(Provider, KeySource, Value), Box<Response>> {
    let model = request["model"].as_str().unwrap_or_default().to_string();
    let keys = crate::providers::keys::provider_for(state, target.caller(), asked, &model);
    let (found, paid_by) = keys.await;
    let found = match found {
        Ok(p) => p,
        Err(why) => return Err(unreached(state, target, route, request, why).await),
    };
    let refused = |why| Box::new(error_response(StatusCode::BAD_REQUEST, why));
    let provider = route.api.speaking(found).map_err(refused)?;
    if let Some(why) = target.unpriced(&provider, &model) {
        return Err(refused(why));
    }
    let request = match route.kind() {
        Kind::ModelCall => capped(route.api, &provider, request),
        Kind::TokenCount => request,
    };
    Ok((provider, paid_by, request))
}

/// `request` as it goes to `p`: OpenRouter's Responses are asked to report their cost (its chat
/// adapter asks itself), and a lent provider's limits (its `cap`) hold: `max_tokens`, or
/// Responses' `max_output_tokens`.
fn capped(api: Api, p: &Provider, mut request: Value) -> Value {
    if p.kind == ProviderKind::OpenRouter && api == Api::Responses {
        request["usage"] = json!({"include": true});
    }
    let Some(cap) = p.cap else {
        return request;
    };
    if api != Api::Responses {
        return cap.apply(p.kind, request);
    }
    let asked = request["max_output_tokens"].as_u64();
    request["max_output_tokens"] = json!(asked.map_or(cap.max_tokens, |n| n.min(cap.max_tokens)));
    request
}

/// The 502 of a call whose provider could not be found (`why`); a model call is recorded as a
/// failed call, as one the provider turned down is.
async fn unreached(
    state: &AppState,
    target: &CallFor,
    route: &Route,
    request: Value,
    why: String,
) -> Box<Response> {
    if route.kind() == Kind::ModelCall {
        let payload = ModelCallPayload {
            request,
            response: json!({ "error": why }),
            usage: json!({}),
            cost_usd: None,
            cost_source: None,
            provider: None,
            provider_response: None,
        };
        let mut v = serde_json::to_value(&payload).unwrap_or_default();
        marked(&mut v, route.api, None);
        let recorded = target.append(state, &route.agent, EventKind::ModelCall, v);
        if let Err(e) = recorded.await {
            eprintln!("gateway: recording a model call on {}: {e:#}", route.branch);
        }
    }
    Box::new(error_response(StatusCode::BAD_GATEWAY, why))
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
    fn unpriced(&self, p: &Provider, model: &str) -> Option<String> {
        let CallFor::Linked(_) = self else {
            return None;
        };
        linked::unpriced(p, model)
    }

    /// A save point before the call, on a live branch (a linked branch's worker makes its own).
    async fn save_point(&self, state: &AppState) {
        if let Some(ctx) = self.live() {
            save_before_model_call(state, ctx).await;
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

    /// Bills `agent`'s call `p` to `provider` that ended `outcome`: what it is charged (its cost;
    /// on a linked branch a conservative estimate when that can't be known, [`charge::linked`]),
    /// added to the run's spend estimate, and the call's event, which on a linked branch keeps
    /// what was charged (`charged_usd`, `estimated`) for a restarted control box to rebuild the
    /// estimate from.
    pub(super) fn bill(
        &self,
        agent: &str,
        (provider, p): (&Provider, &ModelCallPayload),
        outcome: Outcome,
    ) -> (Charge, Value) {
        let cost = (outcome != Outcome::Failed)
            .then(|| call_cost(provider, p))
            .flatten();
        let cut = outcome == Outcome::Cut;
        let charge = match (self, outcome) {
            (CallFor::Linked(l), Outcome::Answered | Outcome::Cut) => {
                charge::linked(&l.link.budget, (provider, p), cost, cut)
            }
            _ => Charge::of(cost),
        };
        let mut v = serde_json::to_value(p).unwrap_or_default();
        let Some(usd) = charge.usd else {
            return (charge, v);
        };
        let _ = match self {
            CallFor::Live(ctx) => ctx.budget.add(agent, usd),
            CallFor::Linked(l) => l.link.budget.add(agent, usd),
        };
        if let CallFor::Linked(_) = self {
            v["charged_usd"] = json!(usd);
            if charge.estimated {
                v["estimated"] = json!(true);
            }
        }
        (charge, v)
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
        if let CallFor::Live(ctx) = self {
            return reasoning::record(ctx, agent, items, reasoning::SOURCE_GATEWAY, Some(seq))
                .await;
        }
        for mut item in items {
            item.model_call_seq = item.model_call_seq.or(Some(seq));
            let Some(p) = reasoning::payload(&item, reasoning::SOURCE_GATEWAY) else {
                continue;
            };
            let v = serde_json::to_value(p).unwrap_or_default();
            if let Err(e) = self.append(state, agent, EventKind::Reasoning, v).await {
                eprintln!("branch {}: recording reasoning: {e:#}", self.branch_id());
            }
        }
    }
}
