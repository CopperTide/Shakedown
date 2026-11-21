//! Reads every item file in a project folder. A file that cannot be read becomes a problem; it
//! never stops the rest of the project from loading.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::ProjectError;
use crate::format::Kind;
use crate::item::{Item, ItemBody, Problem};
use crate::Id;

/// Items keyed by kind and id.
pub(crate) type Items = BTreeMap<(Kind, Id), Item>;

/// The file of item `id` of `kind`, relative to the project folder.
pub fn item_file(kind: Kind, id: &Id) -> String {
    match kind {
        Kind::World => format!("worlds/{id}/world.yaml"),
        _ => format!("{}/{id}.yaml", kind.dir()),
    }
}

/// Loads every item of every kind under `root`.
pub(crate) fn load_all(root: &Path) -> Result<(Items, Vec<Problem>), ProjectError> {
    let mut items = Items::new();
    let mut problems = Vec::new();
    for kind in Kind::ALL {
        for name in names(root, kind, &mut problems)? {
            match read_item(root, kind, &name) {
                Ok(item) => {
                    items.insert((kind, item.id.clone()), item);
                }
                Err(problem) => problems.push(problem),
            }
        }
    }
    Ok((items, problems))
}

/// The item names found for `kind`, reporting loose world files as problems.
fn names(
    root: &Path,
    kind: Kind,
    problems: &mut Vec<Problem>,
) -> Result<Vec<String>, ProjectError> {
    let dir = root.join(kind.dir());
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| io("reading", kind.dir(), source))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.path().is_dir();
        match (kind, is_dir, name.strip_suffix(".yaml")) {
            (Kind::World, true, _) if entry.path().join("world.yaml").is_file() => names.push(name),
            (Kind::World, false, Some(stem)) => problems.push(loose_world(stem)),
            (Kind::World, _, _) => {}
            (_, false, Some(stem)) => names.push(stem.to_string()),
            _ => {}
        }
    }
    names.sort();
    Ok(names)
}

fn loose_world(stem: &str) -> Problem {
    Problem {
        kind: Some(Kind::World),
        id: Some(stem.to_string()),
        pointer: String::new(),
        message: format!("worlds are folders: move this file to a folder named {stem}"),
    }
}

/// Reads one item, or the problem that stopped it.
pub(crate) fn read_item(root: &Path, kind: Kind, name: &str) -> Result<Item, Problem> {
    let problem = |message: String| Problem {
        kind: Some(kind),
        id: Some(name.to_string()),
        pointer: String::new(),
        message,
    };
    let id = Id::parse(name).map_err(|e| problem(e.to_string()))?;
    let file = item_file(kind, &id);
    let bytes = std::fs::read(root.join(&file)).map_err(|e| problem(e.to_string()))?;
    let text = String::from_utf8(bytes.clone()).map_err(|e| problem(e.to_string()))?;
    let body = ItemBody::parse(kind, &file, &text).map_err(|e| Problem {
        pointer: e.pointer.clone(),
        ..problem(e.message)
    })?;
    let label = body.label().to_string();
    Ok(Item {
        kind,
        id,
        label,
        body,
        hash: hash(&bytes),
    })
}

/// SHA-256 of `bytes`, hex-encoded.
pub(crate) fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn io(action: &'static str, file: &str, source: std::io::Error) -> ProjectError {
    ProjectError::Io {
        action,
        file: file.to_string(),
        source,
    }
}
