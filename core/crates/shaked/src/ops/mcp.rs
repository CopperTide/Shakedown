//! The control MCP server: every registry operation as a tool. Served at `POST /mcp` for agents
//! with an API token, and over stdio by `shaked mcp` for agents on this machine.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use super::{all, http::row, Access, OpCtx, Operation};
use crate::{
    app::AppState,
    auth::{Caller, Who},
    jsonrpc::{ok, rpc_error, tool_result},
};

/// The MCP protocol version this server speaks.
pub const PROTOCOL: &str = "2025-06-18";

/// JSON-RPC: unknown tool or bad arguments.
const INVALID_PARAMS: i64 = -32602;
/// JSON-RPC (server range): the caller may not call this tool.
const NOT_ALLOWED: i64 = -32001;

/// Who is calling: the person at this machine over stdio, or an authenticated HTTP caller.
pub enum Asker<'a> {
    /// `shaked mcp`: the process runs as the person at this machine.
    Local,
    /// An HTTP caller, checked per tool call like an API request.
    Caller(&'a Caller),
}

/// `POST /mcp`.
pub async fn serve(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Json(req): Json<Value>,
) -> Response {
    let ctx = OpCtx::as_caller(&state, caller.clone());
    match handle(&ctx, &Asker::Caller(&caller), &req).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// The reply to one JSON-RPC message; `None` for a notification.
pub async fn handle(ctx: &OpCtx<'_>, asker: &Asker<'_>, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned()?;
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    Some(
        match req.get("method").and_then(Value::as_str).unwrap_or("") {
            "initialize" => ok(id, initialize()),
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tools() })),
            "tools/call" => match call(ctx, asker, &params).await {
                Ok(result) => ok(id, result),
                Err((code, message)) => rpc_error(id, code, &message),
            },
            other => rpc_error(id, -32601, &format!("unknown method {other}")),
        },
    )
}

fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "shakedown", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// An operation's tool name: MCP clients reject dots, so `scenarios.put` is `scenarios_put`.
pub fn tool_name(op: &Operation) -> String {
    op.name.replace('.', "_")
}

/// The operation whose tool name is `name`.
pub fn by_tool(name: &str) -> Option<&'static Operation> {
    all().iter().find(|op| tool_name(op) == name)
}

fn tools() -> Vec<Value> {
    all()
        .iter()
        .map(|op| {
            json!({
                "name": tool_name(op),
                "description": op.summary,
                "inputSchema": (op.input_schema)(),
                "annotations": { "readOnlyHint": op.access == Access::Read },
            })
        })
        .collect()
}

async fn call(ctx: &OpCtx<'_>, asker: &Asker<'_>, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let op = by_tool(name).ok_or_else(|| (INVALID_PARAMS, format!("no tool named {name}")))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    allowed(ctx, asker, op, &args)
        .await
        .map_err(|m| (NOT_ALLOWED, m))?;
    let res = (op.run)(ctx, op, args).await.map_err(|e| e.message);
    Ok(tool_result(res))
}

/// Checks an HTTP caller exactly as the operation's HTTP route would be checked.
pub(crate) async fn allowed(
    ctx: &OpCtx<'_>,
    asker: &Asker<'_>,
    op: &Operation,
    args: &Value,
) -> Result<(), String> {
    let Asker::Caller(caller) = asker else {
        return Ok(());
    };
    let app = ctx.app.ok_or("this server cannot check callers")?;
    let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
    crate::api::routes::ask(app, caller, &row(op), id)
        .await
        .map_err(|e| e.message().to_string())
}
