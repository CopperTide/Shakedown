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
    /// Which kinds this home keeps in typed tables. Existing file homes keep their behavior.
    fn table_kinds(&self) -> shakedown_project::TableKinds {
        shakedown_project::TableKinds::NONE
    }
    /// A consistent snapshot, including the file store. Table homes override this.
    fn snapshot(&self) -> BoxFuture<'_, Result<shakedown_project::Snapshot, NoProject>> {
        Box::pin(async move {
            let (revision, files) = self.load().await?;
            Ok(shakedown_project::Snapshot {
                revision,
                tables: self.table_kinds(),
                settings: None,
                rows: Vec::new(),
                store: Arc::new(shakedown_project::store::MemStore::read_only(files)),
            })
        })
    }
    /// Persist a typed change, including its files, atomically.
    fn apply(&self, _change: shakedown_project::ChangeSet) -> BoxFuture<'_, Result<(), NoProject>> {
        Box::pin(async {
            Err(NoProject::Failed(anyhow::anyhow!(
                "this home does not support typed changes"
            )))
        })
    }
    /// Creates the settings row and selects this project's identity for subsequent reads.
    fn initialize<'a>(&'a self, _name: &'a str) -> BoxFuture<'a, Result<(), NoProject>> {
        Box::pin(async {
            Err(NoProject::Failed(anyhow::anyhow!(
                "this home does not support typed initialization"
            )))
        })
    }
    /// The most recent changes, newest first.
    fn history(
        &self,
        _limit: i64,
    ) -> BoxFuture<'_, Result<Vec<crate::store::ChangeRow>, NoProject>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Attach an undo note to an existing change.
    fn note<'a>(
        &'a self,
        _change: &'a str,
        _name: &'a str,
        _text: &'a str,
    ) -> BoxFuture<'a, Result<(), NoProject>> {
        Box::pin(async {
            Err(NoProject::Failed(anyhow::anyhow!(
                "this home does not support history notes"
            )))
        })
    }
}
