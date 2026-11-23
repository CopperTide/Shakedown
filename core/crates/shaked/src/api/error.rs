//! Why a request or an operation failed: one HTTP status and a message safe to show, with the
//! mapping from every error the routes meet, and the authorize check that answers in it.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::{
    app::AppState,
    auth::{Action, Caller, Decision, Target},
    fork::ForkError,
    model::TeamError,
    runner,
};

/// A failed request or operation (`ops::OpError` is the same type).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ApiError {
    /// The HTTP status it answers with.
    pub status: StatusCode,
    /// What went wrong. Never contains a secret.
    pub message: String,
}

/// A route's answer: JSON, or the error.
pub(crate) type ApiResult<T> = Result<Json<T>, ApiError>;

impl ApiError {
    /// A failure with `status`.
    pub fn new(status: StatusCode, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            message: message.into(),
        }
    }

    /// What went wrong, as the caller sees it.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The input was malformed (400).
    pub fn bad_request(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, message)
    }

    /// 400, as `bad_request` (the name the routes use; the operations use the other).
    pub(crate) fn bad(message: impl Into<String>) -> ApiError {
        ApiError::bad_request(message)
    }

    pub(crate) fn not_found(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, message)
    }

    /// 404 `there is no {what} "{id}"`.
    pub(crate) fn missing(what: &str, id: &str) -> ApiError {
        ApiError::not_found(format!("there is no {what} \"{id}\""))
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::FORBIDDEN, message)
    }

    pub(crate) fn conflict(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::CONFLICT, message)
    }

    pub(crate) fn insufficient_storage(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::INSUFFICIENT_STORAGE, message)
    }

    /// 500 with the whole of `e`, whatever caused it (no SQLSTATE is read, unlike `from`).
    pub fn internal(e: anyhow::Error) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
    }
}

impl From<anyhow::Error> for ApiError {
    /// A write the database refused for this caller (SQLSTATE 42501, insufficient privilege: a
    /// hosting layer's policies) is 403; a unique violation (23505) is 409, naming the constraint;
    /// anything else is 500 (its text hidden where the layer serves others, see `app::router`).
    fn from(e: anyhow::Error) -> ApiError {
        use tokio_postgres::error::SqlState;
        let pg = (e.chain()).find_map(|c| c.downcast_ref::<tokio_postgres::Error>());
        match pg.and_then(|p| p.code()) {
            Some(c) if *c == SqlState::INSUFFICIENT_PRIVILEGE => {
                ApiError::forbidden("you may not do that")
            }
            Some(c) if *c == SqlState::UNIQUE_VIOLATION => {
                let db = pg.and_then(|p| p.as_db_error());
                let which = db.and_then(|d| d.constraint()).unwrap_or("a unique key");
                ApiError::conflict(format!("that already exists ({which})"))
            }
            _ => ApiError::internal(e),
        }
    }
}

/// Whether `caller` may do `action` on `target`: the engine's own checks (`auth::engine_check`),
/// then the layer's `authorize`. 404 when not found, 403 when refused.
pub async fn authorize(
    state: &AppState,
    caller: &Caller,
    action: Action,
    target: Target<'_>,
) -> Result<(), ApiError> {
    let decision = match crate::auth::engine_check(caller, &action, &target) {
        Decision::Allow => state.store.layer().authorize(caller, action, target).await,
        refused => refused,
    };
    match decision {
        Decision::Allow => Ok(()),
        Decision::NotFound => Err(ApiError::not_found("not found")),
        Decision::Forbidden(why) => Err(ApiError::forbidden(why)),
    }
}

impl From<crate::layer::Refusal> for ApiError {
    fn from(r: crate::layer::Refusal) -> ApiError {
        ApiError::new(r.status, r.message)
    }
}

impl From<runner::RunError> for ApiError {
    fn from(e: runner::RunError) -> ApiError {
        match e {
            runner::RunError::UnknownTask(_) => ApiError::not_found(e.to_string()),
            runner::RunError::Refused(r) => r.into(),
            runner::RunError::Other(e) => e.into(),
        }
    }
}

impl From<TeamError> for ApiError {
    fn from(e: TeamError) -> ApiError {
        ApiError::bad(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> ApiError {
        anyhow::Error::from(e).into()
    }
}

impl From<ForkError> for ApiError {
    fn from(e: ForkError) -> ApiError {
        let status = match &e {
            ForkError::UnknownBranch(_) => StatusCode::NOT_FOUND,
            ForkError::BranchRunning
            | ForkError::Refused(_)
            | ForkError::ReplayDiverged { .. }
            | ForkError::FingerprintMismatch { .. } => StatusCode::CONFLICT,
            ForkError::UnknownSeq
            | ForkError::TeamChange(_)
            | ForkError::UnknownAgent(_)
            | ForkError::UnknownTool(_)
            | ForkError::NoTools(_)
            | ForkError::Disruptions(_)
            | ForkError::MissingHandoff(_)
            | ForkError::NoRestorePoint { .. } => StatusCode::BAD_REQUEST,
            ForkError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError::new(status, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}
