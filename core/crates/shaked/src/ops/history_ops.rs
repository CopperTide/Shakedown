//! Project history and file interchange, shared by HTTP and MCP.

use super::{input, schema, view, Access, OpCtx, OpError, OpFuture, Operation};
use base64::{engine::general_purpose::STANDARD, Engine};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::{format::Kind, Id};
use std::collections::BTreeMap;

const HISTORY: [(&str, &str, Option<Kind>); 5] = [
    ("project.history", "/api/project/history", None),
    (
        "worlds.history",
        "/api/project/worlds/{id}/history",
        Some(Kind::World),
    ),
    (
        "scenarios.history",
        "/api/project/scenarios/{id}/history",
        Some(Kind::Scenario),
    ),
    (
        "setups.history",
        "/api/project/setups/{id}/history",
        Some(Kind::Setup),
    ),
    (
        "experiments.history",
        "/api/project/experiments/{id}/history",
        Some(Kind::Experiment),
    ),
];

pub(super) fn ops() -> Vec<Operation> {
    let mut ops: Vec<_> = HISTORY
        .into_iter()
        .map(|(name, path, target)| Operation {
            name,
            area: "Project",
            summary: "Recent changes, newest first",
            access: Access::Edit,
            method: "GET",
            path,
            target,
            input_schema: schema::<HistoryInput>,
            run: history,
        })
        .collect();
    ops.extend([export_op(), import_op()]);
    ops
}

fn export_op() -> Operation {
    Operation {
        name: "project.export",
        area: "Project",
        summary: "Export project files as base64 bytes",
        access: Access::Spend,
        method: "GET",
        path: "/api/project/export",
        target: None,
        input_schema: schema::<super::project_ops::NoInput>,
        run: export,
    }
}

fn import_op() -> Operation {
    Operation {
        name: "project.import_files",
        area: "Project",
        summary: "Preview or import project files encoded as base64",
        access: Access::Edit,
        method: "POST",
        path: "/api/project/import-files",
        target: None,
        input_schema: schema::<ImportInput>,
        run: import,
    }
}

#[derive(Deserialize, JsonSchema)]
struct HistoryInput {
    id: Option<String>,
    #[serde(default, deserialize_with = "read_limit")]
    limit: Option<i64>,
}

fn history<'a>(ctx: &'a OpCtx<'a>, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let args: HistoryInput = input(value)?;
        let filter = match op.target {
            Some(kind) => Some((kind, Id::parse(args.id.as_deref().unwrap_or(""))?)),
            None => None,
        };
        let changes = ctx.project.history(args.limit.unwrap_or(50)).await?;
        let changes: Vec<_> = changes
            .iter()
            .filter(|c| {
                filter.as_ref().is_none_or(|(k, id)| {
                    c.items
                        .iter()
                        .any(|i| i.file_name() == shakedown_project::item_file(*k, id))
                })
            })
            .map(|c| {
                json!({"id": c.id, "label": c.label, "created_at": c.created_at,
            "undone_at": c.undone_at, "items": c.items})
            })
            .collect();
        Ok(json!({"changes": changes}))
    })
}

fn export<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let files = ctx.project.current().await?.export()?;
        let files: BTreeMap<_, _> = files.iter().map(|(p, b)| (p, STANDARD.encode(b))).collect();
        Ok(json!({"files": files, "encoding": "base64"}))
    })
}

#[derive(Deserialize, JsonSchema)]
struct ImportInput {
    /// Relative project paths mapped to base64-encoded file bytes.
    files: BTreeMap<String, String>,
    #[serde(default)]
    confirm: bool,
}

fn import<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let args: ImportInput = input(value)?;
        let mut files = shakedown_project::store::Files::new();
        for (path, encoded) in args.files {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| OpError::bad_request(format!("invalid base64: {path}")))?;
            files.insert(path, bytes.into());
        }
        let plan = ctx.project.current().await?.plan_import(&files);
        let problems: Vec<_> = plan.problems.iter().map(view::problem).collect();
        let items: Vec<_> = plan
            .items
            .iter()
            .map(|i| json!({"kind": shakedown_project::kind_name(i.kind), "id": i.id.as_str()}))
            .collect();
        if args.confirm {
            ctx.project.change(|p| p.import_files(&plan)).await??;
        }
        Ok(
            json!({"proposed": !args.confirm, "items": items, "problems": problems, "skipped": plan.skipped}),
        )
    })
}

fn read_limit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<Value>::deserialize(d)?
        .map(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| serde::de::Error::custom("limit must be an integer"))
        })
        .transpose()
}
