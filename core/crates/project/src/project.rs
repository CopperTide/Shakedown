//! A project: its settings, its items and the problems found in them, over a [`Store`], with the
//! kinds a home keeps as rows on the side ([`crate::rows`]).

use std::path::Path;
use std::sync::Arc;

use crate::error::ProjectError;
use crate::format::{parse, to_yaml, Kind, ProjectFile};
use crate::item::{kind_name, Item, ItemBody, KindFile, Problem};
use crate::load::{io, item_file, load_all, Items};
use crate::rows::Db;
use crate::store::{FsStore, Store};
use crate::tables::TableKinds;
use crate::validate::cross_check;
use crate::Id;

pub(crate) const PROJECT_FILE: &str = "shakedown.yaml";
const GITIGNORE: &str = ".shakedown/\n";

/// An open project.
#[derive(Debug, Clone)]
pub struct Project {
    pub(crate) store: Arc<dyn Store>,
    /// `shakedown.yaml`.
    pub config: ProjectFile,
    /// Unreadable files and broken references, in a stable order.
    pub problems: Vec<Problem>,
    pub(crate) items: Items,
    /// The kinds kept as rows, for a project opened from a [`crate::Snapshot`].
    pub(crate) db: Option<Box<Db>>,
}

impl Project {
    /// Opens the project in folder `root`. See [`Project::open_in`].
    pub fn open(root: &Path) -> Result<Project, ProjectError> {
        Project::open_in(Arc::new(FsStore::new(root)))
    }

    /// Opens the project in `store`. Fails only when `shakedown.yaml` is missing or unreadable;
    /// every other problem is listed in `problems`.
    pub fn open_in(store: Arc<dyn Store>) -> Result<Project, ProjectError> {
        let config = read_config(&*store)?;
        Project::assemble(store, config, None)
    }

    /// A project of `config` over `store` and `db`, its items read.
    pub(crate) fn assemble(
        store: Arc<dyn Store>,
        config: ProjectFile,
        db: Option<Box<Db>>,
    ) -> Result<Project, ProjectError> {
        let mut project = Project {
            store,
            config,
            problems: Vec::new(),
            items: Items::new(),
            db,
        };
        project.reload()?;
        Ok(project)
    }

    /// Creates a project named `name` in folder `root`. See [`Project::init_in`].
    pub fn init(root: &Path, name: &str, git: bool) -> Result<Project, ProjectError> {
        Project::init_in(Arc::new(FsStore::new(root)), name, git)
    }

    /// Creates a project named `name` in `store`, with git when `git` is true (only a store on
    /// disk can have git).
    pub fn init_in(store: Arc<dyn Store>, name: &str, git: bool) -> Result<Project, ProjectError> {
        if store.exists(PROJECT_FILE) {
            return Err(ProjectError::AlreadyAProject);
        }
        Project::lay_out(&*store, TableKinds::NONE, &ProjectFile::named(name), git)?;
        Project::open_in(store)
    }

    /// Lays out a new project's files in `store`: a folder for each kind that is not a row,
    /// `shakedown.yaml` unless the settings are a row, a `.gitignore`, and git when asked.
    pub fn lay_out(
        store: &dyn Store,
        tables: TableKinds,
        config: &ProjectFile,
        git: bool,
    ) -> Result<(), ProjectError> {
        let git_root = git_folder(store, git)?;
        for kind in Kind::ALL.into_iter().filter(|k| !tables.has(*k)) {
            store
                .make_dir(kind.dir())
                .map_err(|e| io("creating", kind.dir(), e))?;
        }
        if !tables.settings {
            write(store, PROJECT_FILE, &to_yaml(config)?)?;
        }
        if !store.exists(".gitignore") {
            write(store, ".gitignore", GITIGNORE)?;
        }
        match git_root {
            Some(root) => git_init(&root),
            None => Ok(()),
        }
    }

    /// Where the files are.
    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// The folder on disk, for a project that is in one.
    pub fn root(&self) -> Option<&Path> {
        self.store.folder()
    }

    /// Replaces `shakedown.yaml` with `config`, checked exactly as it would be read back.
    pub fn set_config(&mut self, config: ProjectFile) -> Result<(), ProjectError> {
        let yaml = to_yaml(&config)?;
        let checked: ProjectFile = parse(PROJECT_FILE, &yaml)?;
        if checked != config {
            return Err(crate::FormatError::new(
                PROJECT_FILE,
                "",
                "some values cannot be written faithfully",
            )
            .into());
        }
        if self.tables().settings {
            self.change_settings(config);
            return Ok(());
        }
        self.write_file("edit settings", PROJECT_FILE, yaml.as_bytes())?;
        self.config = config;
        Ok(())
    }

