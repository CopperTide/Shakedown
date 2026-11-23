//! A `shaked worker`'s gateway: its agents' model calls go on to the control box's gateway
//! (`Start.gateway`) with the worker token, and with no model key, for a sandbox holds none.
//! The control box picks the run's key, calls the provider and records the call as its one
//! `model_call` event; the worker records nothing of it and passes the answer back as it comes,
//! a stream chunk by chunk. Here the agent's own token is checked, a paused branch holds the
//! call, a save point is made, and shaked's loop's call is priced into the run's estimate with
//! what the control box says it cost ([`COST_HEADER`]). The control box's word wins: a call it
//! holds for a pause this worker has not heard of yet goes again once the pause ends here, and
//! its 402 puts the run over its limit here too.

use std::{sync::Arc, time::Duration};

use axum::{
    body::{Body, Bytes},
    http::{
        header::{CONTENT_TYPE, RETRY_AFTER},
        HeaderMap, StatusCode,
    },
    response::{IntoResponse, Response},
    Router,
};

use super::{
    admit, error_response,
    linked::HELD_HEADER,
    passthrough::api_headers,
    routes,
    target::{self, Route},
    Api, COST_HEADER,
};
use crate::{app::AppState, exec::BranchCtx, providers::Secret};

/// Where a worker's gateway sends its agents' model calls.
#[derive(Debug, Clone)]
pub struct ForwardTo {
    /// The control box's gateway, `…/gateway`.
    pub gateway: String,
    /// The worker token (redacted in `Debug`).
    pub token: Secret,
}

/// The worker's gateway routes, each forwarding to the control box's.
pub fn router() -> Router<Arc<AppState>> {
    routes(forward)
}

/// How soon a call the control box held for a pause this worker has not heard of goes again.
const PAUSE_POLL: Duration = Duration::from_millis(200);

/// A call on `route`, let through here as on the control box (the agent's token, the pause, the
/// save point), then sent on. Its body is read once, and only looked at enough to refuse one that
/// is no JSON object: the control box checks it whole.
async fn forward(state: Arc<AppState>, route: Route, headers: HeaderMap, body: Body) -> Response {
    let Some(to) = state.config.forward_to.clone() else {
        let why = "this gateway has no control box to send model calls to";
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, why);
    };
    let (target, _) = match admit::let_through(&state, &headers, &route).await {
        Ok(found) => found,
        Err(refused) => return *refused,
    };
    let ctx = target
        .live()
        .expect("a worker's gateway lets through its own agents' calls only");
    let body = match admit::read_body(body, &route).await {
        Ok(body) => body,
        Err(refused) => return *refused,
    };
    if !body.trim_ascii_start().starts_with(b"{") {
        let why = "request body must be a JSON object";
        return error_response(StatusCode::BAD_REQUEST, why);
    }
    let call = Onward {
        state: &state,
        ctx,
        to: &to,
        route: &route,
        headers: &headers,
    };
    call.send(body).await
}

/// One call on its way to the control box.
struct Onward<'a> {
    state: &'a AppState,
    ctx: &'a BranchCtx,
    to: &'a ForwardTo,
    route: &'a Route,
    headers: &'a HeaderMap,
}

/// True when the control box held the call because the run is paused.
fn held_for_pause(answer: &reqwest::Response) -> bool {
    let held = answer.headers().get(HELD_HEADER);
    answer.status() == StatusCode::CONFLICT && held.is_some_and(|v| v == "paused")
}

impl Onward<'_> {
    /// Sends the call on until the control box takes it, and passes its answer back.
    async fn send(&self, body: Bytes) -> Response {
        loop {
            let sent = onward(&self.state.stream_http, self.to, self.route, self.headers);
            let answer = match sent.body(body.clone()).send().await {
                Ok(answer) => answer,
                Err(e) => {
                    let e = e.without_url();
                    let why = format!("the control box's gateway did not answer: {e}");
                    return error_response(StatusCode::BAD_GATEWAY, why);
                }
            };
            if !held_for_pause(&answer) {
                return self.answered(answer).await;
            }
            tokio::time::sleep(PAUSE_POLL).await;
            if let Some(refused) = target::held_back(self.ctx).await {
                return refused;
            }
        }
    }

    /// The control box's answer: a 402 puts the run over its limit here too; shaked's loop's
    /// call is priced with what it cost.
    async fn answered(&self, answer: reqwest::Response) -> Response {
        if answer.status() == StatusCode::PAYMENT_REQUIRED {
            return self.over_budget(answer).await;
        }
        if self.route.api == Api::Chat {
            charge(self.ctx, &self.route.agent, answer.headers());
        }
        relayed(answer)
    }

    /// The control box refused the call for the run's limit: the run is over it here too.
    async fn over_budget(&self, answer: reqwest::Response) -> Response {
        let text = answer.text().await.unwrap_or_default();
        let said = serde_json::from_str::<serde_json::Value>(&text).ok();
        let why = said.and_then(|v| v["error"].as_str().map(str::to_string));
        self.ctx
            .budget
            .force_over(why.unwrap_or_else(|| text.clone()));
        let json = [(CONTENT_TYPE, "application/json")];
        (StatusCode::PAYMENT_REQUIRED, json, text).into_response()
    }
}

