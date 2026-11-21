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
