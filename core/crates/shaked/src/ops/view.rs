//! How items and problems are shown to callers. Responses carry ids and names, never paths.

use serde_json::{json, Value};
use shakedown_project::{kind_name, Item, Problem, Project};

/// An item with its body and its problems.
pub(super) fn item(project: &Project, item: &Item) -> Value {
    let mut out = summary(project, item);
    out["body"] = serde_json::to_value(&item.body).unwrap_or(Value::Null);
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
