//! MADL setups: the team file validated with the MADL crate, and drawn as agents and the
//! connections between them (patterns expanded).

use axum::http::StatusCode;
use serde_json::{json, Value};
use shakedown_project::format::{Kind, Team};
use shakedown_project::{Id, ItemBody, Project};

use super::{input, schema, Access, IdInput, OpCtx, OpError, OpFuture, Operation};

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

fn unprocessable(message: String) -> OpError {
    OpError::new(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn madl_of<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = Id::parse(&input::<IdInput>(value)?.id)?;
        let project = ctx.project.current()?;
        let path = team_file(&project, &id)?;
        Ok(match madl::load(&path) {
            Ok(doc) => report(&doc),
            Err(madl::LoadError(d)) => {
                json!({ "valid": false, "diagnostics": [diagnostic(&d)], "graph": {"agents": [], "edges": []} })
            }
        })
    })
}

/// The MADL file of setup `id`, which must name one in its own folder.
fn team_file(project: &Project, id: &Id) -> Result<std::path::PathBuf, OpError> {
    let file = match project.get(Kind::Setup, id).map(|i| &i.body) {
        Some(ItemBody::Setup(s)) => match &s.team {
            Team::Madl { file } => file.clone(),
            _ => return Err(unprocessable(format!("{id} isn't described in MADL"))),
        },
        _ => {
            return Err(OpError::new(
                StatusCode::NOT_FOUND,
                format!("no setup named {id}"),
            ))
        }
    };
    if file.contains('/') || file.contains('\\') || file.starts_with('.') {
        return Err(unprocessable(format!(
            "{file} must be a file name in the setup's folder"
        )));
    }
    let path = project.root.join("setups").join(id.as_str()).join(&file);
    if !path.is_file() {
        return Err(unprocessable(format!(
            "{file} is missing from the setup's folder"
        )));
    }
    Ok(path)
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
