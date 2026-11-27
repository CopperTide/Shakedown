//! Why an operation failed: the API's error (an HTTP status and a message safe to show), and
//! how the project's errors map onto it.

use axum::http::StatusCode;
use shakedown_project::{FormatError, ProjectError};

use crate::project_state::NoProject;

/// A failed operation: the same type as a failed request.
pub type OpError = crate::api::ApiError;

impl From<NoProject> for OpError {
    /// No project is 409; a database's refusal has its own status; its failure is a 500.
    fn from(e: NoProject) -> OpError {
        match e {
            NoProject::Refused { status, message } => OpError::new(status, message),
            NoProject::Failed(e) => OpError::internal(e),
            e => OpError::new(StatusCode::CONFLICT, e.to_string()),
        }
    }
}

impl From<FormatError> for OpError {
    fn from(e: FormatError) -> OpError {
        OpError::new(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

impl From<ProjectError> for OpError {
    /// An item still in use, or whose dependents changed since they were shown, is 409 with its
    /// `dependents` as the dependents preview lists them; git asked of a project kept in a
    /// database is 400.
    fn from(e: ProjectError) -> OpError {
        let status = match &e {
            ProjectError::InUse { by, .. } | ProjectError::Stale { by, .. } => {
                let dependents = super::view::dependents(by);
                return OpError::new(StatusCode::CONFLICT, e.to_string())
                    .with("dependents", dependents);
            }
            ProjectError::NotFound { .. } => StatusCode::NOT_FOUND,
            ProjectError::NoGitFolder => StatusCode::BAD_REQUEST,
            ProjectError::InvalidId(_)
            | ProjectError::Format(_)
            | ProjectError::WrongKind { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            ProjectError::NotAProject
            | ProjectError::AlreadyAProject
            | ProjectError::Occupied { .. }
            | ProjectError::EditedSince { .. } => StatusCode::CONFLICT,
            ProjectError::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        OpError::new(status, e.to_string())
    }
}
