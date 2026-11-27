//! The repo's worlds, scenarios, setups and experiments, copied into the importing project. An
//! id the project has already is kept unless this import wrote it and it is unchanged since.
//! An item the last import wrote that the repo no longer has is removed (to the project's trash,
//! so it can be undone) when it is unchanged since, nothing left uses it and no other import
//! holds it; otherwise it is kept. Items made in the project, and runs, are never removed.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use shakedown_project::format::Kind;
use shakedown_project::{item_file, kind_name, Id, Project, ProjectError};

use super::record::{Record, RecordedItem};
use super::ImportError;

/// What happens to one of the repo's items.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// New to the project.
    Write,
    /// Written by the last import and unchanged since: replaced.
    Replace,
    /// The project has it already, the same.
    Same,
    /// The project has its own: kept, saying why.
    Keep(String),
    /// The last import wrote it, the repo no longer has it, and it is unchanged since: removed.
    Remove,
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::Write => "write",
            Action::Replace => "replace",
            Action::Same => "same",
            Action::Keep(_) => "keep",
            Action::Remove => "remove",
        }
    }
}

/// One of the repo's items and what happens to it.
pub struct ItemPlan {
    pub kind: Kind,
    pub id: Id,
    pub label: String,
    pub action: Action,
    /// The digest of the repo's copy (of the project's, for an item the repo no longer has).
    pub digest: String,
    /// Planned for removal but kept because something left in the project uses it: still this
    /// import's, so a later import removes it once nothing does.
    pub in_use: bool,
}

/// The digest of an item where it lives under `root`: its file, or a world's whole folder.
pub fn digest(root: &Path, kind: Kind, id: &Id) -> Option<String> {
    let mut h = Sha256::new();
    match kind {
        Kind::World => {
            let dir = root.join(Kind::World.dir()).join(id.as_str());
            let mut files = vec![];
            files_under(&dir, &dir, &mut files);
            if files.is_empty() {
                return None;
            }
            files.sort();
            for f in files {
                h.update(f.as_bytes());
                h.update([0]);
                h.update(std::fs::read(dir.join(&f)).ok()?);
                h.update([0]);
            }
        }
        _ => h.update(std::fs::read(root.join(item_file(kind, id))).ok()?),
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn files_under(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            files_under(base, &p, out);
        } else if let Ok(r) = p.strip_prefix(base) {
            out.push(r.to_string_lossy().into_owned());
        }
    }
}

/// What happens to each of `repo`'s items in the project at `target` (which may not be a
/// project yet), given what the last import of this repo wrote.
pub fn plan(repo: &Project, target: &Path, last: Option<&Record>) -> Vec<ItemPlan> {
    let Some(root) = repo.root() else {
        return vec![];
    };
    let mut out = vec![];
    for kind in Kind::ALL {
        for item in repo.list(kind) {
            let Some(digest) = digest(root, kind, &item.id) else {
                continue;
            };
            let wrote = last.and_then(|r| r.item(kind, item.id.as_str()));
            out.push(ItemPlan {
                action: action(target, kind, &item.id, &digest, wrote),
                kind,
                id: item.id.clone(),
                label: item.label.clone(),
                digest,
                in_use: false,
            });
        }
    }
    out.extend(last.map_or(vec![], |last| gone(repo, target, last)));
    out
}

/// The items `last` wrote that `repo` no longer has and the project at `target` still holds,
/// those that use others first (experiments, setups, scenarios, worlds): removed when unchanged
/// since and no other import of the project holds it, else kept.
fn gone(repo: &Project, target: &Path, last: &Record) -> Vec<ItemPlan> {
    let here = Project::open(target).ok();
    let others: Vec<Record> = (super::record::all(target).into_iter())
        .map(|(_, r)| r)
        .filter(|r| r.id != last.id)
        .collect();
    let mut out = vec![];
    for kind in Kind::ALL.into_iter().rev() {
        let wrote = last.items.iter().filter(|i| i.kind == kind_name(kind));
        let gone = wrote.filter_map(|w| {
            let id = Id::parse(&w.id).ok()?;
            if repo.get(kind, &id).is_some() {
                return None;
            }
            let shared = others.iter().find(|r| r.item(kind, &w.id).is_some());
            let held = Held {
                last: w,
                shared: shared.map(|r| r.id.as_str()),
            };
            gone_one(here.as_ref(), target, kind, id, held)
        });
        out.extend(gone);
    }
    out
}

/// What the import records of an item the repo no longer has: what the last import wrote, and
/// another import of the project that holds it too.
struct Held<'a> {
    last: &'a RecordedItem,
    shared: Option<&'a str>,
}

