//! How items and problems are shown to callers. Responses carry ids, names and paths in the
//! project, never absolute paths.

use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::{kind_name, Dependent, Fate, Item, ItemBody, Problem, Project};

/// An item with its body and its problems.
pub(super) fn item(project: &Project, item: &Item) -> Value {
    let mut out = summary(project, item);
    if let Some(revision) = project.revision(item.kind, &item.id) {
        out["revision"] = json!(revision);
    }
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
    let mut out = json!({
        "kind": kind_name(item.kind),
        "id": item.id.as_str(),
        "label": item.label,
        "hash": item.hash,
        "problems": problems,
    });
    if let (Value::Object(out), Value::Object(place)) = (&mut out, placement(&item.body)) {
        out.extend(place);
    }
    out
}

/// Where an item belongs, for lists to group and nest by: a world's group, a scenario's world,
/// an experiment's world, scenario and parent (and how many setups it compares).
fn placement(body: &ItemBody) -> Value {
    match body {
        ItemBody::World(w) => json!({ "group": w.group }),
        ItemBody::Scenario(s) => json!({ "world": s.world }),
        ItemBody::Experiment(e) => json!({
            "world": e.world, "scenario": e.scenario, "parent": e.parent, "setups": e.setups.len(),
        }),
        ItemBody::Setup(_) => json!({}),
    }
}

/// The items that use another, each with what a cascading delete does to it: every `fates` that
/// applies, and the first as `fate`.
pub(super) fn dependents(by: &[Dependent]) -> Value {
    let name = |f: &Fate| FATES.iter().find(|(g, _)| g == f).map(|(_, n)| *n);
    let one = |d: &Dependent| {
        let fates: Vec<_> = d.fates.iter().map(name).collect();
        json!({ "kind": kind_name(d.kind), "id": d.id.as_str(), "label": d.label,
                "fate": fates.first(), "fates": fates })
    };
    Value::Array(by.iter().map(one).collect())
}

/// Each fate and its name in answers.
const FATES: [(Fate, &str); 3] = [
    (Fate::Deleted, "deleted"),
    (Fate::SetupDropped, "setup_dropped"),
    (Fate::ParentCleared, "parent_cleared"),
];

/// The fate named `name` in an answer.
pub(super) fn fate_named(name: &str) -> Option<Fate> {
    FATES.iter().find(|(_, n)| *n == name).map(|(f, _)| *f)
}

/// The kind named `name` in an answer (`world`, `scenario`, …).
pub(super) fn kind_named(name: &str) -> Option<Kind> {
    Kind::ALL.into_iter().find(|k| kind_name(*k) == name)
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
