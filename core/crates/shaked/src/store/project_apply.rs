//! Apply a model change and its history in the caller's transaction.

use anyhow::{Context, Result};
use shakedown_project::format::Kind;
use shakedown_project::{Base as ModelBase, ChangeSet, ItemChange};
use uuid::Uuid;

use super::project_history::record_named_change_in;
use super::project_items::{
    delete_item_in, project_row, put_item_in, restore_item_in, settings_query, PROJECT_COLS,
    SETTINGS,
};
use super::{Base, ItemKind, ItemWrite, Scoped, Stale, Tx};

impl Scoped<'_> {
    /// Applies every row or none. Unrelated item edits may commit independently.
    pub async fn apply_project_change(&self, project: Uuid, change: &ChangeSet) -> Result<i64> {
        self.in_tx(async |tx| apply_project_change_in(tx, project, change).await)
            .await
    }
}

/// Applies typed rows, settings and history in `tx`. The hosting home must persist the file
/// changes in this same transaction before committing it.
pub async fn apply_project_change_in(tx: &Tx, project: Uuid, change: &ChangeSet) -> Result<i64> {
    let sql = format!("SELECT {PROJECT_COLS} FROM projects WHERE id = $1 FOR UPDATE");
    let row = tx
        .query_opt(&sql, &[&project])
        .await?
        .context("project not found")?;
    let current = project_row(&row);
    check_settings(change, &current)?;
    let mut rows = Vec::new();
    for item in &change.items {
        rows.push(apply_item(tx, project, item).await?);
    }
    let revision = current.revision + 1;
    bump_project(tx, project, change).await?;
    let items = change.records(project, revision, &rows);
    record_named_change_in(tx, project, change.id, &change.label, &items).await?;
    if let Some(id) = change.undoes {
        let sql = "UPDATE project_changes SET undone_at = now() WHERE id = $1 AND project_id = $2 AND undone_at IS NULL";
        anyhow::ensure!(
            tx.execute(sql, &[&id, &project]).await? == 1,
            "change already undone or missing"
        );
    }
    tx.execute("DELETE FROM project_changes WHERE project_id = $1 AND created_at < now() - interval '30 days' AND id NOT IN (SELECT id FROM project_changes WHERE project_id = $1 ORDER BY created_at DESC, id DESC LIMIT 20)", &[&project]).await?;
    Ok(revision)
}

fn check_settings(change: &ChangeSet, current: &super::ProjectRow) -> Result<()> {
    if let Some(settings) = &change.settings {
        if settings.base_revision != current.revision || settings.before != current.settings {
            return Err(Stale {
                what: "project settings".into(),
                based_on: settings.base_revision,
                current: Some(current.revision),
            }
            .into());
        }
    }
    Ok(())
}

async fn bump_project(tx: &Tx, project: Uuid, change: &ChangeSet) -> Result<()> {
    match &change.settings {
        Some(settings) => {
            let sql = format!("UPDATE projects SET {SETTINGS}, revision = revision + 1, updated_at = now() WHERE id = $1 RETURNING id");
            settings_query(tx, &sql, project, &settings.after).await?;
        }
        None => {
            tx.execute(
                "UPDATE projects SET revision = revision + 1, updated_at = now() WHERE id = $1",
                &[&project],
            )
            .await?;
        }
    }
    Ok(())
}

async fn apply_item(tx: &Tx, project: Uuid, item: &ItemChange) -> Result<ModelBase> {
    let kind = row_kind(item.kind)?;
    let base = item.base.map(|b| Base {
        id: b.id,
        revision: b.revision,
    });
    let row = write_item(tx, project, item, kind, base).await?;
    Ok(ModelBase {
        id: row.id,
        revision: row.revision,
    })
}

impl Scoped<'_> {
    /// Keeps a note with a change so restoring it can repair deployment links.
    pub async fn note_project_change(
        &self,
        project: Uuid,
        change: Uuid,
        name: &str,
        text: &str,
    ) -> Result<()> {
        anyhow::ensure!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
            "invalid history note name"
        );
        self.in_tx(async |tx| {
            tx.query_opt("SELECT id FROM project_changes WHERE id = $1 AND project_id = $2 FOR UPDATE", &[&change, &project]).await?.context("change not found")?;
            let name = format!("{}{}", shakedown_project::history::NOTES, name);
            let bytes = text.as_bytes();
            tx.execute("INSERT INTO project_change_items (change_id, n, kind, name, after_bytes) SELECT $1, coalesce(max(n), -1) + 1, 'file', $2, $3 FROM project_change_items WHERE change_id = $1", &[&change, &name, &bytes]).await?;
            Ok(())
        }).await
    }
}

fn row_kind(kind: Kind) -> Result<ItemKind> {
    Ok(match kind {
        Kind::Scenario => ItemKind::Scenario,
        Kind::Setup => ItemKind::Setup,
        Kind::Experiment => ItemKind::Experiment,
        Kind::World => anyhow::bail!("worlds must remain files"),
    })
}

async fn write_item(
    tx: &Tx,
    project: Uuid,
    item: &ItemChange,
    kind: ItemKind,
    base: Option<Base>,
) -> Result<super::ProjectItem> {
    Ok(match &item.after {
        Some(body) => {
            let write = ItemWrite {
                kind,
                slug: item.slug.as_str(),
                body,
                base,
            };
            if item.restore {
                restore_item_in(tx, project, write).await?
            } else {
                put_item_in(tx, project, write).await?
            }
        }
        None => {
            delete_item_in(
                tx,
                project,
                kind,
                base.context("deleted item needs a base")?,
            )
            .await?
        }
    })
}
