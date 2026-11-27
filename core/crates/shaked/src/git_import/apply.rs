//! Carrying out a prepared import: the kit edits, the uv environments, the project's items (the
//! last import's that the repo no longer has removed), the record and the registry, then removing
//! checkouts two commits old.

use std::path::{Path, PathBuf};

use parts::{Hub, Manifest};
use serde_json::{json, Value};
use shakedown_project::Project;

use super::prepare::Prepared;
use super::record::{self, Record, RecordedPackage};
use super::{detect, fetch, items, python, report, ImportError};

pub async fn apply(
    project: &Path,
    mut p: Prepared,
    hub: Option<&Hub>,
) -> Result<Value, ImportError> {
    if !p.conflicts.is_empty() {
        return Err(ImportError::Conflict(p.conflicts.join("; ")));
    }
    if p.packages_only.is_some() && p.found.packages.is_empty() {
        return Err(ImportError::Bad(p.found.problems.join("; ")));
    }
    install(&p).await?;
    let held = match &p.found.project {
        Some(repo) => {
            ensure_project(project, repo)?;
            items::write(&p.checkout, project, &mut p.items)?
        }
        None => vec![],
    };
    let (manifests, packages) = packages_of(&p);
    record::write(&p.dir, &record_of(&p, packages, held))?;
    if let Some(hub) = hub {
        hub.replace_source(&p.id, &p.commit, manifests)
            .await
            .map_err(|e| ImportError::Conflict(format!("{e:#}")))?;
    }
    remove_old(&p);
    let mut out = report::plan_json(&p);
    out["registered"] = json!(hub.is_some());
    out["written"] = report::written_json(&p);
    out["removed"] = report::removed_json(&p);
    Ok(out)
}

/// Points the kit sources at this Shakedown's and installs each uv workspace's environment.
async fn install(p: &Prepared) -> Result<(), ImportError> {
    for (file, text) in &p.sources.writes {
        std::fs::write(file, text).map_err(|e| ImportError::io("writing", file, e))?;
    }
    for env in &p.envs {
        python::install(&env.root, &env.dir).await?;
    }
    Ok(())
}

fn record_of(
    p: &Prepared,
    packages: Vec<RecordedPackage>,
    items: Vec<record::RecordedItem>,
) -> Record {
    let edits = (p.sources.edits.iter())
        .map(|e| format!("{}: {} -> {}", e.file, e.package, e.to))
        .collect();
    Record {
        id: p.id.clone(),
        source: p.source.to_string(),
        packages_only: p.packages_only.clone(),
        url: p.url.clone(),
        git_ref: p.git_ref.clone(),
        commit: p.commit.clone(),
        imported_at: chrono::Utc::now().to_rfc3339(),
        packages,
        items,
        edits,
    }
}

/// Makes `project` a project with the repo's settings when it is not one yet.
fn ensure_project(project: &Path, repo: &Project) -> Result<(), ImportError> {
    if project.join("shakedown.yaml").is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(project).map_err(|e| ImportError::io("creating", project, e))?;
    let made = Project::init(project, &repo.config.name, false)
        .and_then(|mut p| p.set_config(repo.config.clone()));
    made.map_err(|e| ImportError::Io(format!("making the project: {e}")))
}

/// The packages to register (their environments set) and how the record lists them.
fn packages_of(p: &Prepared) -> (Vec<Manifest>, Vec<RecordedPackage>) {
    let mut manifests = vec![];
    let mut recorded = vec![];
    for found in &p.found.packages {
        let env = found.uv_root.as_ref().and_then(|root| {
            let env = p.envs.iter().find(|e| &e.root == root)?;
            Some(detect::rel(&p.dir, &env.dir))
        });
        let mut m = found.manifest.clone();
        m.env = program_env(&p.dir, env.as_deref());
        recorded.push(RecordedPackage {
            name: m.name.clone(),
            kind: report::package_kind(m.kind).into(),
            dir: found.rel.clone(),
            env,
        });
        manifests.push(m);
    }
    (manifests, recorded)
}

/// The environment an imported package's program gets: its uv environment, used as installed.
pub fn program_env(dir: &Path, env: Option<&str>) -> Vec<(String, String)> {
    let Some(env) = env else {
        return vec![];
    };
    let env = dir.join(env).to_string_lossy().into_owned();
    vec![
        ("UV_PROJECT_ENVIRONMENT".into(), env),
        ("UV_FROZEN".into(), "1".into()),
    ]
}

/// Removes the checkouts and environments of commits other than this one and the one before
/// (a run going on the one before finishes on it).
fn remove_old(p: &Prepared) {
    let mut keep = vec![fetch::short(&p.commit).to_string()];
    keep.extend(p.last.as_ref().map(|l| fetch::short(&l.commit).to_string()));
    let kept = |name: &str| keep.iter().any(|k| name.starts_with(k.as_str()));
    let old = |dir: &Path| -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return vec![];
        };
        let names = rd.filter_map(Result::ok).filter(|e| e.path().is_dir());
        names
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name != "env" && !name.starts_with('.') && !kept(&name)
            })
            .map(|e| e.path())
            .collect()
    };
    for d in old(&p.dir).into_iter().chain(old(&p.dir.join("env"))) {
        if let Err(e) = std::fs::remove_dir_all(&d) {
            eprintln!("shaked: could not remove {}: {e}", d.display());
        }
    }
}
