//! The project each caller works in, where the layer says it lives ([`ProjectHome`]). A folder
//! (`SHAKEDOWN_PROJECT`) is re-read whenever its files change, so a hand edit or a `git pull`
//! shows up on the next request. A project in a database ([`ProjectDb`]) is read into memory
//! when its revision moves, and each change is saved in one transaction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::http::StatusCode;
use shakedown_project::store::{FsStore, MemStore, Store};
use shakedown_project::{Project, ProjectError};

use crate::app::AppState;

mod folder;
mod home;
mod tables;

pub use home::{ProjectDb, ProjectHome};
pub use shakedown_project::store::{check_path, parsed, Change, Files, MAX_PATH};

/// Why no project can be served.
#[derive(Debug, thiserror::Error)]
pub enum NoProject {
    /// shaked was started without `SHAKEDOWN_PROJECT`.
    #[error("no project is open: start shaked with SHAKEDOWN_PROJECT set to a project folder")]
    NotConfigured,
    /// The folder could not be opened as a project.
    #[error("no project is open: {0}")]
    Unreadable(#[from] ProjectError),
    /// The project's database refused a change, with the status that says why (409: the
    /// project changed meanwhile; 413: past a size limit).
    #[error("{message}")]
    Refused { status: StatusCode, message: String },
    /// The project's database failed.
    #[error("the project could not be read or saved: {0:#}")]
    Failed(anyhow::Error),
}

/// What changes whenever the project does.
#[derive(Debug, Clone, PartialEq)]
enum Stamp {
    Folder(Option<SystemTime>, usize),
    Db(i64),
}

/// The project last read from a home, the stamp it was read at, and (from a database) its files.
#[derive(Debug, Clone)]
struct Cached {
    stamp: Stamp,
    files: Files,
    project: Project,
}

/// The last project read from one home, shared by every caller of that home.
#[derive(Debug, Default)]
struct Cache(Mutex<Option<Cached>>);

impl Cache {
    fn hit(&self, stamp: &Stamp) -> Option<Cached> {
        let cached = self.0.lock().unwrap_or_else(|e| e.into_inner());
        cached.as_ref().filter(|c| c.stamp == *stamp).cloned()
    }

    fn put(&self, entry: Option<Cached>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = entry;
    }
}

/// One caller's project: where it lives and the cache it shares with that home's other callers.
#[derive(Debug)]
pub struct ProjectState {
    home: Option<ProjectHome>,
    cache: Arc<Cache>,
}

impl ProjectState {
    /// The project in folder `root` (none when `root` is `None`), opened now so problems show
    /// at start.
    pub fn new(root: Option<PathBuf>) -> ProjectState {
        let state = ProjectState::at(root.map(ProjectHome::Folder), Arc::default());
        state.report();
        state
    }

    /// The project in folder `root`, opened on first read and saying nothing now (for a folder
    /// that may become a project, as `shaked import`'s).
    pub fn unopened(root: PathBuf) -> ProjectState {
        ProjectState::at(Some(ProjectHome::Folder(root)), Arc::default())
    }

    /// Forgets the cached copy, so the next read opens the project again (after files were
    /// written behind its back, e.g. by an import).
    pub fn forget(&self) {
        self.cache.put(None);
    }

    fn at(home: Option<ProjectHome>, cache: Arc<Cache>) -> ProjectState {
        ProjectState { home, cache }
    }

    /// Logs the problems of a folder's project, or why it could not be opened.
    fn report(&self) {
        let Some(ProjectHome::Folder(root)) = &self.home else {
            return;
        };
        match self.folder_current(root) {
            Ok(p) if !p.problems.is_empty() => {
                eprintln!("shaked: the project has {} problems", p.problems.len())
            }
            Err(NoProject::Unreadable(e)) => eprintln!("shaked: could not open the project: {e}"),
            _ => {}
        }
    }

    /// The project's folder, for a project in one.
    pub fn root(&self) -> Option<&Path> {
        match &self.home {
            Some(ProjectHome::Folder(root)) => Some(root),
            _ => None,
        }
    }

    /// The project as it is now. A project kept in a database is read-only here: change it with
    /// [`change`](Self::change).
    pub async fn current(&self) -> Result<Project, NoProject> {
        match self.home.as_ref().ok_or(NoProject::NotConfigured)? {
            ProjectHome::Folder(root) => self.folder_current(root),
            ProjectHome::Db(db) if !db.table_kinds().is_none() => {
                Ok(Project::open_snapshot(db.snapshot().await?)?)
            }
            ProjectHome::Db(db) => {
                let stamp = Stamp::Db(db.revision().await?);
                if let Some(hit) = self.cache.hit(&stamp) {
                    return Ok(hit.project);
                }
                let (revision, files) = db.load().await?;
                let store = Arc::new(MemStore::read_only(files.clone()));
                let project = Project::open_in(store)?;
                let stamp = Stamp::Db(revision);
                let cached = Cached {
                    stamp,
                    files,
                    project: project.clone(),
                };
                self.cache.put(Some(cached));
                Ok(project)
            }
        }
    }

