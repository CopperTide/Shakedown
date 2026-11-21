//! The caller of an `/api` request, as the auth layer put it in the request's extensions.

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};

use super::Caller;

/// Who is calling: the caller the auth layer put on this request.
#[derive(Debug, Clone)]
pub struct Who(pub Arc<Caller>);

/// The answer to a request no caller was put on (a route outside the auth layer): fail closed.
pub fn no_caller() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "no caller"})),
    )
}

impl<S: Send + Sync> FromRequestParts<S> for Who {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Who, Self::Rejection> {
        match parts.extensions.get::<Arc<Caller>>() {
            Some(c) => Ok(Who(c.clone())),
            None => Err(no_caller()),
        }
    }
}
