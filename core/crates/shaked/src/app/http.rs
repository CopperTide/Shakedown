//! The HTTP service around `/api`: the router and its middleware (CORS, the DNS-rebinding
//! guard, authentication and authorization, hiding a 500's text where the layer serves others).

use std::sync::Arc;

use axum::Router;

use super::AppState;

/// Where the layer serves others, a 500's text (a query's error, a path, a constraint) stays in
/// the log: the answer is a generic message with a reference to find the detail by.
async fn hide_internal_errors(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = req.uri().path().to_string();
    let r = next.run(req).await;
    if r.status() != axum::http::StatusCode::INTERNAL_SERVER_ERROR {
        return r;
    }
    let detail = axum::body::to_bytes(r.into_body(), 64 * 1024).await;
    let detail = detail.unwrap_or_default();
    let reference = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let detail = String::from_utf8_lossy(&detail);
    eprintln!("shaked: 500 on {path} (ref {reference}): {detail}");
    let error = format!("something went wrong on our side (ref {reference})");
    let status = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
    (status, axum::Json(serde_json::json!({ "error": error }))).into_response()
}

/// DNS-rebinding guard: a web page on another origin can resolve its own name to this machine, so
/// the browser would send that name in `Host`. Only the allowed names reach the API.
async fn allowed_host(
    axum::extract::State(hosts): axum::extract::State<Arc<Vec<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let ok = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| crate::bind::host_allowed(h, &hosts));
    if !ok {
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"error": "forbidden host"})),
        )
            .into_response();
    }
    next.run(req).await
}

/// The origins of `SHAKEDOWN_CORS_ORIGIN` (comma-separated, each `http(s)://host[:port]` without a
/// path); anything else, `*` included, is skipped with a warning, since a wildcard would let any
/// web page drive this machine's shakedown.
pub fn cors_origins_from(value: &str) -> Vec<String> {
    let mut out = vec![];
    for o in value.split(',').map(|o| o.trim().trim_end_matches('/')) {
        if o.is_empty() {
            continue;
        }
        let rest = o
            .strip_prefix("http://")
            .or_else(|| o.strip_prefix("https://"));
        match rest {
            Some(host) if !host.is_empty() && !host.contains(['/', '*', ' ']) => {
                out.push(o.to_string())
            }
            _ => eprintln!(
                "shaked: SHAKEDOWN_CORS_ORIGIN: skipped {o:?} (expected http(s)://host[:port])"
            ),
        }
    }
    out
}

/// CORS for the origins in `SHAKEDOWN_CORS_ORIGIN`: a request from one of them gets
/// `Access-Control-Allow-Origin`, and its preflight (`OPTIONS`) is answered here. Requests from
/// any other origin are left as they are (the browser then refuses the answer).
async fn cors(
    axum::extract::State(origins): axum::extract::State<Arc<Vec<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue, Method};
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .filter(|o| origins.iter().any(|a| a == o))
        .and_then(|o| HeaderValue::from_str(o).ok());
    let Some(origin) = origin else {
        return next.run(req).await;
    };
    let preflight = req.method() == Method::OPTIONS
        && (req.headers()).contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    let mut res = match preflight {
        true => preflight_answer(),
        false => next.run(req).await,
    };
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    h.append(header::VARY, HeaderValue::from_static("origin"));
    res
}

/// The answer to an allowed origin's CORS preflight (its `Access-Control-Allow-Origin` is added
/// by [`cors`]).
fn preflight_answer() -> axum::response::Response {
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    let mut r = StatusCode::NO_CONTENT.into_response();
    let h = r.headers_mut();
    let methods = HeaderValue::from_static("GET, POST, DELETE, OPTIONS");
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, methods);
    let headers = HeaderValue::from_static("content-type");
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, headers);
    h.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    r
}

