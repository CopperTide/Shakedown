//! A project as the files of a project folder, and a project folder's files loaded into a project.
//! Export writes today's layout as canonical YAML whatever kinds the home keeps as rows, so it
//! can be committed to git; import reads that layout back through the same strict parsing as a
//! folder, item by item, as one change.

use std::sync::Arc;

use crate::error::ProjectError;
use crate::format::{parse, to_yaml, Kind, ProjectFile};
use crate::item::{ItemBody, Problem};
use crate::load::{io, item_file};
use crate::project::{checked_yaml, PROJECT_FILE};
use crate::store::{check_path, Files, Store};
use crate::tables::TableKinds;
use crate::{Id, Project};

/// What an import does to one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Made,
    Replaced,
}

/// An item an import writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub kind: Kind,
    pub id: Id,
    pub body: ItemBody,
    pub action: Action,
}

/// What importing a project folder's files does; what is already the same is left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportPlan {
    /// The settings, when they differ.
    pub settings: Option<ProjectFile>,
    pub items: Vec<Imported>,
    /// The other files of world and setup folders, written as they are.
    pub files: Vec<(String, Arc<[u8]>)>,
    /// Files that cannot be read: nothing of them is written.
    pub problems: Vec<Problem>,
    /// Paths outside a project folder's layout, left out.
    pub skipped: Vec<String>,
}

impl Project {
    /// The project as a project folder's files: `shakedown.yaml` and every item as canonical
    /// YAML, and every other file of the store (world and setup folders) as it is. Left out:
    /// `.shakedown/`, links, and stale files of the kinds kept as rows.
    pub fn export(&self) -> Result<Files, ProjectError> {
        let mut out = Files::new();
        for path in walk(&*self.store, "")? {
            if !left_out(&path, self.tables()) {
                let bytes = self
                    .store
                    .read(&path)
                    .map_err(|e| io("reading", &path, e))?;
                out.insert(path, bytes.into());
            }
        }
        out.insert(
            PROJECT_FILE.into(),
            to_yaml(&self.config)?.into_bytes().into(),
        );
        for item in self.items.values() {
            let yaml = item.body.to_yaml()?;
            out.insert(item_file(item.kind, &item.id), yaml.into_bytes().into());
        }
        self.export_unreadable_rows(&mut out)?;
        Ok(out)
    }

    fn export_unreadable_rows(&self, out: &mut Files) -> Result<(), ProjectError> {
        let Some(db) = &self.db else { return Ok(()) };
        for ((kind, id), row) in &db.rows {
            if self.get(*kind, id).is_none() {
                let path = item_file(*kind, id);
                let yaml = serde_yaml_ng::to_string(&row.body)
                    .map_err(|e| crate::FormatError::new(&path, "", e.to_string()))?;
                out.insert(path, yaml.into_bytes().into());
            }
        }
        Ok(())
    }
    /// What importing `files` (paths in a project folder) would do.
    pub fn plan_import(&self, files: &Files) -> ImportPlan {
        let mut plan = ImportPlan::default();
        for (path, bytes) in files {
            match place(path) {
                Place::Settings => self.plan_settings(&mut plan, bytes),
                Place::Item(kind, id) => self.plan_item(&mut plan, kind, id, path, bytes),
                Place::File if self.store.read(path).ok().as_deref() != Some(&bytes[..]) => {
                    plan.files.push((path.clone(), bytes.clone()));
                }
                Place::File => {}
                Place::Outside => plan.skipped.push(path.clone()),
            }
        }
        plan
    }

    fn plan_settings(&self, plan: &mut ImportPlan, bytes: &[u8]) {
        let read = std::str::from_utf8(bytes).map_err(|e| e.to_string());
        let parsed =
            read.and_then(|t| parse::<ProjectFile>(PROJECT_FILE, t).map_err(|e| e.to_string()));
        match parsed {
            Ok(config) if config != self.config => plan.settings = Some(config),
            Ok(_) => {}
            Err(message) => plan.problems.push(problem(None, PROJECT_FILE, message)),
        }
    }