    /// The items of `kind`, ordered by id.
    pub fn list(&self, kind: Kind) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, item)| item)
            .collect()
    }

    /// The item of `kind` named `id`.
    pub fn get(&self, kind: Kind, id: &Id) -> Option<&Item> {
        self.items.get(&(kind, id.clone()))
    }

    /// The body of item `id` of the kind `B` is the file format of.
    pub fn body<B: KindFile>(&self, id: &Id) -> Option<&B> {
        self.get(B::KIND, id).and_then(|item| B::of(&item.body))
    }

    /// The problems on one item.
    pub fn problems_of(&self, kind: Kind, id: &Id) -> Vec<&Problem> {
        let id = id.as_str();
        self.problems
            .iter()
            .filter(|p| p.kind == Some(kind) && p.id.as_deref() == Some(id))
            .collect()
    }

    /// Writes `body` as item `id` of `kind`, creating or replacing its file (or row).
    pub fn put(&mut self, kind: Kind, id: &Id, body: ItemBody) -> Result<Item, ProjectError> {
        self.put_at(kind, id, body, None)
    }

    /// [`Project::put`], based on the revision `base_revision` the caller read of a row: refused
    /// with [`ProjectError::Changed`] when the row is no longer at it (a file has no revision).
    pub fn put_at(
        &mut self,
        kind: Kind,
        id: &Id,
        body: ItemBody,
        base_revision: Option<i64>,
    ) -> Result<Item, ProjectError> {
        let yaml = checked_yaml(kind, id, &body)?;
        let label = format!("save {} {id}", kind_name(kind));
        self.store_item(&label, kind, id, &body, &yaml, base_revision)?;
        self.reload()?;
        self.get(kind, id)
            .cloned()
            .ok_or_else(|| ProjectError::not_found(kind, id))
    }

    /// Deletes item `id` of `kind` whatever uses it (see [`Project::delete`] for the safe way).
    /// A world's data folder is kept; only its `world.yaml` goes.
    pub fn remove(&mut self, kind: Kind, id: &Id) -> Result<(), ProjectError> {
        if self.get(kind, id).is_none() {
            return Err(ProjectError::not_found(kind, id));
        }
        self.drop_item(&format!("remove {} {id}", kind_name(kind)), kind, id)?;
        self.reload()
    }

    /// The folder that stays when item `id` of `kind` is deleted: a world's data folder, and a
    /// setup's own folder (a MADL setup's team file) when it has one.
    pub fn kept_folder(&self, kind: Kind, id: &Id) -> Option<String> {
        let folder = format!("{}/{id}/", kind.dir());
        let kept = match kind {
            Kind::World => true,
            Kind::Setup => self.store.is_dir(folder.trim_end_matches('/')),
            Kind::Scenario | Kind::Experiment => false,
        };
        kept.then_some(folder)
    }

    /// Re-reads every item from the store and the rows, and re-checks references.
    pub fn reload(&mut self) -> Result<(), ProjectError> {
        let (mut items, mut problems) = load_all(&*self.store, self.tables())?;
        if let Some(db) = &self.db {
            db.load_into(&mut items, &mut problems);
        }
        problems.extend(cross_check(&items));
        problems.extend(crate::views::view_problems(&*self.store, &items));
        self.items = items;
        self.problems = problems;
        Ok(())
    }
}

/// `shakedown.yaml` in `store`: [`ProjectError::NotAProject`] when it is missing.
fn read_config(store: &dyn Store) -> Result<ProjectFile, ProjectError> {
    match store.read_text(PROJECT_FILE) {
        Ok(text) => Ok(parse(PROJECT_FILE, &text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ProjectError::NotAProject),
        Err(e) => Err(io("reading", PROJECT_FILE, e)),
    }
}

/// `body` as the YAML of item `id` of `kind`, checked exactly as it would be read back.
pub(crate) fn checked_yaml(kind: Kind, id: &Id, body: &ItemBody) -> Result<String, ProjectError> {
    if body.kind() != kind {
        return Err(ProjectError::WrongKind {
            expected: kind_name(kind),
            got: kind_name(body.kind()),
        });
    }
    let file = item_file(kind, id);
    let yaml = body.to_yaml()?;
    if ItemBody::parse(kind, &file, &yaml)? != *body {
        let why = "some values cannot be written faithfully (e.g. NaN)";
        return Err(crate::FormatError::new(&file, "", why).into());
    }
    Ok(yaml)
}

/// Replaces `file` in `store` with `contents` in one go.
pub(crate) fn write(store: &dyn Store, file: &str, contents: &str) -> Result<(), ProjectError> {
    store
        .write(file, contents.as_bytes())
        .map_err(|e| io("writing", file, e))
}

/// The folder `git init` runs in when `git` is asked for: a store on disk's own.
fn git_folder(store: &dyn Store, git: bool) -> Result<Option<std::path::PathBuf>, ProjectError> {
    match (git, store.folder()) {
        (false, _) => Ok(None),
        (true, Some(root)) => Ok(Some(root.to_path_buf())),
        (true, None) => Err(ProjectError::NoGitFolder),
    }
}

fn git_init(root: &Path) -> Result<(), ProjectError> {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(io(
            "running git init in",
            ".",
            std::io::Error::other(s.to_string()),
        )),
        Err(e) => Err(io("running git init in", ".", e)),
    }
}