pub fn router(state: Arc<AppState>) -> Router {
    let origins = Arc::new(state.config.cors_origins.clone());
    // Locally a tokenless request is the person at this machine: only loopback names reach /api,
    // whatever the config says (a tunnel's Host never does).
    let hosts = Arc::new(match state.store.layer().policy().serves_others {
        true => state.config.hosts(),
        false => crate::bind::loopback_hosts(),
    });
    let authorize =
        axum::middleware::from_fn_with_state(state.clone(), crate::api::routes::authorize_route);
    let authenticate =
        axum::middleware::from_fn_with_state(state.clone(), crate::auth::middleware::authenticate);
    let api = crate::api::router()
        .route_layer(authorize)
        .route("/mcp", axum::routing::post(crate::ops::mcp::serve));
    let api = api.merge(state.store.layer().routes()).layer(authenticate);
    let api = api
        .layer(axum::middleware::from_fn_with_state(hosts, allowed_host))
        .layer(axum::middleware::from_fn_with_state(origins, cors));
    // A worker says who it is in its link's first frame, not in a header: `/worker` is outside
    // `/api`'s auth.
    let all = crate::mcp::router()
        .merge(crate::gateway::router())
        .merge(crate::otel::receive::router())
        .merge(crate::worker::router())
        .merge(api);
    let all = match state.store.layer().policy().serves_others {
        true => all.layer(axum::middleware::from_fn(hide_internal_errors)),
        false => all,
    };
    all.with_state(state)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cors_origins() {
        let got = super::cors_origins_from(
            " http://127.0.0.1:5173, https://ui.example/ ,*,ftp://x,http://a/b,,http://",
        );
        assert_eq!(got, ["http://127.0.0.1:5173", "https://ui.example"]);
    }

    /// A router with one `/api/x` route behind [`super::cors`] for `http://ui.test:5173`, and a
    /// request to it with `method` from `origin`, answered with its status and allowed origin.
    async fn cors_call(method: &str, origin: &str) -> (u16, Option<String>, bool) {
        use axum::{body::Body, http::Request, routing::get, Router};
        use tower::ServiceExt;
        let origins = std::sync::Arc::new(vec!["http://ui.test:5173".to_string()]);
        let app = Router::new()
            .route("/api/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(origins, super::cors));
        let req = Request::builder()
            .method(method)
            .uri("/api/x")
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap();
        let r = app.oneshot(req).await.unwrap();
        let h = r.headers();
        let allow = (h.get("access-control-allow-origin")).map(|v| v.to_str().unwrap().to_string());
        let methods = h.contains_key("access-control-allow-methods");
        (r.status().as_u16(), allow, methods)
    }

    #[tokio::test]
    async fn cors_answers_allowed_origins_only() {
        let ui = "http://ui.test:5173";
        assert_eq!(cors_call("GET", ui).await, (200, Some(ui.into()), false));
        assert_eq!(cors_call("OPTIONS", ui).await, (204, Some(ui.into()), true));
        assert_eq!(
            cors_call("GET", "http://evil.test").await,
            (200, None, false)
        );
        let (status, allow, _) = cors_call("OPTIONS", "http://evil.test").await;
        assert!(status != 204 && allow.is_none());
    }

    /// `host` (a Host header value) names this machine's loopback: `127.0.0.1`, `localhost` or
    /// `[::1]`, with an optional port.
    #[test]
    fn loopback_hosts() {
        let ok = |h| crate::bind::host_allowed(h, &crate::bind::loopback_hosts());
        for h in [
            "127.0.0.1",
            "127.0.0.1:4510",
            "localhost",
            "LOCALHOST:80",
            "[::1]",
            "[::1]:4510",
        ] {
            assert!(ok(h), "{h}");
        }
        for h in [
            "evil.example",
            "127.0.0.1.evil.example",
            "localhost.evil.com:80",
            "127.0.0.1:abc",
            "[::2]",
            "",
            "::1",
        ] {
            assert!(!ok(h), "{h}");
        }
    }
}
