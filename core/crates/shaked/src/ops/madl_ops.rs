//! MADL setups: the team file validated with the MADL crate, and drawn as agents and the
//! connections between them (patterns expanded).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use shakedown_project::format::{Kind, SetupFile, Team};
use shakedown_project::store::Store;
use shakedown_project::{Id, Project, ProjectError};

use super::{input, schema, unprocessable, Access, IdInput, OpCtx, OpError, OpFuture, Operation};

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "setups.madl",
        area: "Agent setups",
        summary:
            "A MADL setup's checks (code, pointer, message) and its graph of agents and connections",
        access: Access::Read,
        method: "GET",
        path: "/api/project/setups/{id}/madl",
        target: Some(Kind::Setup),
        input_schema: schema::<IdInput>,
        run: madl_of,
    }]
}

fn madl_of<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = Id::parse(&input::<IdInput>(value)?.id)?;
        let project = ctx.project.current().await?;
        let file = team_file(&project, &id)?;
        let (path, _copy) = on_disk(&**project.store(), &format!("setups/{id}"), &file)?;
        Ok(match madl::load(&path) {
            Ok(doc) => report(&doc),
            Err(madl::LoadError(d)) => {
                json!({ "valid": false, "diagnostics": [diagnostic(&d)], "graph": {"agents": [], "edges": []} })
            }
        })
    })
}

/// The MADL file of setup `id`, which must name one in its own folder: its path in the project.
fn team_file(project: &Project, id: &Id) -> Result<String, OpError> {
    let setup = project
        .body::<SetupFile>(id)
        .ok_or_else(|| ProjectError::not_found(Kind::Setup, id))?;
    let Team::Madl { file } = &setup.team else {
        return Err(unprocessable(format!("{id} isn't described in MADL")));
    };
    if file.contains('/') || file.contains('\\') || file.starts_with('.') {
        return Err(unprocessable(format!(
            "{file} must be a file name in the setup's folder"
        )));
    }
    let path = format!("setups/{id}/{file}");
    if !project.store().is_file(&path) {
        return Err(unprocessable(format!(
            "{file} is missing from the setup's folder"
        )));
    }
    Ok(path)
}

/// `file` on disk, so the `$ref`s between the files of `folder` resolve: in the project's own
/// folder, or for a project with none (kept in a database) in a temporary copy of `folder`,
/// removed when the copy is dropped.
fn on_disk(
    store: &dyn Store,
    folder: &str,
    file: &str,
) -> Result<(PathBuf, Option<tempfile::TempDir>), OpError> {
    if let Some(root) = store.folder() {
        return Ok((root.join(file), None));
    }
    let copy = tempfile::tempdir().map_err(|e| OpError::internal(e.into()))?;
    copy_folder(store, folder, copy.path()).map_err(|e| OpError::internal(e.into()))?;
    let path = copy.path().join(
        file.strip_prefix(folder)
            .unwrap_or(file)
            .trim_start_matches('/'),
    );
    Ok((path, Some(copy)))
}

/// Every file under `folder` in `store`, written under `to`.
fn copy_folder(store: &dyn Store, folder: &str, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in store.list(folder)? {
        let (from, target) = (format!("{folder}/{}", entry.name), to.join(&entry.name));
        if entry.dir {
            copy_folder(store, &from, &target)?;
        } else {
            std::fs::write(target, store.read(&from)?)?;
        }
    }
    Ok(())
}

fn report(doc: &madl::Document) -> Value {
    let checks = madl::validate(doc);
    let expanded = madl::expand(doc);
    let agents: Vec<Value> = expanded
        .agents
        .iter()
        .map(|a| json!({ "id": a.id, "label": a.name.clone().unwrap_or_else(|| a.id.clone()), "runtime": a.runtime }))
        .collect();
    let edges: Vec<Value> = madl::graph::edges(&expanded)
        .iter()
        .map(|e| json!({ "from": e.from, "to": e.to, "kind": serde_json::to_value(e.kind).unwrap_or(Value::Null) }))
        .collect();
    let diagnostics: Vec<Value> = checks.diagnostics.iter().map(diagnostic).collect();
    json!({ "valid": checks.is_ok(), "diagnostics": diagnostics, "graph": { "agents": agents, "edges": edges } })
}

fn diagnostic(d: &madl::Diagnostic) -> Value {
    let severity = format!("{:?}", d.severity).to_lowercase();
    json!({ "code": d.code, "severity": severity, "pointer": d.path, "message": d.message })
}
