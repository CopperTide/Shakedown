//! Why an operation failed, as an HTTP status and a message safe to show.

use axum::http::StatusCode;
use shakedown_project::{FormatError, ProjectError};

use crate::project_state::NoProject;

/// A failed operation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct OpError {
    /// The HTTP status it answers with.
    pub status: StatusCode,
    /// What went wrong. Never contains a secret.
    pub message: String,
}

impl OpError {
    /// A failure with `status`.
    pub fn new(status: StatusCode, message: impl Into<String>) -> OpError {
        OpError {
            status,
            message: message.into(),
        }
    }

    /// The input was malformed (400).
    pub fn bad_request(message: impl Into<String>) -> OpError {
        OpError::new(StatusCode::BAD_REQUEST, message)
    }
}

impl From<NoProject> for OpError {
    fn from(e: NoProject) -> OpError {
        OpError::new(StatusCode::CONFLICT, e.to_string())
    }
}

impl From<FormatError> for OpError {
    fn from(e: FormatError) -> OpError {
        OpError::new(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

impl From<ProjectError> for OpError {
    fn from(e: ProjectError) -> OpError {
        let status = match &e {
            ProjectError::NotFound { .. } => StatusCode::NOT_FOUND,
            ProjectError::InvalidId(_)
            | ProjectError::Format(_)
            | ProjectError::WrongKind { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            ProjectError::NotAProject | ProjectError::AlreadyAProject => StatusCode::CONFLICT,
            ProjectError::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        OpError::new(status, e.to_string())
    }
}
