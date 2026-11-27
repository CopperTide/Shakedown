//! Explore: measures × dimensions over runs and their steps, the runs or steps behind any cell,
//! read-only SQL over the explore views, and saved views, for the Analytics page's Explore tab, the
//! assistant and any agent over MCP. Queries read as the caller; saving a view is a write.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    input, project_ops::NoInput, schema, Access, IdInput, OpCtx, OpError, OpFn, OpFuture, Operation,
};
use crate::{
    analytics::explore::{catalog, docs, guard, spec::Spec, sql, SCHEMA},
    store::QUERY_TIMEOUT_SECS,
};

#[allow(clippy::too_many_arguments)]
fn op(
    name: &'static str,
    access: Access,
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    input_schema: fn() -> Value,
    run: OpFn,
) -> Operation {
    Operation {
        name,
        area: "Explore",
        summary,
        access,
        method,
        path,
        target: None,
        input_schema,
        run,
    }
}

pub(super) fn ops() -> Vec<Operation> {
    let mut ops = vec![
        op("explore.schema", Access::Read, "GET", "/api/explore/schema",
            "What Explore offers: the builder's measures and dimensions, the values its filters can take, and the SQL views (runs, steps, model_calls, tool_calls, agents) with their columns, the functions a query may call, its limits and example queries",
            schema::<NoInput>, describe),
        op("explore.query", Access::Read, "POST", "/api/explore/query",
            "Measures (runs, pass rate, cost, tokens in/out/cached, model and tool calls, refusals, errors, run time, thinking time, steps) by up to two dimensions (agent, model, setup, scenario, world, experiment, tool, step kind, day), filtered by time, experiment, setup and agent: a table, one row per group",
            schema::<Spec>, query),
        op("explore.drill", Access::Read, "POST", "/api/explore/drill",
            "The runs or steps behind one cell of an Explore query (its dimensions' values and measure), newest first, each with what the run page needs to open it",
            schema::<DrillInput>, drill),
        op("explore.sql", Access::Read, "POST", "/api/explore/sql",
            "Runs one read-only SELECT over the explore views (runs, steps, model_calls, tool_calls, agents; see explore.schema), as the caller, with a 5 s timeout and at most 5,000 rows",
            schema::<SqlInput>, run_sql),
    ];
    ops.extend(view_ops());
    ops
}

fn view_ops() -> Vec<Operation> {
    vec![
        op("explore.list", Access::Read, "GET", "/api/explore/views",
            "Saved Explore views (a builder query or an SQL query, with how to chart it), last changed first",
            schema::<NoInput>, list),
        op("explore.get", Access::Read, "GET", "/api/explore/views/{id}", "One saved Explore view", schema::<IdInput>, get),
        op("explore.save", Access::Edit, "POST", "/api/explore/views",
            "Saves an Explore view under a name: a builder spec (kind builder) or an SQL query (kind sql); with an id, replaces that view",
            schema::<SaveInput>, save),
        op("explore.delete", Access::Delete, "DELETE", "/api/explore/views/{id}", "Deletes a saved Explore view", schema::<IdInput>, delete),
    ]
}

/// One cell of a query's result.
#[derive(Deserialize, JsonSchema)]
struct DrillInput {
    /// The query whose cell this is (its filters apply).
    spec: Spec,
    /// The cell's dimension values by dimension id (null: none).
    #[serde(default)]
    cell: serde_json::Map<String, Value>,
    /// The cell's measure: for a step-level one, the steps it counts.
    #[serde(default)]
    measure: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SqlInput {
    /// One SELECT (or WITH … SELECT) over the explore views.
    sql: String,
}

#[derive(Deserialize, JsonSchema)]
struct SaveInput {
    /// The view to replace; none for a new one.
    #[serde(default)]
    id: Option<String>,
    /// 1 to 120 characters.
    name: String,
    /// `builder` or `sql`.
    kind: String,
    /// A builder spec (explore.query's input, plus how the page charts it), or `{sql, chart?}`.
    spec: Value,
}

fn describe<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        Ok(json!({
            "measures": catalog::MEASURES,
            "dimensions": catalog::DIMENSIONS,
            "options": store.explore_options().await?,
            "sql": {
                "schema": SCHEMA,
                "views": store.explore_views().await?,
                "functions": guard::FUNCTIONS,
                "limits": {"rows": sql::ROW_CAP, "bytes": sql::BYTE_CAP, "timeout_seconds": sql::TIMEOUT_SECS, "columns": sql::MAX_COLUMNS},
                "examples": docs::EXAMPLES,
            },
        }))
    })
}

