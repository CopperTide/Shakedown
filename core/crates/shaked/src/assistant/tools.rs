//! The registry's operations as model tools, and what happens to each call the model makes.

use serde::Serialize;
use serde_json::{json, Value};

use super::{ChatAnswer, ToolUse};
use crate::ops::{self, mcp, Access, OpCtx, Operation};

/// Something the person decides: apply a proposed edit, or confirm a run, spend or delete.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Card {
    /// A proposed edit, applied by sending `arguments` with `confirm: true` to the tool's route.
    Diff {
        tool: String,
        arguments: Value,
        diff: String,
    },
    /// An operation that starts runs, costs money or deletes, run by the person's click.
    Confirm {
        tool: String,
        arguments: Value,
        access: &'static str,
    },
}

/// Every operation as an OpenAI-style function tool.
pub(super) fn specs() -> Vec<Value> {
    ops::all()
        .iter()
        .map(|op| json!({"type": "function", "function": {
            "name": mcp::tool_name(op), "description": op.summary, "parameters": (op.input_schema)() }}))
        .collect()
}

/// Runs, proposes or holds one tool call; the tool message for the model.
pub(super) async fn dispatch(ctx: &OpCtx<'_>, call: &Value, answer: &mut ChatAnswer) -> Value {
    let id = call["id"].clone();
    let name = call["function"]["name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let args: Value = call["function"]["arguments"]
        .as_str()
        .and_then(|a| serde_json::from_str(a).ok())
        .unwrap_or_else(|| json!({}));
    let content = match mcp::by_tool(&name) {
        None => format!("no tool named {name}"),
        Some(op) => outcome(ctx, op, args, answer).await,
    };
    json!({ "role": "tool", "tool_call_id": id, "content": content })
}

async fn outcome(
    ctx: &OpCtx<'_>,
    op: &'static Operation,
    args: Value,
    answer: &mut ChatAnswer,
) -> String {
    if let Err(why) = mcp::allowed(ctx, &mcp::Asker::Caller(&ctx.caller), op, &args).await {
        return format!("not allowed: {why}");
    }
    answer.tools.push(ToolUse {
        name: mcp::tool_name(op),
        summary: op.summary.to_string(),
    });
    match op.access {
        Access::Read => run(ctx, op, args).await,
        Access::Edit if takes_confirm(op) => propose(ctx, op, args, answer).await,
        _ => hold(op, args, answer),
    }
}

async fn run(ctx: &OpCtx<'_>, op: &'static Operation, args: Value) -> String {
    match (op.run)(ctx, op, args).await {
        Ok(v) => v.to_string(),
        Err(e) => format!("failed: {}", e.message),
    }
}

/// Whether an operation shows what it would change until sent with `confirm: true`.
fn takes_confirm(op: &Operation) -> bool {
    (op.input_schema)()["properties"].get("confirm").is_some()
}

/// An edit: written only if the project allows it without asking, else a diff card.
async fn propose(
    ctx: &OpCtx<'_>,
    op: &'static Operation,
    mut args: Value,
    answer: &mut ChatAnswer,
) -> String {
    args["confirm"] = json!(false);
    match (op.run)(ctx, op, args.clone()).await {
        Ok(v) if v["proposed"] == true => {
            if let Some(map) = args.as_object_mut() {
                map.remove("confirm");
                // What the proposal is pinned to (a restore's delete), so Apply does that.
                if let Some(trash) = v.get("trash") {
                    map.insert("trash".into(), trash.clone());
                }
            }
            let diff = match v["diff"].as_str() {
                Some(diff) => diff.to_string(),
                None => serde_json::to_string_pretty(&v).unwrap_or_default(),
            };
            answer.cards.push(Card::Diff {
                tool: mcp::tool_name(op),
                arguments: args,
                diff,
            });
            "Proposed this change; the person sees the diff and can apply it.".into()
        }
        Ok(v) => format!("Written: {v}"),
        Err(e) => format!("failed: {}", e.message),
    }
}

/// Runs, spending or deleting: never run here; the person confirms the card. A delete card
/// carries only what to delete: the person picks alone or with what uses it, in the card.
fn hold(op: &'static Operation, mut args: Value, answer: &mut ChatAnswer) -> String {
    if let (Access::Delete, Some(map)) = (op.access, args.as_object_mut()) {
        for chosen in ["cascade", "expect", "confirm"] {
            map.remove(chosen);
        }
    }
    let access = match op.access {
        Access::Spend => "spend",
        Access::Delete => "delete",
        _ => "edit",
    };
    answer.cards.push(Card::Confirm {
        tool: mcp::tool_name(op),
        arguments: args,
        access,
    });
    "Needs the person's OK; shown to them as a card to confirm.".into()
}
