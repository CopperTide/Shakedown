//! Errors a file can fail with, located by file and JSON pointer.

use crate::format::Kind;
use crate::item::kind_name;
use crate::{Dependent, Id};

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

/// `scenario a, experiment b`.
fn names(by: &[Dependent]) -> String {
    let named: Vec<String> = by
        .iter()
        .map(|d| format!("{} {}", kind_name(d.kind), d.id))
        .collect();
    named.join(", ")
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
    /// Git tracking was asked for on a project that is not a folder on disk.
    #[error("git tracking needs a project folder on disk, and this project is kept in a database: create it without git")]
    NoGitFolder,
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
    /// The item is named by others; deleting it alone would break them.
    #[error(
        "the {kind} {id} is used by {}: delete it with cascade to delete those too",
        names(by)
    )]
    InUse {
        /// The kind, e.g. `world`.
        kind: &'static str,
        /// The item's id.
        id: String,
        /// What uses it, and what a cascading delete would do to each.
        by: Vec<Dependent>,
    },
    /// What uses the item is not what the caller was shown: nothing was deleted.
    #[error(
        "what uses the {kind} {id} changed since it was shown ({}): look again before deleting",
        names(by)
    )]
    Stale {
        /// The kind, e.g. `world`.
        kind: &'static str,
        /// The item's id.
        id: String,
        /// What uses it now.
        by: Vec<Dependent>,
    },
    /// Undoing a delete would overwrite a file made since.
    #[error("{file} exists again: move or delete it before undoing the delete")]
    Occupied {
        /// The file, relative to the project folder.
        file: String,
    },
    /// Undoing a delete would overwrite files edited since the delete rewrote them.
    #[error("undoing the delete would overwrite later edits to {}: undo by hand from the trash, or edit them back", files.join(", "))]
    EditedSince {
        /// The files, relative to the project folder.
        files: Vec<String>,
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

impl ProjectError {
    /// No world is in the group named `name`.
    pub fn no_group(name: &str) -> ProjectError {
        ProjectError::NotFound {
            kind: "world group",
            id: name.to_string(),
        }
    }

    /// No item of `kind` named `id`.
    pub fn not_found(kind: Kind, id: &Id) -> ProjectError {
        ProjectError::NotFound {
            kind: kind_name(kind),
            id: id.to_string(),
        }
    }
}