fn db(e: anyhow::Error) -> OpError {
    sql::refused_after(e, QUERY_TIMEOUT_SECS)
}

fn query<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let plan = input::<Spec>(value)?
            .resolve()
            .map_err(OpError::bad_request)?;
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        Ok(json!(store.explore_query(&plan).await.map_err(db)?))
    })
}

/// A cell's dimensions with their values; an unknown dimension or a non-text value is 400.
fn cell_of(
    cell: serde_json::Map<String, Value>,
) -> Result<Vec<(&'static catalog::Dimension, Option<String>)>, OpError> {
    cell.into_iter()
        .map(|(id, v)| {
            let d = catalog::dimension(&id)
                .ok_or_else(|| OpError::bad_request(format!("no dimension {id:?}")))?;
            match v {
                Value::Null => Ok((d, None)),
                Value::String(s) => Ok((d, Some(s))),
                _ => Err(OpError::bad_request(format!(
                    "{id}: a dimension's value is text or null"
                ))),
            }
        })
        .collect()
}

fn drill<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let i: DrillInput = input(value)?;
        let plan = i.spec.resolve().map_err(OpError::bad_request)?;
        let measure = match i.measure.as_deref() {
            None => None,
            Some(m) => Some(
                catalog::measure(m)
                    .ok_or_else(|| OpError::bad_request(format!("no measure {m:?}")))?,
            ),
        };
        let cell = cell_of(i.cell)?;
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        Ok(json!(store
            .explore_drill(&plan, &cell, measure)
            .await
            .map_err(db)?))
    })
}

fn run_sql<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let i: SqlInput = input(value)?;
        let app = ctx.app("explore")?;
        Ok(json!(sql::run(&app.store, &ctx.caller, &i.sql).await?))
    })
}

fn list<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        Ok(json!({ "items": store.saved_views().await? }))
    })
}

fn missing(id: &str) -> OpError {
    OpError::not_found(format!("no saved view {id:?}"))
}

fn get<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = input::<IdInput>(value)?.id;
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        Ok(json!(store
            .saved_view(&id)
            .await?
            .ok_or_else(|| missing(&id))?))
    })
}

/// A view's spec, checked: a builder spec resolves, an SQL query passes the guard.
fn checked(kind: &str, spec: &Value) -> Result<(), OpError> {
    match kind {
        "builder" => serde_json::from_value::<Spec>(spec.clone())
            .map_err(|e| e.to_string())
            .and_then(Spec::resolve)
            .map(drop),
        "sql" => match spec.get("sql").and_then(Value::as_str) {
            Some(q) => guard::check(q).map(drop),
            None => Err("an SQL view's spec has its query in `sql`".into()),
        },
        k => Err(format!("kind is builder or sql, not {k:?}")),
    }
    .map_err(OpError::bad_request)
}

fn save<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let i: SaveInput = input(value)?;
        let name = i.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(OpError::bad_request("a view's name is 1 to 120 characters"));
        }
        checked(&i.kind, &i.spec)?;
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        let saved = store
            .save_view(i.id.as_deref(), name, &i.kind, &i.spec)
            .await?;
        Ok(json!(
            saved.ok_or_else(|| missing(i.id.as_deref().unwrap_or("")))?
        ))
    })
}

fn delete<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = input::<IdInput>(value)?.id;
        let store = ctx.app("explore")?.store.at(&ctx.caller);
        if !store.delete_view(&id).await? {
            return Err(missing(&id));
        }
        Ok(json!({ "deleted": true }))
    })
}
