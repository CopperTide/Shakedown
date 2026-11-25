//! List, get, put, delete and dependents for each kind of item, from one set of handlers. Edits
//! follow the project's permissions: with "show a diff first", a put returns the diff and writes
//! nothing until it is sent again with `confirm: true`. A delete is refused while anything uses
//! the item, unless sent with `cascade: true`.

use std::ops::ControlFlow;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::{ExperimentFile, Kind, ScenarioFile, SetupFile, WorldFile};
use shakedown_project::{item_file, Id, ItemBody, ProjectError};

use super::delete_ops;
use super::project_ops::NoInput;
use super::{
    as_yaml, diff_first, flag, input, schema, view, Access, IdInput, OpCtx, OpError, OpFn,
    OpFuture, Operation,
};

pub(super) fn ops() -> Vec<Operation> {
    Kind::ALL.iter().flat_map(|kind| for_kind(*kind)).collect()
}

/// The names, paths and schemas of one kind's operations.
struct KindOps {
    area: &'static str,
    names: [&'static str; 5],
    paths: [&'static str; 5],
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
                concat!($dir, ".dependents"),
            ],
            paths: [
                concat!("/api/project/", $dir),
                concat!("/api/project/", $dir, "/{id}"),
                concat!("/api/project/", $dir, "/{id}"),
                concat!("/api/project/", $dir, "/{id}"),
                concat!("/api/project/", $dir, "/{id}/dependents"),
            ],
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

/// Summary, access and HTTP method of list, get, put, delete and dependents, in that order.
const VERBS: [(&str, Access, &str); 5] = [
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
    (
        "Deletes one; refused while anything uses it unless sent with cascade",
        Access::Delete,
        "DELETE",
    ),
    (
        "What uses one, and what deleting it with cascade does to each",
        Access::Read,
        "GET",
    ),
];

fn for_kind(kind: Kind) -> [Operation; 5] {
    let s = spec(kind);
    let runs: [OpFn; 5] = [list, get, put, delete_ops::delete, delete_ops::dependents];
    let schemas: [fn() -> Value; 5] = [
        schema::<NoInput>,
        schema::<IdInput>,
        s.put_schema,
        schema::<delete_ops::DeleteInput>,
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
            path: s.paths[i],
            target: Some(kind),
            input_schema: schemas[i],
            run: runs[i],
        }
    })
}

/// What a put takes; `body` is checked as the item's file format.
#[derive(Deserialize)]
struct PutInput {
    id: String,
    body: Value,
    #[serde(default, deserialize_with = "flag")]
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

pub(super) fn kind_of(op: &Operation) -> Result<Kind, OpError> {
    op.target
        .ok_or_else(|| OpError::bad_request("this operation acts on no kind of item"))
}

fn list<'a>(ctx: &'a OpCtx<'a>, op: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, project) = (kind_of(op)?, ctx.project.current().await?);
        let items: Vec<Value> = project
            .list(kind)
            .into_iter()
            .map(|i| view::summary(&project, i))
            .collect();
        Ok(json!({ "items": items }))
    })
}

fn get<'a>(ctx: &'a OpCtx<'a>, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, id) = (kind_of(op)?, Id::parse(&input::<IdInput>(value)?.id)?);
        let project = ctx.project.current().await?;
        let item = project
            .get(kind, &id)
            .ok_or_else(|| ProjectError::not_found(kind, &id))?;
        Ok(view::item(&project, item))
    })
}

fn put<'a>(ctx: &'a OpCtx<'a>, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let kind = kind_of(op)?;
        let PutInput { id, body, confirm } = input(value)?;
        let id = Id::parse(&id)?;
        // Checked exactly as the file would be checked when read back.
        let body = ItemBody::parse(kind, &item_file(kind, &id), &as_yaml(&body)?)?;
        let project = ctx.project.current().await?;
        let old = match project.get(kind, &id) {
            Some(item) => item.body.to_yaml()?,
            None => String::new(),
        };
        let policy = project.config.permissions.edit;
        let diff = match diff_first(policy, &old, &body.to_yaml()?, confirm) {
            ControlFlow::Continue(diff) => diff,
            ControlFlow::Break(proposal) => return Ok(proposal),
        };
        let item = ctx.project.change(|p| p.put(kind, &id, body)).await??;
        let mut out = view::item(&ctx.project.current().await?, &item);
        out["diff"] = Value::String(diff);
        Ok(out)
    })
}
