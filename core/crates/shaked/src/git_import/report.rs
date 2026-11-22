//! What the import operation answers: the plan, or once applied the outcome.

use std::path::Path;

use parts::Kind as PackageKind;
use serde_json::{json, Value};
use shakedown_project::{kind_name, ItemBody, Project};

use super::prepare::Prepared;
use super::{detect, items};

pub fn package_kind(kind: PackageKind) -> &'static str {
    match kind {
        PackageKind::Integration => "integration",
        PackageKind::Benchmark => "benchmark",
        PackageKind::Builder => "builder",
    }
}

/// The plan (or, once applied, the outcome).
pub fn plan_json(p: &Prepared) -> Value {
    let rel = |path: &Path| detect::rel(&p.checkout, path);
    let mut problems = p.found.problems.clone();
    problems.extend(p.sources.problems.iter().cloned());
    json!({
        "id": p.id,
        "url": p.url,
        "ref": p.git_ref,
        "commit": p.commit,
        "checkout": p.checkout.display().to_string(),
        "project": p.found.project.as_ref().map(project_json),
        "packages": packages_json(p),
        "items": p.items.iter().map(item_json).collect::<Vec<_>>(),
        "views": views(p.found.project.as_ref()),
        "compose": p.found.compose.iter().map(|c| compose_json(&p.checkout, c)).collect::<Vec<_>>(),
        "python": p.envs.iter().map(|e| json!({"root": rel(&e.root), "env": e.dir.display().to_string()})).collect::<Vec<_>>(),
        "edits": p.sources.edits.iter().map(|e| json!({"file": e.file, "package": e.package, "from": e.from, "to": e.to})).collect::<Vec<_>>(),
        "notes": p.sources.notes,
        "problems": problems,
        "conflicts": p.conflicts,
        "previous_commit": p.last.as_ref().map(|r| r.commit.clone()),
    })
}

fn packages_json(p: &Prepared) -> Vec<Value> {
    let rel = |path: &Path| detect::rel(&p.checkout, path);
    (p.found.packages.iter())
        .map(|f| {
            json!({
                "name": f.manifest.name,
                "kind": package_kind(f.manifest.kind),
                "dir": f.rel,
                "python": f.uv_root.as_deref().map(rel),
            })
        })
        .collect()
}

fn item_json(i: &items::ItemPlan) -> Value {
    let reason = match &i.action {
        items::Action::Keep(why) => Some(why.clone()),
        _ => None,
    };
    json!({"kind": kind_name(i.kind), "id": i.id.as_str(), "label": i.label,
           "action": i.action.name(), "reason": reason})
}

/// The items written or replaced.
pub fn written_json(p: &Prepared) -> Value {
    let written = (p.items.iter())
        .filter(|i| matches!(i.action, items::Action::Write | items::Action::Replace))
        .map(|i| json!({"kind": kind_name(i.kind), "id": i.id.as_str(), "label": i.label}));
    Value::Array(written.collect())
}

fn project_json(repo: &Project) -> Value {
    let problems: Vec<String> = (repo.problems.iter())
        .map(|pr| {
            let what = match (pr.kind, &pr.id) {
                (Some(k), Some(id)) => format!("{} {id}", kind_name(k)),
                (None, Some(id)) => id.clone(),
                _ => "shakedown.yaml".into(),
            };
            format!("{what}{}: {}", pr.pointer, pr.message)
        })
        .collect();
    json!({"name": repo.config.name, "problems": problems})
}

/// Each world's view files, `{world, file}`.
fn views(repo: Option<&Project>) -> Vec<Value> {
    let Some(repo) = repo else {
        return vec![];
    };
    let worlds = repo.list(shakedown_project::format::Kind::World);
    let mut out = vec![];
    for item in worlds {
        if let ItemBody::World(w) = &item.body {
            out.extend((w.views.iter()).map(|f| json!({"world": item.id.as_str(), "file": f})));
        }
    }
    out
}

fn compose_json(checkout: &Path, c: &detect::ComposeFile) -> Value {
    let file = checkout.join(&c.rel).display().to_string();
    json!({
        "file": c.rel,
        "services": c.services,
        "volumes": c.volumes,
        "part": {"integration": "service", "config": {"compose_file": file, "volumes": c.volumes}},
    })
}
