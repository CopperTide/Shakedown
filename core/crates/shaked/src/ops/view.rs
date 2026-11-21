//! How items and problems are shown to callers. Responses carry ids and names, never paths.

use serde_json::{json, Value};
use shakedown_project::{kind_name, Item, ItemBody, Problem, Project};

/// An item with its body and its problems.
pub(super) fn item(project: &Project, item: &Item) -> Value {
    let mut out = summary(project, item);
    out["body"] = body(&item.body);
    out
}

/// An item without its body, for lists.
pub(super) fn summary(project: &Project, item: &Item) -> Value {
    let problems: Vec<Value> = project
        .problems_of(item.kind, &item.id)
        .into_iter()
        .map(problem)
        .collect();
    json!({
        "kind": kind_name(item.kind),
        "id": item.id.as_str(),
        "label": item.label,
        "hash": item.hash,
        "problems": problems,
    })
}

/// A problem: which item, where in it, and what is wrong.
pub(super) fn problem(p: &Problem) -> Value {
    json!({
        "kind": p.kind.map(kind_name),
        "id": p.id,
        "pointer": p.pointer,
        "message": p.message,
    })
}

fn body(body: &ItemBody) -> Value {
    let value = match body {
        ItemBody::World(b) => serde_json::to_value(b),
        ItemBody::Scenario(b) => serde_json::to_value(b),
        ItemBody::Setup(b) => serde_json::to_value(b),
        ItemBody::Experiment(b) => serde_json::to_value(b),
    };
    value.unwrap_or(Value::Null)
}
