//! Analytics across runs: the overview's figures (pass rate, cost, run time, a leaderboard), for
//! the Analytics page, the assistant and any agent over MCP. It reads as the caller.

use serde_json::{json, Value};

use super::{input, schema, Access, OpCtx, OpError, OpFuture, Operation};
use crate::analytics::Filter;

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "analytics.summary",
        area: "Analytics",
        summary: "Pass rate, cost, run time and tokens across runs: totals, pass rate over time and per setup or model, and a setup × scenario leaderboard, filtered by time, world, scenario, setup, model and status",
        access: Access::Read,
        method: "GET",
        path: "/api/analytics/summary",
        target: None,
        input_schema: schema::<Filter>,
        run: summary,
    }]
}

fn summary<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let filter: Filter = input(value)?;
        let q = filter.resolve().map_err(OpError::bad_request)?;
        let app = ctx.app("analytics")?;
        Ok(json!(
            app.store.at(&ctx.caller).analytics_summary(&q).await?
        ))
    })
}
