//! What the routes take from a request before their own checks: a JSON body (read whatever the
//! content type says, its errors `bad request body: …`), a body that may be empty, and the permit
//! to start a run while no cleanup runs.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{FromRequest, FromRequestParts, Request},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use tokio::sync::OwnedRwLockReadGuard;

use super::{check_disk, parse_body, ApiError};
use crate::app::AppState;

/// A JSON request body; 400 `bad request body: …` when it doesn't read as `T`.
pub struct JsonBody<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for JsonBody<T> {
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Response> {
        let body = Bytes::from_request(req, state)
            .await
            .map_err(IntoResponse::into_response)?;
        parse_body(&body)
            .map(JsonBody)
            .map_err(IntoResponse::into_response)
    }
}

/// A JSON request body that may be left empty, which reads as `{}`.
pub struct OptionalJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for OptionalJson<T> {
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Response> {
        let body = Bytes::from_request(req, state)
            .await
            .map_err(IntoResponse::into_response)?;
        let body = match body.is_empty() {
            true => Bytes::from_static(b"{}"),
            false => body,
        };
        parse_body(&body)
            .map(OptionalJson)
            .map_err(IntoResponse::into_response)
    }
}

/// The reason a run or fork is refused while saves are being cleaned up.
pub(super) const CLEANING_UP: &str = "Saves are being cleaned up; try again in a moment";

/// Held while a run or fork starts, so no cleanup drops a save it is about to use: 409 while a
/// cleanup holds the lock. Taken before the request's body is read.
pub struct StartPermit {
    _held: OwnedRwLockReadGuard<()>,
}

impl StartPermit {
    /// The permit, or 409 while a cleanup runs.
    pub fn take(state: &AppState) -> Result<StartPermit, ApiError> {
        let lock = state.cleanup.clone().try_read_owned();
        let held = lock.map_err(|_| ApiError::conflict(CLEANING_UP))?;
        Ok(StartPermit { _held: held })
    }

    /// 507 while the saves use more than the limit.
    pub async fn check_disk(&self, state: &AppState) -> Result<(), ApiError> {
        check_disk(state).await
    }
}

impl FromRequestParts<Arc<AppState>> for StartPermit {
    type Rejection = ApiError;

    async fn from_request_parts(_: &mut Parts, state: &Arc<AppState>) -> Result<Self, ApiError> {
        StartPermit::take(state)
    }
}
