//! `project.import`: moves earlier work into the project. Without `confirm` it answers what it
//! would write; with it, it writes only what doesn't exist yet and links each earlier experiment
//! as its file's first deployment.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::kind_name;

use super::migrate::{plan, OldExperiment, Plan};
use super::{flag, input, schema, Access, OpCtx, OpError, OpFuture, Operation};
use crate::app::AppState;
use crate::auth::Caller;

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "project.import",
        area: "Project",
        summary: "Moves installed benchmarks and earlier experiments into the project as files; shows what it would write first",
        access: Access::Edit,
        method: "POST",
        path: "/api/project/import",
        target: None,
        input_schema: schema::<ImportInput>,
        run: import,
    }]
}

/// What `project.import` takes.
#[derive(Deserialize, JsonSchema)]
struct ImportInput {
    /// Write now; without it, nothing is written and the plan is answered.
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    confirm: bool,
}

fn import<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let ImportInput { confirm } = input(value)?;
        let app = ctx.app("moving earlier work")?;
        let old = earlier(app, &ctx.caller).await?;
        let moved = plan(&ctx.project.current()?, &benchmarks(app), &old);
        let files: Vec<Value> = moved
            .files
            .iter()
            .map(|(k, id, b)| json!({"kind": kind_name(*k), "id": id.as_str(), "label": b.label()}))
            .collect();
        if !confirm {
            return Ok(json!({ "proposed": true, "files": files, "skipped": moved.skipped }));
        }
        let linked = write(ctx, app, moved).await?;
        Ok(json!({ "written": files, "linked": linked.0, "skipped": linked.1 }))
    })
}

/// Writes the plan's files, then links the earlier experiments: how many were linked, and the
/// plan's notes.
async fn write(
    ctx: &OpCtx<'_>,
    app: &AppState,
    moved: Plan,
) -> Result<(usize, Vec<String>), OpError> {
    let Plan {
        files,
        links,
        skipped,
    } = moved;
    for (kind, id, body) in files {
        ctx.project.change(|p| p.put(kind, &id, body))??;
    }
    let mut linked = 0;
    for (old, id) in links {
        if app
            .store
            .at(&ctx.caller)
            .link_deployment(&old, id.as_str())
            .await
            .map_err(OpError::internal)?
        {
            linked += 1;
        }
    }
    Ok((linked, skipped))
}

/// Installed benchmarks and built worlds, as `(name, title)`.
fn benchmarks(app: &AppState) -> Vec<(String, String)> {
    let registry = app.hub.registry();
    let shown = registry
        .benchmarks()
        .filter(|m| !m.opt_in || m.world.is_some());
    shown
        .map(|m| {
            (
                m.name.clone(),
                m.world
                    .as_ref()
                    .map(|w| w.title.clone())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

/// The experiments not yet in a project, with their setups' teams.
async fn earlier(app: &AppState, caller: &Caller) -> Result<Vec<OldExperiment>, OpError> {
    let records = app.store.at(caller);
    let suites = records.suites().await.map_err(OpError::internal)?;
    let mut out = Vec::new();
    let unlinked = records.unlinked_experiments().await;
    for id in unlinked.map_err(OpError::internal)? {
        let Some(e) = records.experiment(&id).await.map_err(OpError::internal)? else {
            continue;
        };
        let teams = suites
            .iter()
            .filter(|s| s.experiment_id.as_deref() == Some(&id))
            .map(|s| s.team.clone())
            .collect();
        out.push(OldExperiment {
            id: e.id,
            name: e.name,
            task: e.task_id,
            benchmark: e.benchmark,
            runs: e.runs,
            teams,
        });
    }
    Ok(out)
}
