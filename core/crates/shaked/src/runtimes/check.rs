//! The conformance check of a runtime's packages, run when it is installed and on request, each
//! package on a hub of its own (its programs apart from the ones runs use, its saves in a
//! temporary folder): an integration gets `shakedown-part-check`'s check (`parts::check`) with an
//! empty config; a benchmark is described, and the integrations its world needs must be
//! installed. The reports are kept in `check.json` beside the import's record, for its commit.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use parts::{Hub, Kind, Manifest, Report, Step};
use serde_json::{json, Value};

use crate::git_import::record::Record;

const FILE: &str = "check.json";
/// Longest one package's check may take.
const LIMIT: Duration = Duration::from_secs(300);

/// Checks every package of the install recorded in `dir` that `hub` holds, keeps the reports and
/// answers them.
pub async fn run(hub: &Hub, dir: &Path, rec: &Record) -> Vec<Value> {
    let mut reports = vec![];
    for p in &rec.packages {
        let manifest = hub.registry().get(&p.name).cloned();
        let report = match manifest {
            Some(m) => one(hub, dir, m).await,
            None => failed(&p.name, "describe", "the package is not registered"),
        };
        reports.push(json!(report));
    }
    let kept = json!({"commit": rec.commit, "checked_at": chrono::Utc::now().to_rfc3339(), "reports": reports});
    if let Err(e) = std::fs::write(dir.join(FILE), kept.to_string()) {
        eprintln!("shaked: could not keep the check of {}: {e}", rec.id);
    }
    reports
}

/// The kept reports of the install in `dir`, by package, when they are of its current commit.
pub fn read(dir: &Path, rec: &Record) -> HashMap<String, Value> {
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap_or_default();
    let kept: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if kept["commit"] != rec.commit.as_str() {
        return HashMap::new();
    }
    let at = kept["checked_at"].clone();
    (kept["reports"].as_array().into_iter().flatten())
        .filter_map(|r| {
            let mut r = r.clone();
            r["checked_at"] = at.clone();
            Some((r["package"].as_str()?.to_string(), r))
        })
        .collect()
}

/// The check of the package `m`, on a hub of its own.
async fn one(hub: &Hub, dir: &Path, m: Manifest) -> Report {
    let name = m.name.clone();
    let saves = match tempfile::Builder::new().prefix(".check-").tempdir_in(dir) {
        Ok(s) => s,
        Err(e) => return failed(&name, "describe", &format!("a saves folder: {e}")),
    };
    let kind = m.kind;
    let alone = match hub.alone(m, saves.path().to_path_buf()) {
        Ok(h) => h,
        Err(e) => return failed(&name, "describe", &format!("{e:#}")),
    };
    let checked = match kind {
        Kind::Integration => {
            tokio::time::timeout(LIMIT, parts::check(&alone, &name, json!({}))).await
        }
        Kind::Benchmark => tokio::time::timeout(LIMIT, benchmark(hub, &alone, &name)).await,
        Kind::Builder => Ok(passed(&name, "describe", "a world builder: not checked")),
    };
    drop(alone);
    checked.unwrap_or_else(|_| failed(&name, "describe", "the check took more than 5 minutes"))
}

/// A benchmark describes itself with at least one task, and the integrations its world's parts
/// name are installed in `hub`.
async fn benchmark(hub: &Hub, alone: &Hub, name: &str) -> Report {
    let d = match alone.bench_describe(name).await {
        Ok(d) => d,
        Err(e) => return failed(name, "describe", &format!("{e:#}")),
    };
    let tasks = format!(
        "{} tasks, a world of {} parts",
        d.tasks.len(),
        d.world.len()
    );
    let missing: Vec<String> = (d.world.iter())
        .filter(|p| hub.registry().get(&p.integration).is_none())
        .map(|p| format!("{} ({})", p.part, p.integration))
        .collect();
    let steps = vec![
        step("describe", !d.tasks.is_empty(), &tasks),
        match missing.is_empty() {
            true => step("parts", true, "every part's integration is installed"),
            false => step(
                "parts",
                false,
                &format!("not installed: {}", missing.join(", ")),
            ),
        },
    ];
    report(name, steps)
}

fn step(name: &str, ok: bool, detail: &str) -> Step {
    Step {
        name: name.into(),
        ok,
        detail: detail.into(),
    }
}

fn report(package: &str, steps: Vec<Step>) -> Report {
    Report {
        package: package.into(),
        passed: steps.iter().all(|s| s.ok),
        steps,
        skipped: vec![],
    }
}

fn failed(package: &str, at: &str, detail: &str) -> Report {
    report(package, vec![step(at, false, detail)])
}

fn passed(package: &str, at: &str, detail: &str) -> Report {
    report(package, vec![step(at, true, detail)])
}
