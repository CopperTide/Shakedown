//! Model calls a branch's world makes itself (docs/protocol.md, "World model calls"): a part's
//! simulated person on `/gateway/{branch}/world.<part>/v1/chat/completions`, the benchmark's
//! scoring on `…/world.scoring/…`, each with the branch's world token. The call is recorded under
//! its actor (`world.<who>`, or `world.<who>.<name>` when `X-Shakedown-Actor` names who speaks),
//! charged and metered like an agent's, with no save point before it and without making it the
//! branch's last model caller: it is made inside a tool call or the scoring, which hold the
//! branch's gate.

use std::sync::Arc;

use axum::{
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::Response,
};
use parts::proto::{ACTOR_HEADER, PROVIDER_HEADER};

use super::{error_response, target::Route, Api, Refused};
use crate::{
    app::AppState,
    exec::BranchCtx,
    world::models::{is_world_actor, SCORING, WORLD_ACTOR_PREFIX},
};

/// True when `route`'s actor is the world's.
pub(super) fn is_world(route: &Route) -> bool {
    is_world_actor(Some(&route.agent))
}

/// `route` with the actor its call is recorded under: `world.<who>`, or `world.<who>.<name>` when
/// the call names who speaks. 400 for a name that isn't `[a-z0-9_-]{1,40}`; 404 on another API
/// than chat completions, or for an actor that is already more than `world.<who>`.
pub(super) fn routed(mut route: Route, headers: &HeaderMap) -> Result<Route, Box<Response>> {
    let who = &route.agent[WORLD_ACTOR_PREFIX.len()..];
    if route.api != Api::Chat || who.is_empty() || who.contains('.') {
        let why = "a world's model calls go to chat/completions of world.<part> or world.scoring";
        return Err(Box::new(error_response(StatusCode::NOT_FOUND, why)));
    }
    if let Some(name) = header(headers, ACTOR_HEADER) {
        if !valid_name(name) {
            let why = "X-Shakedown-Actor must match [a-z0-9_-]{1,40}";
            return Err(Box::new(error_response(StatusCode::BAD_REQUEST, why)));
        }
        route.agent = format!("{}.{name}", route.agent);
    }
    Ok(route)
}

fn header<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    let v = headers.get(name).and_then(|v| v.to_str().ok())?;
    Some(v.trim()).filter(|v| !v.is_empty())
}

/// `[a-z0-9_-]{1,40}`.
fn valid_name(name: &str) -> bool {
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-';
    (1..=40).contains(&name.len()) && name.chars().all(ok)
}

/// The live branch a world actor's call is for, and the provider the call names
/// (`X-Shakedown-Provider`; None: the default one): 404 when the branch isn't live, gave its world
/// no models, or `who` is neither a part of its world nor the scoring; 401 without the branch's
/// world token.
pub(super) fn live_world(
    state: &AppState,
    headers: &HeaderMap,
    route: &Route,
) -> Result<(Arc<BranchCtx>, Option<String>), Refused> {
    let unknown = (StatusCode::NOT_FOUND, "unknown branch or world actor");
    let ctx = state.live.get(&route.branch).ok_or(unknown)?;
    let models = ctx.world.models().ok_or(unknown)?;
    let rest = &route.agent[WORLD_ACTOR_PREFIX.len()..];
    let who = rest.split('.').next().unwrap_or_default();
    if who != SCORING && !ctx.world.part_names().iter().any(|p| p == who) {
        return Err(unknown);
    }
    let authorization = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !models.token_ok(authorization) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "the branch's world token is wanted",
        ));
    }
    let provider = header(headers, PROVIDER_HEADER).map(str::to_string);
    Ok((ctx, provider))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(agent: &str, api: Api) -> Route {
        Route {
            branch: "b".into(),
            agent: agent.into(),
            api,
        }
    }

    fn with_actor(name: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(ACTOR_HEADER, name.parse().unwrap());
        h
    }

    #[test]
    fn a_named_speaker_is_recorded_as_world_who_name() {
        let r = routed(route("world.shop", Api::Chat), &with_actor("customer")).unwrap();
        assert_eq!(r.agent, "world.shop.customer");
        let r = routed(route("world.scoring", Api::Chat), &HeaderMap::new()).unwrap();
        assert_eq!(r.agent, "world.scoring");
    }

    #[test]
    fn a_bad_name_another_api_or_a_deeper_actor_is_refused() {
        let status = |r: Result<Route, Box<Response>>| r.err().unwrap().status();
        let bad = routed(route("world.shop", Api::Chat), &with_actor("Big Customer"));
        assert_eq!(status(bad), StatusCode::BAD_REQUEST);
        let long = routed(route("world.shop", Api::Chat), &with_actor(&"a".repeat(41)));
        assert_eq!(status(long), StatusCode::BAD_REQUEST);
        let other = routed(route("world.shop", Api::Messages), &HeaderMap::new());
        assert_eq!(status(other), StatusCode::NOT_FOUND);
        let deeper = routed(route("world.shop.x", Api::Chat), &HeaderMap::new());
        assert_eq!(status(deeper), StatusCode::NOT_FOUND);
        assert!(valid_name("judge_2-b") && !valid_name("") && !valid_name("a.b"));
    }
}