/// Item `id` of `kind`, which the repo no longer has: `None` when the project no longer has it
/// either.
fn gone_one(
    here: Option<&Project>,
    target: &Path,
    kind: Kind,
    id: Id,
    held: Held,
) -> Option<ItemPlan> {
    let digest = digest_here(target, kind, &id)?;
    let what = format!("the repo no longer has the {} {id}", kind_name(kind));
    let action = match (digest == held.last.digest, held.shared) {
        (false, _) => Action::Keep(format!(
            "{what}, but the project's was changed since the last import: kept"
        )),
        (true, Some(other)) => {
            Action::Keep(format!("{what}, but the import {other} has it too: kept"))
        }
        (true, None) => Action::Remove,
    };
    let item = here.and_then(|p| p.get(kind, &id));
    let label = item.map_or_else(|| id.to_string(), |i| i.label.clone());
    Some(ItemPlan {
        kind,
        id,
        label,
        action,
        digest,
        in_use: false,
    })
}

/// What happens to item `id` whose repo copy has `digest`, given what the last import wrote.
fn action(
    target: &Path,
    kind: Kind,
    id: &Id,
    digest: &str,
    wrote: Option<&RecordedItem>,
) -> Action {
    let what = format!("{} {id}", kind_name(kind));
    match (digest_here(target, kind, id), wrote) {
        (None, _) => Action::Write,
        (Some(h), _) if h == digest => Action::Same,
        (Some(h), Some(w)) if h == w.digest => Action::Replace,
        (Some(_), Some(_)) => Action::Keep(format!(
            "the project's {what} was changed since the last import: kept"
        )),
        (Some(_), None) => Action::Keep(format!("the project has its own {what}: kept")),
    }
}

/// The digest of item `id` in the project at `target`, when it has one.
fn digest_here(target: &Path, kind: Kind, id: &Id) -> Option<String> {
    let file = target.join(item_file(kind, id));
    file.is_file().then(|| digest(target, kind, id)).flatten()
}

/// Copies the planned items from `repo` into `target` and removes those planned for removal,
/// answering what the project now holds of this import (written, replaced, already the same, or
/// planned for removal but kept because something left uses it; its plan then says so).
pub fn write(
    repo: &Path,
    target: &Path,
    plans: &mut [ItemPlan],
) -> Result<Vec<RecordedItem>, ImportError> {
    for p in plans.iter() {
        if matches!(p.action, Action::Write | Action::Replace) {
            copy_item(repo, target, p.kind, &p.id)?;
        }
    }
    remove_gone(target, plans)?;
    let held = plans.iter().filter(|p| match &p.action {
        Action::Write | Action::Replace | Action::Same => true,
        Action::Keep(_) => p.in_use,
        Action::Remove => false,
    });
    let recorded = held.map(|p| RecordedItem {
        kind: kind_name(p.kind).to_string(),
        id: p.id.to_string(),
        digest: p.digest.clone(),
    });
    Ok(recorded.collect())
}

/// Removes each item planned for removal, in plan order (what uses others first); one that
/// something left in the project uses is kept, its plan saying what uses it.
fn remove_gone(target: &Path, plans: &mut [ItemPlan]) -> Result<(), ImportError> {
    if !plans.iter().any(|p| p.action == Action::Remove) {
        return Ok(());
    }
    let mut project =
        Project::open(target).map_err(|e| ImportError::Io(format!("opening the project: {e}")))?;
    for p in plans.iter_mut().filter(|p| p.action == Action::Remove) {
        match project.delete(p.kind, &p.id, false, None) {
            Ok(_) => {}
            Err(e @ ProjectError::InUse { .. }) => {
                p.action = Action::Keep(format!("the repo no longer has it, but {e}: kept"));
                p.in_use = true;
            }
            Err(e) => return Err(ImportError::Io(format!("removing {}: {e}", p.id))),
        }
    }
    Ok(())
}

fn copy_item(repo: &Path, target: &Path, kind: Kind, id: &Id) -> Result<(), ImportError> {
    if kind != Kind::World {
        let file = item_file(kind, id);
        let to = target.join(&file);
        create_parent(&to)?;
        return std::fs::copy(repo.join(&file), &to)
            .map(|_| ())
            .map_err(|e| ImportError::io("writing", &to, e));
    }
    let rel = PathBuf::from(Kind::World.dir()).join(id.as_str());
    let to = target.join(&rel);
    if to.exists() {
        std::fs::remove_dir_all(&to).map_err(|e| ImportError::io("replacing", &to, e))?;
    }
    copy_dir(&repo.join(&rel), &to)
}

fn create_parent(file: &Path) -> Result<(), ImportError> {
    match file.parent() {
        Some(dir) => std::fs::create_dir_all(dir).map_err(|e| ImportError::io("creating", dir, e)),
        None => Ok(()),
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), ImportError> {
    std::fs::create_dir_all(to).map_err(|e| ImportError::io("creating", to, e))?;
    let rd = std::fs::read_dir(from).map_err(|e| ImportError::io("reading", from, e))?;
    for e in rd.filter_map(Result::ok) {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| ImportError::io("writing", &dst, e))?;
        }
    }
    Ok(())
}
