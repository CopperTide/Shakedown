//! A project folder: its settings, its items and the problems found in them.

use std::path::{Path, PathBuf};

use crate::error::ProjectError;
use crate::format::{parse, to_yaml, Kind, ProjectFile};
use crate::item::{kind_name, Item, ItemBody, Problem};
use crate::load::{io, item_file, load_all, Items};
use crate::validate::cross_check;
use crate::write::write_atomic;
use crate::Id;

const PROJECT_FILE: &str = "shakedown.yaml";
const GITIGNORE: &str = ".shakedown/\n";

/// An open project folder.
#[derive(Debug, Clone)]
pub struct Project {
    /// The folder.
    pub root: PathBuf,
    /// `shakedown.yaml`.
    pub config: ProjectFile,
    /// Unreadable files and broken references, in a stable order.
    pub problems: Vec<Problem>,
    items: Items,
}

impl Project {
    /// Opens the project at `root`. Fails only when `shakedown.yaml` is missing or unreadable;
    /// every other problem is listed in `problems`.
    pub fn open(root: &Path) -> Result<Project, ProjectError> {
        let text = match std::fs::read_to_string(root.join(PROJECT_FILE)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ProjectError::NotAProject)
            }
            Err(e) => return Err(io("reading", PROJECT_FILE, e)),
        };
        let config = parse(PROJECT_FILE, &text)?;
        let mut project = Project {
            root: root.to_path_buf(),
            config,
            problems: Vec::new(),
            items: Items::new(),
        };
        project.reload()?;
        Ok(project)
    }

    /// Creates a project named `name` at `root`, with git when `git` is true.
    pub fn init(root: &Path, name: &str, git: bool) -> Result<Project, ProjectError> {
        if root.join(PROJECT_FILE).exists() {
            return Err(ProjectError::AlreadyAProject);
        }
        for kind in Kind::ALL {
            std::fs::create_dir_all(root.join(kind.dir()))
                .map_err(|e| io("creating", kind.dir(), e))?;
        }
        let config = ProjectFile {
            format: "shakedown-project/1".into(),
            name: name.into(),
            defaults: Default::default(),
            permissions: Default::default(),
        };
        write_atomic(root, PROJECT_FILE, &to_yaml(&config)?)?;
        if !root.join(".gitignore").exists() {
            write_atomic(root, ".gitignore", GITIGNORE)?;
        }
        if git {
            git_init(root)?;
        }
        Project::open(root)
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

    /// The problems on one item.
    pub fn problems_of(&self, kind: Kind, id: &Id) -> Vec<&Problem> {
        let id = id.as_str();
        self.problems
            .iter()
            .filter(|p| p.kind == Some(kind) && p.id.as_deref() == Some(id))
            .collect()
    }

    /// Writes `body` as item `id` of `kind`, creating or replacing its file.
    pub fn put(&mut self, kind: Kind, id: &Id, body: ItemBody) -> Result<Item, ProjectError> {
        if body.kind() != kind {
            return Err(ProjectError::WrongKind {
                expected: kind_name(kind),
                got: kind_name(body.kind()),
            });
        }
        let file = item_file(kind, id);
        let yaml = body.to_yaml()?;
        if ItemBody::parse(kind, &file, &yaml)? != body {
            return Err(crate::FormatError::new(
                &file,
                "",
                "some values cannot be written faithfully (e.g. NaN)",
            )
            .into());
        }
        write_atomic(&self.root, &file, &yaml)?;
        self.reload()?;
        self.get(kind, id)
            .cloned()
            .ok_or_else(|| not_found(kind, id))
    }

    /// Deletes item `id` of `kind`. A world's data folder is kept; only its `world.yaml` goes.
    pub fn remove(&mut self, kind: Kind, id: &Id) -> Result<(), ProjectError> {
        if self.get(kind, id).is_none() {
            return Err(not_found(kind, id));
        }
        let file = item_file(kind, id);
        std::fs::remove_file(self.root.join(&file)).map_err(|e| io("deleting", &file, e))?;
        self.reload()
    }

    /// Re-reads every item from disk and re-checks references.
    pub fn reload(&mut self) -> Result<(), ProjectError> {
        let (items, mut problems) = load_all(&self.root)?;
        problems.extend(cross_check(&items));
        self.items = items;
        self.problems = problems;
        Ok(())
    }
}

fn not_found(kind: Kind, id: &Id) -> ProjectError {
    ProjectError::NotFound {
        kind: kind_name(kind),
        id: id.to_string(),
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
