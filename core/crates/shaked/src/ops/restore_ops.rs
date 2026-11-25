//! Undoing a delete: the most recent delete (what the app's Undo and Project settings offer), and
//! restoring a delete from the trash. Under "show a diff first" an unconfirmed restore answers the
//! files it would put back and the delete's `trash`, so confirming restores that same delete.
//! The deployments the delete unlinked are linked to their experiment again.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::delete_ops::RUNS_NOTE;
use super::project_ops::NoInput;
use super::{diff_first, flag, input, schema, Access, OpCtx, OpFuture, Operation};

pub(super) fn ops() -> Vec<Operation> {
    let op = |name, summary, access, method, path, input_schema, run| Operation {
        name,
        area: "Project",
        summary,
        access,
        method,
        path,
        target: None,
        input_schema,
        run,
    };
    vec![
        op("project.deleted", "The most recent delete that can be undone: its trash, what it deleted and its files",
            Access::Read, "GET", "/api/project/trash", schema::<NoInput>, last as super::OpFn),
        op("project.restore", "Undoes the most recent delete (or the one named), putting its files back; shows which files first unless confirmed",
            Access::Edit, "POST", "/api/project/trash/restore", schema::<RestoreInput>, restore),
    ]
}

fn last<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let (name, manifest) = ctx.project.current().await?.trashed(None)?;
        let files: Vec<&str> = manifest.files().collect();
        Ok(json!({ "trash": name, "label": manifest.label, "files": files }))
    })
}

/// What undoing a delete takes.
#[derive(Deserialize, JsonSchema)]
struct RestoreInput {
    /// The delete to undo, as its `trash` answered; the most recent when missing.
    #[serde(default)]
    trash: Option<String>,
    /// Restore now, without showing which files first.
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    confirm: bool,
}

fn restore<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let RestoreInput { trash, confirm } = input(value)?;
        let project = ctx.project.current().await?;
        let (name, manifest) = project.trashed(trash.as_deref())?;
        let files: Vec<&str> = manifest.files().collect();
        let listing: String = files.iter().map(|f| format!("{f}\n")).collect();
        let policy = project.config.permissions.edit;
        if let ControlFlow::Break(mut proposal) = diff_first(policy, "", &listing, confirm) {
            proposal["trash"] = json!(name);
            return Ok(proposal);
        }
        let runs = project.trash_note(&name, RUNS_NOTE);
        ctx.project.change(|p| p.restore(Some(&name))).await??;
        let runs_relinked = relink(ctx, runs).await;
        let label = &manifest.label;
        Ok(
            json!({ "restored": label, "files": files, "trash": name, "runs_relinked": runs_relinked }),
        )
    })
}

/// Links each deployment the delete unlinked to its experiment again (unless it was linked
/// since). Whether that worked: the files are back, so a database failure is logged, not an error.
async fn relink(ctx: &OpCtx<'_>, note: Option<String>) -> bool {
    let (Some(app), Some(note)) = (ctx.app, note) else {
        return true;
    };
    let unlinked: BTreeMap<String, Vec<String>> = serde_json::from_str(&note).unwrap_or_default();
    let records = app.store.at(&ctx.caller);
    for (experiment, deployed) in &unlinked {
        for id in deployed {
            if let Err(e) = records.link_deployment(id, experiment).await {
                eprintln!("shaked: relinking the runs of a restored experiment: {e:#}");
                return false;
            }
        }
    }
    true
}
