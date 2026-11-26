//! Analytics across runs: the overview's figures (pass rate, cost, run time, a leaderboard) and
//! search over every run's steps, for the Analytics page, the assistant and any agent over MCP.
//! Both read as the caller.

use serde_json::{json, Value};

use super::{input, schema, Access, OpCtx, OpError, OpFuture, Operation};
use crate::analytics::{Filter, SearchInput};

pub(super) fn ops() -> Vec<Operation> {
    vec![
        Operation {
            name: "analytics.summary",
            area: "Analytics",
            summary: "Pass rate, cost, run time and tokens across runs: totals, pass rate over time and per setup or model, and a setup × scenario leaderboard, filtered by time, world, scenario, setup, model and status",
            access: Access::Read,
            method: "GET",
            path: "/api/analytics/summary",
            target: None,
            input_schema: schema::<Filter>,
            run: summary,
        },
        Operation {
            name: "analytics.search",
            area: "Analytics",
            summary: "Finds steps across every run by their text (reasoning, messages, delegations and reports, tool calls' names, arguments, results and errors, checks), newest first, each with its run, branch, step number, agent and a snippet of the match; filtered by kind, agent, run and experiment, a page at a time",
            access: Access::Read,
            method: "GET",
            path: "/api/analytics/search",
            target: None,
            input_schema: schema::<SearchInput>,
            run: search,
        },
    ]
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

fn search<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let input: SearchInput = input(value)?;
        let q = input.resolve().map_err(OpError::bad_request)?;
        let app = ctx.app("search")?;
        Ok(json!(app.store.at(&ctx.caller).search_steps(&q).await?))
    })
}
