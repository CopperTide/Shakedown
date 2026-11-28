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
        if !ctx.project.current().await?.tables().is_none() {
            let change = table_delete(ctx, None).await?;
            let files: Vec<_> = change
                .items
                .iter()
                .filter(|i| !i.is_note())
                .map(|i| i.file_name())
                .collect();
            return Ok(json!({"trash": change.id, "label": change.label, "files": files}));
        }
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
        if !project.tables().is_none() {
            return restore_table(ctx, trash.as_deref(), confirm).await;
        }
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
            if let Err(e) = link_restored(ctx, &records, id, experiment).await {
                eprintln!("shaked: relinking the runs of a restored experiment: {e:#}");
                return false;
            }
        }
    }
    true
}

async fn table_delete(
    ctx: &OpCtx<'_>,
    id: Option<&str>,
) -> Result<crate::store::ChangeRow, super::OpError> {
    ctx.project
        .history(100)
        .await?
        .into_iter()
        .find(|c| {
            c.undone_at.is_none()
                && id.map_or_else(
                    || c.label.starts_with("delete "),
                    |id| c.id.to_string() == id,
                )
        })
        .ok_or_else(|| {
            shakedown_project::ProjectError::NotFound {
                kind: "delete to undo",
                id: id.unwrap_or("recent").into(),
            }
            .into()
        })
}

async fn restore_table(
    ctx: &OpCtx<'_>,
    id: Option<&str>,
    confirm: bool,
) -> Result<Value, super::OpError> {
    let change = table_delete(ctx, id).await?;
    let files: Vec<_> = change
        .items
        .iter()
        .filter(|i| !i.is_note())
        .map(|i| i.file_name())
        .collect();
    let listing = files.join("\n");
    let policy = ctx.project.current().await?.config.permissions.edit;
    if let ControlFlow::Break(mut proposal) = diff_first(policy, "", &listing, confirm) {
        proposal["trash"] = json!(change.id);
        return Ok(proposal);
    }
    let recorded = shakedown_project::history::Recorded {
        id: change.id,
        label: &change.label,
        items: &change.items,
    };
    ctx.project.change(|p| p.undo(recorded)).await??;
    let note = runs_note(&change);
    let runs_relinked = relink(ctx, note).await;
    Ok(
        json!({"restored": change.label, "files": files, "trash": change.id, "runs_relinked": runs_relinked}),
    )
}

fn runs_note(change: &crate::store::ChangeRow) -> Option<String> {
    change
        .items
        .iter()
        .find(|i| i.is_note() && i.name.ends_with(RUNS_NOTE))
        .and_then(|i| i.after_bytes.as_ref())
        .and_then(|b| String::from_utf8(b.clone()).ok())
}

async fn link_restored(
    ctx: &OpCtx<'_>,
    records: &crate::store::Scoped<'_>,
    id: &str,
    slug: &str,
) -> anyhow::Result<bool> {
    let project = ctx.project.current().await?;
    if project.tables().is_none() {
        return records.link_deployment(id, slug).await;
    }
    let slug_id = shakedown_project::Id::parse(slug)?;
    match project.row_id(shakedown_project::format::Kind::Experiment, &slug_id) {
        Some(row) => records.link_deployment_to_row(id, slug, row).await,
        None => Ok(false),
    }
}
