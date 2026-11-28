//! Reads every item file in a project folder. A file that cannot be read becomes a problem; it
//! never stops the rest of the project from loading.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::error::ProjectError;
use crate::format::Kind;
use crate::item::{Item, ItemBody, Problem};
use crate::store::Store;
use crate::tables::TableKinds;
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

/// Loads every item in `store` of every kind not kept as rows (`rows`).
pub(crate) fn load_all(
    store: &dyn Store,
    rows: TableKinds,
) -> Result<(Items, Vec<Problem>), ProjectError> {
    let mut items = Items::new();
    let mut problems = Vec::new();
    for kind in Kind::ALL.into_iter().filter(|k| !rows.has(*k)) {
        for name in names(store, kind, &mut problems)? {
            match read_item(store, kind, &name) {
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
    store: &dyn Store,
    kind: Kind,
    problems: &mut Vec<Problem>,
) -> Result<Vec<String>, ProjectError> {
    let Ok(entries) = store.list(kind.dir()) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let name = entry.name;
        let world_file = || store.is_file(&format!("{}/{name}/world.yaml", kind.dir()));
        match (kind, entry.dir, name.strip_suffix(".yaml")) {
            (Kind::World, true, _) if world_file() => names.push(name),
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

/// A problem with the whole of item `name` of `kind`.
fn problem(kind: Kind, name: &str, message: String) -> Problem {
    Problem {
        kind: Some(kind),
        id: Some(name.to_string()),
        pointer: String::new(),
        message,
    }
}

/// Reads one item, or the problem that stopped it.
pub(crate) fn read_item(store: &dyn Store, kind: Kind, name: &str) -> Result<Item, Problem> {
    let id = Id::parse(name).map_err(|e| problem(kind, name, e.to_string()))?;
    let file = item_file(kind, &id);
    let bytes = store
        .read(&file)
        .map_err(|e| problem(kind, name, e.to_string()))?;
    parse_item(kind, &id, &bytes)
}

/// Item `id` of `kind` from its file's bytes, or the problem that stops it.
pub(crate) fn parse_item(kind: Kind, id: &Id, bytes: &[u8]) -> Result<Item, Problem> {
    let file = item_file(kind, id);
    let text = std::str::from_utf8(bytes).map_err(|e| problem(kind, id.as_str(), e.to_string()))?;
    let body = ItemBody::parse(kind, &file, text).map_err(|e| Problem {
        pointer: e.pointer.clone(),
        ..problem(kind, id.as_str(), e.message)
    })?;
    let label = body.label().to_string();
    Ok(Item {
        kind,
        id: id.clone(),
        label,
        body,
        hash: hash(bytes),
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
