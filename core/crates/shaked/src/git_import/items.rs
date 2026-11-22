//! The repo's worlds, scenarios, setups and experiments, copied into the importing project. An
//! id the project has already is kept unless this import wrote it and it is unchanged since.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use shakedown_project::format::Kind;
use shakedown_project::{item_file, kind_name, Id, Project};

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
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::Write => "write",
            Action::Replace => "replace",
            Action::Same => "same",
            Action::Keep(_) => "keep",
        }
    }
}

/// One of the repo's items and what happens to it.
pub struct ItemPlan {
    pub kind: Kind,
    pub id: Id,
    pub label: String,
    pub action: Action,
    /// The digest of the repo's copy.
    pub digest: String,
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
    let mut out = vec![];
    for kind in Kind::ALL {
        for item in repo.list(kind) {
            let Some(digest) = digest(&repo.root, kind, &item.id) else {
                continue;
            };
            let wrote = last.and_then(|r| r.item(kind, item.id.as_str()));
            out.push(ItemPlan {
                action: action(target, kind, &item.id, &digest, wrote),
                kind,
                id: item.id.clone(),
                label: item.label.clone(),
                digest,
            });
        }
    }
    out
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

/// Copies the planned items from `repo` into `target`, answering what the project now holds of
/// this import (written, replaced or already the same).
pub fn write(
    repo: &Path,
    target: &Path,
    plans: &[ItemPlan],
) -> Result<Vec<RecordedItem>, ImportError> {
    let mut held = vec![];
    for p in plans {
        match p.action {
            Action::Write | Action::Replace => copy_item(repo, target, p.kind, &p.id)?,
            Action::Same => {}
            Action::Keep(_) => continue,
        }
        held.push(RecordedItem {
            kind: kind_name(p.kind).to_string(),
            id: p.id.to_string(),
            digest: p.digest.clone(),
        });
    }
    Ok(held)
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
