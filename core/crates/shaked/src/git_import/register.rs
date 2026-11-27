//! Registering a project's recorded imports with the hub, and listing them.

use std::path::Path;

use parts::{Hub, Manifest};
use serde_json::{json, Value};

use super::apply::program_env;
use super::record::{self, Record};

/// Registers with `hub` every import recorded in the project at `project` that it does not
/// hold at its recorded commit; answers what could not be registered.
pub async fn register(hub: &Hub, project: &Path) -> Vec<String> {
    let mut errors = vec![];
    for (dir, rec) in record::all(project) {
        if hub.source_version(&rec.id).as_deref() == Some(rec.commit.as_str()) {
            continue;
        }
        let registered = match manifests(&dir, &rec) {
            Ok(ms) => {
                (hub.replace_source(&rec.id, &rec.commit, ms).await).map_err(|e| format!("{e:#}"))
            }
            Err(e) => Err(e),
        };
        if let Err(e) = registered {
            errors.push(format!("import {}: {e}", rec.id));
        }
    }
    errors
}

/// The recorded packages of the import in `dir`, read from its checkout, environments set.
fn manifests(dir: &Path, rec: &Record) -> Result<Vec<Manifest>, String> {
    let checkout = rec.checkout(dir);
    (rec.packages.iter())
        .map(|p| {
            let mut m =
                Manifest::load(&checkout.join(&p.dir)).map_err(|e| format!("{}: {e:#}", p.dir))?;
            m.env = program_env(dir, p.env.as_deref());
            Ok(m)
        })
        .collect()
}

/// The imports recorded in the project at `project`, for its status.
pub fn listed(project: &Path) -> Vec<Value> {
    record::all(project)
        .into_iter()
        .map(|(_, r)| {
            json!({"id": r.id, "source": r.source, "url": r.url, "ref": r.git_ref, "commit": r.commit,
                   "imported_at": r.imported_at, "packages": r.packages.len(), "items": r.items.len()})
        })
        .collect()
}