    fn plan_item(&self, plan: &mut ImportPlan, kind: Kind, id: Id, path: &str, bytes: &[u8]) {
        let read = std::str::from_utf8(bytes).map_err(|e| e.to_string());
        let body = read.and_then(|t| ItemBody::parse(kind, path, t).map_err(|e| e.to_string()));
        let action = match (&body, self.get(kind, &id)) {
            (Ok(b), Some(now)) if now.body == *b => return,
            (_, Some(_)) => Action::Replaced,
            (_, None) => Action::Made,
        };
        match body {
            Ok(body) => plan.items.push(Imported {
                kind,
                id,
                body,
                action,
            }),
            Err(message) => plan
                .problems
                .push(problem(Some(kind), id.as_str(), message)),
        }
    }

    /// Writes what `plan` imports as one change: the settings, the items (rows or files) and the
    /// other files.
    pub fn import_files(&mut self, plan: &ImportPlan) -> Result<(), ProjectError> {
        let label = "import files";
        self.pending(label);
        if let Some(config) = &plan.settings {
            self.set_config(config.clone())?;
        }
        for (path, bytes) in &plan.files {
            self.write_file(label, path, bytes)?;
        }
        for i in &plan.items {
            let yaml = checked_yaml(i.kind, &i.id, &i.body)?;
            self.store_item(label, i.kind, &i.id, &i.body, &yaml, None)?;
        }
        self.reload()
    }
}

/// Where a path sits in a project folder.
enum Place {
    Settings,
    Item(Kind, Id),
    /// A file of a world's or setup's folder.
    File,
    Outside,
}

fn place(path: &str) -> Place {
    if path == PROJECT_FILE {
        return Place::Settings;
    }
    if check_path(path).is_err() || path.starts_with(".shakedown/") {
        return Place::Outside;
    }
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    let kind = Kind::ALL.into_iter().find(|k| k.dir() == parts[0]);
    let id = |s: &str| Id::parse(s).ok();
    match (kind, parts.as_slice()) {
        (Some(Kind::World), [_, w, "world.yaml"]) => {
            id(w).map_or(Place::Outside, |w| Place::Item(Kind::World, w))
        }
        (Some(Kind::World | Kind::Setup), [_, folder, _]) if id(folder).is_some() => Place::File,
        (Some(k), [_, file]) if k != Kind::World => {
            let stem = file.strip_suffix(".yaml").and_then(id);
            stem.map_or(Place::Outside, |i| Place::Item(k, i))
        }
        _ => Place::Outside,
    }
}

fn problem(kind: Option<Kind>, id: &str, message: String) -> Problem {
    Problem {
        kind,
        id: Some(id.to_string()),
        pointer: String::new(),
        message,
    }
}

/// Every file under folder `dir` of `store`, links left out.
fn walk(store: &dyn Store, dir: &str) -> Result<Vec<String>, ProjectError> {
    let entries = match store.list(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io("listing", dir, e)),
    };
    let mut out = Vec::new();
    for e in entries {
        let path = match dir {
            "" => e.name.clone(),
            _ => format!("{dir}/{}", e.name),
        };
        match (e.link, e.dir) {
            (true, _) => {}
            (false, true) if path != ".git" && path != ".shakedown" => {
                out.extend(walk(store, &path)?)
            }
            (false, true) => {}
            (false, false) => out.push(path),
        }
    }
    Ok(out)
}

/// Whether export leaves `path` out: `.shakedown/`, and an item file of a kind kept as rows.
fn left_out(path: &str, tables: TableKinds) -> bool {
    let stale = matches!(place(path), Place::Item(kind, _) if tables.has(kind));
    let outside = matches!(place(path), Place::Outside) && path != ".gitignore";
    outside || stale
}
