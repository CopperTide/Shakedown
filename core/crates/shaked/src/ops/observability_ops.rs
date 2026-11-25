//! Exporting a run to its observability targets (Langfuse, Arize Phoenix, an OTLP collector): the
//! same as `POST /api/branches/{id}/export`, for the assistant and any agent over MCP.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{input, schema, Access, OpCtx, OpError, OpFuture, Operation};

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "runs.export",
        area: "Runs",
        summary:
            "Sends a run (one branch) to the observability targets now, or queues it until it ends",
        access: Access::Spend,
        method: "POST",
        path: "/api/runs/export",
        target: None,
        input_schema: schema::<ExportInput>,
        run: export,
    }]
}

/// What an export takes.
#[derive(Deserialize, JsonSchema)]
struct ExportInput {
    /// The branch to send (a run's first branch, or a fork).
    branch_id: String,
    /// One target's id (a UUID); every target when missing.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    target_id: Option<uuid::Uuid>,
}

fn export<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let ExportInput {
            branch_id,
            target_id,
        } = input(value)?;
        let app = ctx.app("exporting a run")?;
        let done = crate::otel::exporter::export_now(app, &ctx.caller, &branch_id, target_id);
        let results = done.await?;
        let results =
            results.ok_or_else(|| OpError::not_found(format!("no branch {branch_id}")))?;
        Ok(json!({ "branch_id": branch_id, "exports": results }))
    })
}
