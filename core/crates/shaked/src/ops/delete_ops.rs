//! Deleting items: what uses one (the preview the delete dialog shows), the delete itself, and
//! undoing a delete from the trash. A delete is refused while anything uses the item unless sent
//! with `cascade`; a cascade sent with `expect` (what the preview showed) is refused when that has
//! changed; under "show a diff first" an unconfirmed cascade only answers what it would take. The
//! deployments of a deleted experiment are unlinked from it, so they list as earlier experiments,
//! and noted in the delete's trash so undoing it (`restore_ops`) links them again.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::{EditPolicy, Kind};
use shakedown_project::{Expected, Id, Removal};

use crate::store::Scoped;

use super::item_ops::kind_of;
use super::{flag, input, view, IdInput, OpCtx, OpError, OpFuture, Operation};

/// What a delete takes.
#[derive(Deserialize, JsonSchema)]
pub(super) struct DeleteInput {
    /// The item's id: lowercase letters, digits and dashes.
    id: String,
    /// Also delete what uses it (a setup instead leaves the experiments comparing others, and a
    /// sub-experiment loses its parent).
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    cascade: bool,
    /// The `dependents` a preview showed: the delete is refused if what uses the item changed.
    #[serde(default)]
    expect: Option<Vec<Shown>>,
    /// Delete now: under "show a diff first", a cascade otherwise answers what it would take.
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    confirm: bool,
}

/// One dependent as the preview showed it.
#[derive(Deserialize, JsonSchema)]
struct Shown {
    /// `scenario` or `experiment`.
    kind: String,
    id: String,
    /// Every fate the preview showed: `deleted`, or `setup_dropped` and/or `parent_cleared`.
    #[serde(default)]
    fates: Option<Vec<String>>,
    /// The first of them, when `fates` is missing.
    #[serde(default)]
    fate: Option<String>,
}

/// The trash note naming the deployments a delete unlinked, by project experiment.
pub(super) const RUNS_NOTE: &str = "runs.json";

pub(super) fn delete<'a>(ctx: &'a OpCtx<'a>, op: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let DeleteInput {
            id,
            cascade,
            expect,
            confirm,
        } = input(value)?;
        let (kind, id, expect) = (kind_of(op)?, Id::parse(&id)?, shown(expect)?);
        if let Some(proposal) = held(ctx, kind, &id, cascade && !confirm)? {
            return Ok(proposal);
        }
        let removal = ctx
            .project
            .change(|p| p.delete(kind, &id, cascade, expect.as_deref()))??;
        let runs_unlinked = unlink_runs(ctx, kind, &id, &removal).await;
        let with = view::dependents(&removal.fallout);
        let trash = removal.trash;
        Ok(
            json!({ "deleted": id.as_str(), "with": with, "trash": trash, "runs_unlinked": runs_unlinked }),
        )
    })
}

pub(super) fn dependents<'a>(
    ctx: &'a OpCtx<'a>,
    op: &'static Operation,
    value: Value,
) -> OpFuture<'a> {
    Box::pin(async move {
        let (kind, id) = (kind_of(op)?, Id::parse(&input::<IdInput>(value)?.id)?);
        let project = ctx.project.current()?;
        if project.get(kind, &id).is_none() {
            return Err(shakedown_project::ProjectError::not_found(kind, &id).into());
        }
        let dependents = view::dependents(&project.fallout(kind, &id));
        Ok(json!({ "dependents": dependents, "kept": project.kept_folder(kind, &id) }))
    })
}

/// The preview's dependents as kinds, ids and fates.
fn shown(list: Option<Vec<Shown>>) -> Result<Option<Vec<Expected>>, OpError> {
    let Some(list) = list else { return Ok(None) };
    let bad = |what: &str, v: &str| OpError::bad_request(format!("expect: no {what} {v:?}"));
    let one = |s: Shown| -> Result<Expected, OpError> {
        let kind = view::kind_named(&s.kind).ok_or_else(|| bad("kind", &s.kind))?;
        let names = s.fates.unwrap_or_else(|| s.fate.into_iter().collect());
        let fates = names
            .iter()
            .map(|f| view::fate_named(f).ok_or_else(|| bad("fate", f)));
        Ok((kind, Id::parse(&s.id)?, fates.collect::<Result<_, _>>()?))
    };
    list.into_iter()
        .map(one)
        .collect::<Result<_, _>>()
        .map(Some)
}

/// Under "show a diff first", what an unconfirmed cascade would take; nothing is written.
fn held(ctx: &OpCtx<'_>, kind: Kind, id: &Id, unconfirmed: bool) -> Result<Option<Value>, OpError> {
    if !unconfirmed {
        return Ok(None);
    }
    let project = ctx.project.current()?;
    let fallout = project.fallout(kind, id);
    if project.config.permissions.edit != EditPolicy::DiffFirst || fallout.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({ "proposed": true, "dependents": view::dependents(&fallout) }),
    ))
}

/// Unlinks the deployments of every experiment the delete removed, and notes them in its trash
/// for an undo. Whether that worked: the files have moved, so a database failure is logged and
/// answered as `runs_unlinked: false`, never an error. Over stdio there is no database.
async fn unlink_runs(ctx: &OpCtx<'_>, kind: Kind, id: &Id, removal: &Removal) -> bool {
    let Some(app) = ctx.app else { return true };
    let gone = removal
        .fallout
        .iter()
        .filter(|d| d.kind == Kind::Experiment && d.deleted());
    let itself = (kind == Kind::Experiment).then_some(id);
    let experiments: Vec<&Id> = itself.into_iter().chain(gone.map(|d| &d.id)).collect();
    let unlinked = match unlink(&app.store.at(&ctx.caller), &experiments).await {
        Ok(unlinked) => unlinked,
        Err(e) => {
            eprintln!("shaked: unlinking the runs of deleted experiments: {e:#}");
            return false;
        }
    };
    let note = serde_json::to_string(&unlinked).unwrap_or_default();
    let kept = ctx
        .project
        .current()
        .map(|p| p.write_trash_note(&removal.trash, RUNS_NOTE, &note));
    if let Ok(Err(e)) = kept {
        eprintln!("shaked: noting the unlinked runs of a delete: {e}");
    }
    true
}

/// Unlinks the deployments of `experiments`: each experiment's engine experiment ids.
async fn unlink(
    records: &Scoped<'_>,
    experiments: &[&Id],
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let mut unlinked = BTreeMap::new();
    for experiment in experiments {
        let deployed = records.deployments_of(experiment.as_str()).await?;
        records.unlink_deployments(experiment.as_str()).await?;
        unlinked.insert(experiment.to_string(), deployed);
    }
    Ok(unlinked)
}
