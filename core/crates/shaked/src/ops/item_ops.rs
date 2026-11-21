//! List, get, put and delete for each kind of item, from one set of handlers. Edits follow the
//! project's permissions: with "show a diff first", a put returns the diff and writes nothing until
//! it is sent again with `confirm: true`.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::{
    EditPolicy, ExperimentFile, Kind, ScenarioFile, SetupFile, WorldFile,
};
use shakedown_project::{item_file, Id, ItemBody, Project};

use super::project_ops::NoInput;
use super::{input, schema, view, Access, OpError, OpFn, OpFuture, Operation};
use crate::app::AppState;

pub(super) fn ops() -> Vec<Operation> {
    Kind::ALL.iter().flat_map(|kind| for_kind(*kind)).collect()
}

/// The names, paths and schemas of one kind's operations.
struct KindOps {
    area: &'static str,
    names: [&'static str; 4],
    many: &'static str,
    one: &'static str,
    put_schema: fn() -> Value,
}

/// One kind's operations: its area, `<dir>.<verb>` names, paths and typed put schema.
macro_rules! kind_ops {
    ($area:literal, $dir:literal, $file:ty) => {
        KindOps {
            area: $area,
            names: [
                concat!($dir, ".list"),
                concat!($dir, ".get"),
                concat!($dir, ".put"),
                concat!($dir, ".delete"),
            ],
            many: concat!("/api/project/", $dir),
            one: concat!("/api/project/", $dir, "/{id}"),
            put_schema: schema::<PutSchema<$file>>,
        }
    };
}

fn spec(kind: Kind) -> KindOps {
    match kind {
        Kind::World => kind_ops!("Worlds", "worlds", WorldFile),
        Kind::Scenario => kind_ops!("Scenarios", "scenarios", ScenarioFile),
        Kind::Setup => kind_ops!("Agent setups", "setups", SetupFile),
        Kind::Experiment => kind_ops!("Experiments", "experiments", ExperimentFile),
    }
}

/// Summary, access and HTTP method of list, get, put and delete, in that order.
const VERBS: [(&str, Access, &str); 4] = [
    (
        "Lists them, with any problems in their files",
        Access::Read,
        "GET",
    ),
    ("One of them, with its contents", Access::Read, "GET"),
    (
        "Creates or replaces one; shows a diff first unless confirmed",
        Access::Edit,
        "PUT",
    ),
    ("Deletes one", Access::Delete, "DELETE"),
];

fn for_kind(kind: Kind) -> [Operation; 4] {
    let s = spec(kind);
    let runs: [OpFn; 4] = [list, get, put, delete];
    let schemas: [fn() -> Value; 4] = [
        schema::<NoInput>,
        schema::<IdInput>,
        s.put_schema,
        schema::<IdInput>,
    ];
    std::array::from_fn(|i| {
        let (summary, access, method) = VERBS[i];
        Operation {
            name: s.names[i],
            area: s.area,
            summary,
            access,
            method,
            path: if i == 0 { s.many } else { s.one },
            target: Some(kind),
            input_schema: schemas[i],
            run: runs[i],
        }
    })
}

/// Names one item.
#[derive(Deserialize, JsonSchema)]
struct IdInput {
    /// The item's id: lowercase letters, digits and dashes.
    id: String,
}

/// What a put takes; `body` is checked as the item's file format.
#[derive(Deserialize)]
struct PutInput {
    id: String,
    body: Value,
    #[serde(default)]
    confirm: bool,
}

/// The schema of a put's input, with the body typed as the kind's file format.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct PutSchema<B: JsonSchema> {
    /// The item's id: lowercase letters, digits and dashes.
    id: String,
    /// The item's contents.
    body: B,
    /// Write now, without showing a diff first.
    confirm: Option<bool>,
}

fn kind_of(op: &Operation) -> Result<Kind, OpError> {
    op.target
        .ok_or_else(|| OpError::bad_request("this operation acts on no kind of item"))
}

fn list<'a>(state: &'a AppState, op: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, project) = (kind_of(op)?, state.project()?);
        let items: Vec<Value> = project
            .list(kind)
            .into_iter()
            .map(|i| view::summary(&project, i))
            .collect();
        Ok(json!({ "items": items }))
    })
}

fn get<'a>(state: &'a AppState, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, id) = (kind_of(op)?, Id::parse(&input::<IdInput>(value)?.id)?);
        let project = state.project()?;
        let item = project.get(kind, &id).ok_or_else(|| not_found(kind, &id))?;
        Ok(view::item(&project, item))
    })
}

fn put<'a>(state: &'a AppState, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let kind = kind_of(op)?;
        let PutInput { id, body, confirm } = input(value)?;
        let id = Id::parse(&id)?;
        let body = parse_body(kind, &id, &body)?;
        let project = state.project()?;
        let diff = diff_against(&project, kind, &id, &body)?;
        if project.config.permissions.edit == EditPolicy::DiffFirst && !confirm {
            return Ok(json!({ "proposed": true, "diff": diff }));
        }
        let item = state.with_project_mut(|p| p.put(kind, &id, body))??;
        let mut out = view::item(&state.project()?, &item);
        out["diff"] = Value::String(diff);
        Ok(out)
    })
}

fn delete<'a>(state: &'a AppState, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, id) = (kind_of(op)?, Id::parse(&input::<IdInput>(value)?.id)?);
        state.with_project_mut(|p| p.remove(kind, &id))??;
        Ok(json!({ "deleted": id.as_str() }))
    })
}

/// Checks `body` exactly as the file would be checked when read back.
fn parse_body(kind: Kind, id: &Id, body: &Value) -> Result<ItemBody, OpError> {
    let yaml = serde_yaml_ng::to_string(body).map_err(|e| OpError::bad_request(e.to_string()))?;
    Ok(ItemBody::parse(kind, &item_file(kind, id), &yaml)?)
}

/// The change `body` makes to the item as it is now, as a unified diff of its YAML.
fn diff_against(
    project: &Project,
    kind: Kind,
    id: &Id,
    body: &ItemBody,
) -> Result<String, OpError> {
    let old = match project.get(kind, id) {
        Some(item) => item.body.to_yaml()?,
        None => String::new(),
    };
    let new = body.to_yaml()?;
    Ok(similar::TextDiff::from_lines(&old, &new)
        .unified_diff()
        .to_string())
}

fn not_found(kind: Kind, id: &Id) -> OpError {
    shakedown_project::ProjectError::NotFound {
        kind: shakedown_project::kind_name(kind),
        id: id.to_string(),
    }
    .into()
}