    fn folder_current(&self, root: &Path) -> Result<Project, NoProject> {
        let (newest, count) = folder::stamp(root);
        let stamp = Stamp::Folder(newest, count);
        if let Some(hit) = self.cache.hit(&stamp) {
            return Ok(hit.project);
        }
        let project = Project::open(root)?;
        let files = Files::new();
        let cached = Cached {
            stamp,
            files,
            project: project.clone(),
        };
        self.cache.put(Some(cached));
        Ok(project)
    }

    /// Runs `change` on the project as it is now; in a database, what it changed is saved in one
    /// transaction when it succeeds (nothing is saved when it fails). The next read sees it.
    pub async fn change<T>(
        &self,
        change: impl FnOnce(&mut Project) -> Result<T, ProjectError>,
    ) -> Result<Result<T, ProjectError>, NoProject> {
        if let Some(ProjectHome::Db(db)) = &self.home {
            if !db.table_kinds().is_none() {
                return self.table_change(&**db, change).await;
            }
        }
        self.edit(|store| {
            let mut project = Project::open_in(store)?;
            Ok(change(&mut project))
        })
        .await
    }

    /// Creates a project named `name` here (see [`Project::init_in`]).
    pub async fn init(
        &self,
        name: &str,
        git: bool,
    ) -> Result<Result<Project, ProjectError>, NoProject> {
        if let Some(ProjectHome::Db(db)) = &self.home {
            if !db.table_kinds().is_none() {
                if git {
                    return Ok(Err(ProjectError::NoGitFolder));
                }
                db.initialize(name).await?;
                return self.current().await.map(Ok);
            }
        }
        self.edit(|s| Ok(Project::init_in(s, name, git))).await
    }

    /// Runs `edit` on the project's store; in a database, saves what it changed when both it
    /// and the result it answers succeed.
    async fn edit<T>(
        &self,
        edit: impl FnOnce(Arc<dyn Store>) -> Result<Result<T, ProjectError>, NoProject>,
    ) -> Result<Result<T, ProjectError>, NoProject> {
        let result = match self.home.as_ref().ok_or(NoProject::NotConfigured)? {
            ProjectHome::Folder(root) => edit(Arc::new(FsStore::new(root))),
            ProjectHome::Db(db) => {
                let (revision, files) = self.db_files(&**db).await?;
                let store = Arc::new(MemStore::new(files));
                let result = edit(store.clone());
                let changes = store.changes();
                if matches!(result, Ok(Ok(_))) && !changes.is_empty() {
                    db.save(revision, changes).await?;
                }
                result
            }
        };
        self.cache.put(None);
        result
    }

    /// The database's files at its revision now, from the cache when it holds that revision.
    async fn db_files(&self, db: &dyn ProjectDb) -> Result<(i64, Files), NoProject> {
        let revision = db.revision().await?;
        match self.cache.hit(&Stamp::Db(revision)) {
            Some(hit) => Ok((revision, hit.files)),
            None => db.load().await,
        }
    }
}

/// Which home a cache is for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Folder(PathBuf),
    Db(String),
}

/// The caches of every project this shaked serves, one per home, made on first use.
#[derive(Debug)]
pub struct Projects {
    /// `SHAKEDOWN_PROJECT`, the folder the layer is offered.
    configured: Option<PathBuf>,
    caches: Mutex<HashMap<Key, Arc<Cache>>>,
}

impl Projects {
    /// The projects of a shaked started with `configured` (`SHAKEDOWN_PROJECT`).
    pub fn new(configured: Option<PathBuf>) -> Projects {
        let projects = Projects {
            configured,
            caches: Mutex::default(),
        };
        if let Some(dir) = projects.configured.clone() {
            projects.get(Some(ProjectHome::Folder(dir))).report();
        }
        projects
    }

    /// The configured folder, if any.
    pub fn configured(&self) -> Option<&Path> {
        self.configured.as_deref()
    }

    /// The project at `home`, sharing its cache with every caller of that home; `None` is no
    /// project.
    pub fn get(&self, home: Option<ProjectHome>) -> Arc<ProjectState> {
        let key = match &home {
            None => return Arc::new(ProjectState::at(None, Arc::default())),
            Some(ProjectHome::Folder(dir)) => Key::Folder(dir.clone()),
            Some(ProjectHome::Db(db)) => Key::Db(db.key()),
        };
        let mut caches = self.caches.lock().unwrap_or_else(|e| e.into_inner());
        let cache = caches.entry(key).or_default().clone();
        Arc::new(ProjectState::at(home, cache))
    }
}

impl AppState {
    /// The project `c` works in, where the layer keeps it.
    pub fn project_for(&self, c: &crate::auth::Caller) -> Arc<ProjectState> {
        let home = self
            .store
            .layer()
            .project_home(c, self.projects.configured());
        self.projects.get(home)
    }

    /// The person's project, as it is now.
    pub async fn project(&self) -> Result<Project, NoProject> {
        self.project_for(&crate::auth::Caller::person())
            .current()
            .await
    }

    /// Changes the person's project with `f`; the next read sees the result.
    pub async fn with_project_mut<T>(
        &self,
        f: impl FnOnce(&mut Project) -> Result<T, ProjectError>,
    ) -> Result<Result<T, ProjectError>, NoProject> {
        self.project_for(&crate::auth::Caller::person())
            .change(f)
            .await
    }
}

#[cfg(test)]
mod tests;
