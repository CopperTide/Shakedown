//! The query face: `ANY /q/{prometheus|loki|tempo}/{*path}`, or any path at the root of a host
//! whose first label names the store (`prometheus.<domain>`, as agents reach it through arenad's
//! proxy, which sends the public host as `X-Forwarded-Host`). An agent's query without its time
//! is asked at world now (`shift::default_params`). A query with `X-Shakedown-Shift: wall`
//! (Grafana's datasources) has its window moved from wall to world time and the answer's times
//! moved back, so dashboards follow the world clock. Query parameters may come in the URL or,
//! for a POST, as a form.

use std::sync::Arc;

use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;

use crate::clock::wall_now_ns;
use crate::shift::{self, Params};
use crate::{upstream_answer, Gateway};

const SHIFT: &str = "x-shakedown-shift";
const MAX_BODY: usize = 1024 * 1024;
/// Headers not passed on: the connection's own, the shift and an encoding the gateway would have to undo.
const DROPPED: [&str; 6] = [
    "host",
    "content-length",
    "accept-encoding",
    "connection",
    "transfer-encoding",
    SHIFT,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Prometheus,
    Loki,
    Tempo,
}

impl Backend {
    fn named(name: &str) -> Option<Backend> {
        match name {
            "prometheus" => Some(Backend::Prometheus),
            "loki" => Some(Backend::Loki),
            "tempo" => Some(Backend::Tempo),
            _ => None,
        }
    }

    fn base(self, g: &Gateway) -> &str {
        match self {
            Backend::Prometheus => &g.up.prometheus,
            Backend::Loki => &g.up.loki,
            Backend::Tempo => &g.up.tempo,
        }
    }
}

pub fn router(g: Arc<Gateway>) -> Router {
    Router::new()
        .route("/q/{backend}/{*path}", any(by_path))
        .fallback(by_host)
        .with_state(g)
}

fn not_found() -> Response {
    let why =
        "no store here: ask /q/{prometheus|loki|tempo}/... or a prometheus., loki. or tempo. host";
    (StatusCode::NOT_FOUND, why).into_response()
}

async fn by_path(
    State(g): State<Arc<Gateway>>,
    Path((name, path)): Path<(String, String)>,
    req: Request,
) -> Response {
    match Backend::named(&name) {
        Some(b) => forward(&g, b, &format!("/{path}"), req).await,
        None => not_found(),
    }
}

/// The store the request's host names: `X-Forwarded-Host`, else `Host`, by its first label.
fn host_backend(h: &HeaderMap) -> Option<Backend> {
    let host = (h.get("x-forwarded-host").or_else(|| h.get(header::HOST)))?
        .to_str()
        .ok()?;
    Backend::named(host.split(['.', ':']).next()?)
}

async fn by_host(State(g): State<Arc<Gateway>>, req: Request) -> Response {
    match host_backend(req.headers()) {
        Some(b) => {
            let path = req.uri().path().to_string();
            forward(&g, b, &path, req).await
        }
        None => not_found(),
    }
}

/// The query's parameters as world time wants them: Grafana's window moved, or an agent's time
/// filled in.
fn adjust(b: Backend, path: &str, p: &mut Params, shifting: bool, offset_ms: i64) {
    if shifting {
        shift::shift_params(b, p, offset_ms);
    } else {
        let world_now_ms = (wall_now_ns() / 1_000_000) as i64 + offset_ms;
        shift::default_params(b, path, p, world_now_ms);
    }
}

/// The query's parameters and body, adjusted (see `adjust`): a form body's, else the URL's.
fn adjusted(
    b: Backend,
    path: &str,
    parts: &Parts,
    body: &[u8],
    shifting: bool,
    offset_ms: i64,
) -> (Params, Vec<u8>) {
    let form = (parts.headers.get(header::CONTENT_TYPE)).is_some_and(|t| {
        t.as_bytes()
            .starts_with(b"application/x-www-form-urlencoded")
    });
    let mut query = shift::parse(parts.uri.query().unwrap_or("").as_bytes());
    if !form {
        adjust(b, path, &mut query, shifting, offset_ms);
        return (query, body.to_vec());
    }
    let mut f = shift::parse(body);
    adjust(b, path, &mut f, shifting, offset_ms);
    (query, shift::encode(&f).into_bytes())
}

async fn forward(g: &Gateway, b: Backend, path: &str, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let Ok(body) = to_bytes(body, MAX_BODY).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, "the query is too large").into_response();
    };
    let shifting = parts.headers.get(SHIFT).is_some_and(|v| v == "wall");
    let offset_ms = g.clock.offset_ms();
    let (query, body) = adjusted(b, path, &parts, &body, shifting, offset_ms);
    let url = match query.is_empty() {
        true => format!("{}{path}", b.base(g)),
        false => format!("{}{path}?{}", b.base(g), shift::encode(&query)),
    };
    let mut headers = parts.headers.clone();
    DROPPED.iter().for_each(|h| drop(headers.remove(*h)));
    let sent = g
        .http
        .request(parts.method, url)
        .headers(headers)
        .body(body);
    let back = move |answer: Vec<u8>| match shifting {
        true => shift::shift_answer(b, answer, offset_ms),
        false => answer,
    };
    upstream_answer(sent.send().await, back).await
}
