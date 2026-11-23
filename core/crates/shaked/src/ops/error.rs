//! Why an operation failed: the API's error (an HTTP status and a message safe to show), and
//! how the project's errors map onto it.

use axum::http::StatusCode;
use shakedown_project::{FormatError, ProjectError};

use crate::project_state::NoProject;

/// A failed operation: the same type as a failed request.
pub type OpError = crate::api::ApiError;

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
