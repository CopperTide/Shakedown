//! Where a caller's project lives, as the layer says ([`crate::layer::Layer::project_home`]): a
//! folder on disk, or a database the layer keeps (a hosting layer that serves others can keep
//! each project there, so none of them lives as files on its server).

use std::path::PathBuf;
use std::sync::Arc;

use shakedown_project::store::{Change, Files};

use super::NoProject;
use crate::layer::BoxFuture;

/// Where a caller's project lives.
#[derive(Clone)]
pub enum ProjectHome {
    /// A project folder: personal use's `SHAKEDOWN_PROJECT`.
    Folder(PathBuf),
    /// A project in a database, read and written as the caller it was made for.
    Db(Arc<dyn ProjectDb>),
}

impl std::fmt::Debug for ProjectHome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectHome::Folder(dir) => f.debug_tuple("Folder").field(dir).finish(),
            ProjectHome::Db(db) => f.debug_tuple("Db").field(&db.key()).finish(),
        }
    }
}

/// A project kept in a database, by file: the engine reads every file into memory, changes the
/// copy as it would a folder, and saves what changed in one transaction. A handle acts for one
/// caller; handles with the same [`key`](ProjectDb::key) are the same project.
pub trait ProjectDb: Send + Sync + 'static {
    /// The project's name among the database's projects (the scope it belongs to), for the
    /// engine's cache.
    fn key(&self) -> String;
    /// A number that changes whenever the project's files do (0 before the first save).
    fn revision(&self) -> BoxFuture<'_, Result<i64, NoProject>>;
    /// Every file of the project and the revision they are at.
    fn load(&self) -> BoxFuture<'_, Result<(i64, Files), NoProject>>;
    /// Saves `changes` (a file's new contents, or `None` to remove it) in one transaction while
    /// the project is still at revision `base`, answering the new revision. A project changed
    /// since `base` is refused with 409, a change past the database's limits with 413.
    fn save(&self, base: i64, changes: Vec<Change>) -> BoxFuture<'_, Result<i64, NoProject>>;
}
