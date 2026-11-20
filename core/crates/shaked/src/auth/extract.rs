//! The caller of an `/api` request, as the auth layer put it in the request's extensions.

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};

use super::Scope;

/// Who is calling: the scope the auth layer built for this request.
#[derive(Debug, Clone)]
pub struct Caller(pub Arc<Scope>);

impl<S: Send + Sync> FromRequestParts<S> for Caller {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Caller, Self::Rejection> {
        match parts.extensions.get::<Arc<Scope>>() {
            Some(s) => Ok(Caller(s.clone())),
            // A route outside the auth layer: fail closed.
            None => Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "no caller"})),
            )),
        }
    }
}

/// The local mode's auth layer: every request that passed the Host guard is the local owner's.
pub async fn local_owner_layer(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    req.extensions_mut().insert(Arc::new(super::local_owner()));
    next.run(req).await
}
