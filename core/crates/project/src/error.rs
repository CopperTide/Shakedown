//! Errors a file can fail with, located by file and JSON pointer.

/// A file that could not be read as its format, with where in the file the problem is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}{}: {message}", at(pointer))]
pub struct FormatError {
    /// The file, relative to the project folder.
    pub file: String,
    /// A JSON pointer to the offending value (`""` for the whole file).
    pub pointer: String,
    /// What is wrong. Never echoes a value that looked like a credential.
    pub message: String,
}

impl FormatError {
    /// An error at `pointer` in `file`.
    pub fn new(file: &str, pointer: impl Into<String>, message: impl Into<String>) -> Self {
        FormatError {
            file: file.to_string(),
            pointer: pointer.into(),
            message: message.into(),
        }
    }
}

fn at(pointer: &str) -> String {
    if pointer.is_empty() {
        String::new()
    } else {
        format!(" at {pointer}")
    }
}

/// Why a project operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    /// The folder has no `shakedown.yaml`.
    #[error("this folder is not a Shakedown project (no shakedown.yaml)")]
    NotAProject,
    /// `init` was asked to create a project where one already exists.
    #[error("this folder is already a Shakedown project")]
    AlreadyAProject,
    /// An id that is not lowercase letters, digits and dashes.
    #[error("{0:?} is not a valid id: use 1–64 lowercase letters, digits and dashes, starting with a letter or digit")]
    InvalidId(String),
    /// No item of this kind has this id.
    #[error("no {kind} named {id}")]
    NotFound {
        /// The kind, e.g. `scenario`.
        kind: &'static str,
        /// The id asked for.
        id: String,
    },
    /// The body given is not of the kind asked for.
    #[error("expected a {expected}, got a {got}")]
    WrongKind {
        /// The kind asked for.
        expected: &'static str,
        /// The kind of the body given.
        got: &'static str,
    },
    /// A file that does not parse, or a body that would not.
    #[error(transparent)]
    Format(#[from] FormatError),
    /// Reading or writing a file failed.
    #[error("{action} {file}: {source}")]
    Io {
        /// What was being done, e.g. `writing`.
        action: &'static str,
        /// The file, relative to the project folder.
        file: String,
        /// The underlying error.
        source: std::io::Error,
    },
}