/// The request to the control box's route for `route`: the worker token its only credential,
/// and of the agent's headers only its content type and the API's version and betas.
fn onward(
    http: &reqwest::Client,
    to: &ForwardTo,
    route: &Route,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    let base = to.gateway.trim_end_matches('/');
    let (branch, agent, path) = (&route.branch, &route.agent, route.api.path());
    let url = format!("{base}/{branch}/{agent}/v1/{path}");
    let mut sent = api_headers(headers);
    if let Some(kind) = headers.get(CONTENT_TYPE) {
        sent.insert(CONTENT_TYPE, kind.clone());
    }
    http.post(url).headers(sent).bearer_auth(to.token.expose())
}

/// Adds what the control box says the call cost to the run's estimate.
fn charge(ctx: &BranchCtx, agent: &str, headers: &reqwest::header::HeaderMap) {
    if let Some(usd) = said_cost(headers) {
        let _ = ctx.budget.add(agent, usd);
    }
}

/// What the control box says a call cost ([`COST_HEADER`]): nothing for a call it sends none
/// for (a failed one, or one whose cost is unknown) or one it can't be read from.
fn said_cost(headers: &reqwest::header::HeaderMap) -> Option<f64> {
    let cost = headers.get(COST_HEADER)?.to_str().ok()?;
    cost.trim().parse().ok()
}

/// The control box's answer, passed back as it comes: its status, content type, `retry-after`
/// and body, chunk by chunk.
fn relayed(answer: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(answer.status().as_u16());
    let status = status.unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for name in [CONTENT_TYPE, RETRY_AFTER] {
        if let Some(v) = answer.headers().get(&name) {
            headers.insert(name, v.clone());
        }
    }
    let chunks = futures_util::stream::unfold(Some(answer), |answer| async move {
        let mut answer = answer?;
        match answer.chunk().await {
            Ok(Some(chunk)) => Some((Ok(chunk), Some(answer))),
            Ok(None) => None,
            Err(e) => Some((Err(std::io::Error::other(e.without_url())), None)),
        }
    });
    (status, headers, Body::from_stream(chunks)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worker_reads_a_cost_only_where_the_control_box_said_one() {
        let mut h = reqwest::header::HeaderMap::new();
        assert_eq!(said_cost(&h), None, "a failed call says none");
        h.insert(COST_HEADER, "0.004".parse().unwrap());
        assert_eq!(said_cost(&h), Some(0.004));
        h.insert(COST_HEADER, "unknown".parse().unwrap());
        assert_eq!(said_cost(&h), None);
    }

    /// Nothing the agent sent as a credential goes on: only the worker token.
    #[test]
    fn a_forwarded_call_carries_only_the_worker_token() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer sda_agent".parse().unwrap());
        h.insert("x-api-key", "sda_agent".parse().unwrap());
        h.insert("anthropic-version", "2023-06-01".parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        h.insert("cookie", "c=1".parse().unwrap());
        let to = ForwardTo {
            gateway: "http://control/gateway/".into(),
            token: Secret::new("sdw_worker"),
        };
        let route = Route {
            branch: "b1".into(),
            agent: "a".into(),
            api: Api::Messages,
        };
        let req = onward(&reqwest::Client::new(), &to, &route, &h)
            .build()
            .unwrap();
        assert_eq!(
            req.url().as_str(),
            "http://control/gateway/b1/a/v1/messages"
        );
        let sent = req.headers();
        let names: Vec<&str> = sent.keys().map(|k| k.as_str()).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert_eq!(sent["authorization"], "Bearer sdw_worker");
        assert!(sent["authorization"].is_sensitive());
        assert_eq!(sent["anthropic-version"], "2023-06-01");
        assert_eq!(sent["content-type"], "application/json");
    }
}
