//! The project folder shaked serves (`SHAKEDOWN_PROJECT`). Reads see edits made outside the app:
//! the folder is re-read whenever its files change, so a hand edit or a `git pull` shows up on the
//! next request.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use shakedown_project::format::Kind;
use shakedown_project::{Project, ProjectError};

use crate::app::AppState;

/// Why no project can be served.
#[derive(Debug, thiserror::Error)]
pub enum NoProject {
    /// shaked was started without `SHAKEDOWN_PROJECT`.
    #[error("no project is open: start shaked with SHAKEDOWN_PROJECT set to a project folder")]
    NotConfigured,
    /// The folder could not be opened as a project.
    #[error("no project is open: {0}")]
    Unreadable(#[from] ProjectError),
}

/// The open project, re-read when its files change.
#[derive(Debug)]
pub struct ProjectState {
    root: Option<PathBuf>,
    cached: Mutex<Option<(Stamp, Project)>>,
}

/// What changes whenever a file in the project is added, removed or edited.
type Stamp = (Option<SystemTime>, usize);

impl ProjectState {
    /// The project at `root` (none when `root` is `None`), opened now so problems show at start.
    pub fn new(root: Option<PathBuf>) -> ProjectState {
        let state = ProjectState {
            root,
            cached: Mutex::new(None),
        };
        match state.current() {
            Ok(p) if !p.problems.is_empty() => {
                eprintln!("shaked: the project has {} problems", p.problems.len())
            }
            Err(NoProject::Unreadable(e)) => eprintln!("shaked: could not open the project: {e}"),
            _ => {}
        }
        state
    }

    /// The configured folder, if any.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The project as it is on disk now.
    pub fn current(&self) -> Result<Project, NoProject> {
        let root = self.root.as_deref().ok_or(NoProject::NotConfigured)?;
        let stamp = stamp(root);
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((seen, project)) = cached.as_ref() {
            if *seen == stamp {
                return Ok(project.clone());
            }
        }
        let project = Project::open(root)?;
        *cached = Some((stamp, project.clone()));
        Ok(project)
    }

    /// Runs `change` on the project as it is on disk now, then forgets the cached copy.
    pub fn change<R>(&self, change: impl FnOnce(&mut Project) -> R) -> Result<R, NoProject> {
        let mut project = self.current()?;
        let result = change(&mut project);
        *self.cached.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(result)
    }
}

/// Every project folder this shaked serves, one [`ProjectState`] per folder, opened on first use.
#[derive(Debug)]
pub struct Projects {
    /// `SHAKEDOWN_PROJECT`, the folder the layer is offered.
    configured: Option<PathBuf>,
    open: Mutex<HashMap<PathBuf, Arc<ProjectState>>>,
    none: Arc<ProjectState>,
}

impl Projects {
    /// The projects of a shaked started with `configured` (`SHAKEDOWN_PROJECT`).
    pub fn new(configured: Option<PathBuf>) -> Projects {
        let projects = Projects {
            configured,
            open: Mutex::default(),
            none: Arc::new(ProjectState::new(None)),
        };
        if let Some(dir) = projects.configured.clone() {
            projects.get(Some(dir));
        }
        projects
    }

    /// The configured folder, if any.
    pub fn configured(&self) -> Option<&Path> {
        self.configured.as_deref()
    }

    /// The project at `dir`, shared by every caller of that folder; `None` is no project.
    pub fn get(&self, dir: Option<PathBuf>) -> Arc<ProjectState> {
        let Some(dir) = dir else {
            return self.none.clone();
        };
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let state = open.entry(dir.clone());
        state
            .or_insert_with(|| Arc::new(ProjectState::new(Some(dir))))
            .clone()
    }
}

impl AppState {
    /// The project `c` works in, as the layer files it.
    pub fn project_for(&self, c: &crate::auth::Caller) -> Arc<ProjectState> {
        let dir = self
            .store
            .layer()
            .project_dir(c, self.projects.configured());
        self.projects.get(dir)
    }

    /// The person's project, as it is on disk now.
    pub fn project(&self) -> Result<Project, NoProject> {
        self.project_for(&crate::auth::Caller::person()).current()
    }

    /// Changes the person's project with `f`; the next read sees the result.
    pub fn with_project_mut<R>(&self, f: impl FnOnce(&mut Project) -> R) -> Result<R, NoProject> {
        self.project_for(&crate::auth::Caller::person()).change(f)
    }
}

/// The newest modification time and the number of entries under the folders a project reads.
fn stamp(root: &Path) -> Stamp {
    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(Kind::ALL.iter().map(|k| root.join(k.dir())));
    let worlds = std::fs::read_dir(root.join(Kind::World.dir()))
        .into_iter()
        .flatten()
        .flatten();
    dirs.extend(worlds.map(|e| e.path()).filter(|p| p.is_dir()));
    let mut newest = None;
    let mut count = 0;
    for entry in dirs
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
    {
        count += 1;
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        newest = newest.max(modified);
    }
    (newest, count)
}
