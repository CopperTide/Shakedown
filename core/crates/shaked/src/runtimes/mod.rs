//! Runtimes: the packages that run worlds, as the Runtimes page lists and manages them. A runtime
//! is installed from a git repo (or a folder of it) or an uploaded archive by the import
//! machinery with packages only (`crate::git_import`, `Request::packages_only`), then checked
//! (`check`). Packages in `SHAKEDOWN_PACKAGES` folders are listed as built in.

pub mod check;

use std::collections::BTreeSet;
use std::path::Path;

use parts::{Hub, Manifest};
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::{ItemBody, Project};

use crate::git_import::record::{self, imports_dir, Record};
use crate::git_import::{package_kind, ImportError};

/// Every runtime: the installs recorded in the project at `root`, each with its packages, and
/// the packages installed from folders. `project` names the worlds that use each package.
pub fn list(hub: &Hub, root: Option<&Path>, project: Option<&Project>) -> Value {
    let installs = root.map(record::all).unwrap_or_default();
    let mut theirs = BTreeSet::new();
    let installs: Vec<Value> = (installs.iter())
        .map(|(dir, rec)| {
            theirs.extend(rec.packages.iter().map(|p| p.name.clone()));
            install_json(hub, dir, rec, project)
        })
        .collect();
    let registry = hub.registry();
    let builtin: Vec<Value> = (registry.benchmarks())
        .chain(registry.integrations())
        .chain(registry.builders())
        .filter(|m| !theirs.contains(&m.name))
        .map(|m| package_json(m, project))
        .collect();
    json!({"installs": installs, "builtin": builtin})
}

fn install_json(hub: &Hub, dir: &Path, rec: &Record, project: Option<&Project>) -> Value {
    let checks = check::read(dir, rec);
    let packages: Vec<Value> = (rec.packages.iter())
        .map(|p| {
            let registered = hub.registry().get(&p.name).cloned();
            let mut row = match &registered {
                Some(m) => package_json(m, project),
                None => {
                    json!({"name": p.name, "kind": p.kind, "used_by": used_by(project, &[&p.name])})
                }
            };
            row["registered"] = json!(registered.is_some());
            row["check"] = checks.get(&p.name).cloned().unwrap_or(Value::Null);
            row
        })
        .collect();
    json!({
        "id": rec.id, "source": rec.source, "url": rec.url, "ref": rec.git_ref,
        "path": rec.packages_only, "commit": rec.commit, "installed_at": rec.imported_at,
        "runtime": rec.packages_only.is_some(), "items": rec.items.len(), "packages": packages,
    })
}

fn package_json(m: &Manifest, project: Option<&Project>) -> Value {
    json!({
        "name": m.name, "kind": package_kind(m.kind), "version": m.version,
        "dir": m.dir.display().to_string(), "used_by": used_by(project, &[&m.name]),
    })
}

/// The ids of the project's worlds whose runtime is one of `packages`.
pub fn used_by(project: Option<&Project>, packages: &[&str]) -> Vec<String> {
    let Some(project) = project else {
        return vec![];
    };
    (project.list(Kind::World).into_iter())
        .filter(|item| match &item.body {
            ItemBody::World(w) => w.package.as_deref().is_some_and(|p| packages.contains(&p)),
            _ => false,
        })
        .map(|item| item.id.to_string())
        .collect()
}

/// Removes the runtime `id` installed in the project at `root`: unregistered, its checkout and
/// environments deleted. Refused for one a world uses (naming the worlds) and for an import
/// that wrote project files (Import from git manages those).
pub async fn remove(
    hub: &Hub,
    root: &Path,
    project: Option<&Project>,
    id: &str,
) -> Result<Value, ImportError> {
    let dir = imports_dir(root).join(id);
    let rec = (record::read(&dir).filter(|_| shakedown_project::Id::parse(id).is_ok()))
        .ok_or_else(|| ImportError::NotFound(format!("no runtime {id} is installed")))?;
    if rec.packages_only.is_none() {
        return Err(ImportError::Conflict(format!(
            "{id} was imported from git with its project files: it is managed by Import from git"
        )));
    }
    let names: Vec<&str> = rec.packages.iter().map(|p| p.name.as_str()).collect();
    let users = used_by(project, &names);
    if !users.is_empty() {
        return Err(ImportError::Conflict(format!(
            "{id} runs the worlds {}: give them another runtime first",
            users.join(", ")
        )));
    }
    (hub.replace_source(id, "", vec![]).await)
        .map_err(|e| ImportError::Failed(format!("{e:#}")))?;
    std::fs::remove_dir_all(&dir).map_err(|e| ImportError::Io(format!("removing {id}: {e}")))?;
    Ok(json!({"removed": id, "packages": names}))
}
