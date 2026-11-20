//! The `/api` auth layer of the open-source shaked: every request (past the Host guard) is the
//! local owner's, and inbound gateway headers are dropped.

use std::sync::Arc;

use axum::{extract::Request, middleware::Next, response::Response};

/// Headers a hosting gateway sets; never trusted from a caller here.
const GATEWAY_HEADER_PREFIX: &str = "x-sculptured-";

/// Drops any `x-sculptured-*` header, then acts as the local owner.
pub async fn local(mut req: Request, next: Next) -> Response {
    let gateway: Vec<_> = (req.headers().keys())
        .filter(|k| k.as_str().starts_with(GATEWAY_HEADER_PREFIX))
        .cloned()
        .collect();
    for k in gateway {
        req.headers_mut().remove(k);
    }
    req.extensions_mut().insert(Arc::new(super::local_owner()));
    next.run(req).await
}
