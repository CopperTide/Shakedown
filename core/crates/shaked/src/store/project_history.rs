//! A project's undo history and git import records in Postgres (migration `shakedown 10`), as the
//! caller. A change is one edit, delete, import or undo, with what it did to each item: a row's
//! body before and after, or a file's bytes. An import record is what one git import fetched,
//! registered and wrote; a re-import updates it in place. Typed homes use this history; legacy
//! folder homes retain their trash and `import.json`.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::Row;
use uuid::Uuid;

use super::{Scoped, Tx};

pub use shakedown_project::history::{ChangeItem, ChangeKind};

/// A change as the history lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRow {
    pub id: Uuid,
    /// What it did, e.g. `delete world mill`.
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub undone_at: Option<DateTime<Utc>>,
    pub items: Vec<ChangeItem>,
}

/// One git import into a project (`import.json`'s fields).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRecord {
    /// The import's id.
    pub slug: String,
    pub url: String,
    pub git_ref: String,
    pub commit_sha: String,
    pub imported_at: DateTime<Utc>,
    /// The packages it registered, each `{name, kind, dir, env?}`.
    pub packages: Value,
    /// The items it wrote, each `{kind, id, digest}`.
    pub items: Value,
    /// The edits it made to the checkout.
    pub edits: Value,
}

const CHANGE_COLS: &str = "id, label, created_at, undone_at";
const ITEM_COLS: &str =
    "change_id, kind, item_id, name, before, after, before_bytes, after_bytes, revision";
const IMPORT_COLS: &str = "slug, url, git_ref, commit_sha, imported_at, packages, items, edits";

impl Scoped<'_> {
    /// Records a change to project `project` labelled `label`, with its items in order.
    pub async fn record_project_change(
        &self,
        project: Uuid,
        label: &str,
        items: &[ChangeItem],
    ) -> Result<Uuid> {
        self.in_tx(async |tx| record_change_in(tx, project, label, items).await)
            .await
    }

    /// Project `project`'s last `limit` changes, newest first, each with its items.
    pub async fn project_changes(&self, project: Uuid, limit: i64) -> Result<Vec<ChangeRow>> {
        self.in_tx(async |tx| changes_in(tx, project, limit).await)
            .await
    }

    /// Records import `r.slug` of project `project`, replacing the record a previous import of
    /// that id left.
    pub async fn upsert_project_import(&self, project: Uuid, r: &ImportRecord) -> Result<()> {
        let sql = format!(
            "INSERT INTO project_imports (project_id, {IMPORT_COLS})
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (project_id, slug) DO UPDATE SET url = $3, git_ref = $4, commit_sha = $5,
               imported_at = $6, packages = $7, items = $8, edits = $9, updated_at = now()"
        );
        let p: [&(dyn tokio_postgres::types::ToSql + Sync); 9] = [
            &project,
            &r.slug,
            &r.url,
            &r.git_ref,
            &r.commit_sha,
            &r.imported_at,
            &r.packages,
            &r.items,
            &r.edits,
        ];
        self.execute(&sql, &p).await.map(drop)
    }

    /// Project `project`'s import records, by id.
    pub async fn project_imports(&self, project: Uuid) -> Result<Vec<ImportRecord>> {
        let sql = format!(
            "SELECT {IMPORT_COLS} FROM project_imports WHERE project_id = $1 ORDER BY slug"
        );
        let rows = self.query(&sql, &[&project]).await?;
        Ok(rows.iter().map(import_row).collect())
    }
}

/// Records a change in `tx` (see [`Scoped::record_project_change`]).
pub(super) async fn record_change_in(
    tx: &Tx,
    project: Uuid,
    label: &str,
    items: &[ChangeItem],
) -> Result<Uuid> {
    record_named_change_in(tx, project, Uuid::new_v4(), label, items).await
}

pub(super) async fn record_named_change_in(
    tx: &Tx,
    project: Uuid,
    id: Uuid,
    label: &str,
    items: &[ChangeItem],
) -> Result<Uuid> {
    let sql = "INSERT INTO project_changes (id, project_id, label) VALUES ($1, $2, $3)";
    tx.execute(sql, &[&id, &project, &label]).await?;
    let insert = format!(
        "INSERT INTO project_change_items (n, {ITEM_COLS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
    );
    for (n, i) in (0i32..).zip(items) {
        let kind = i.kind.as_str();
        let p: [&(dyn tokio_postgres::types::ToSql + Sync); 10] = [
            &n,
            &id,
            &kind,
            &i.item_id,
            &i.name,
            &i.before,
            &i.after,
            &i.before_bytes,
            &i.after_bytes,
            &i.revision,
        ];
        tx.execute(&insert, &p).await?;
    }
    Ok(id)
}

async fn changes_in(tx: &Tx, project: Uuid, limit: i64) -> Result<Vec<ChangeRow>> {
    let sql = format!(
        "SELECT {CHANGE_COLS} FROM project_changes WHERE project_id = $1
          ORDER BY created_at DESC, id LIMIT $2"
    );
    let mut changes: Vec<ChangeRow> = tx
        .query(&sql, &[&project, &limit])
        .await?
        .iter()
        .map(change_row)
        .collect();
    let ids: Vec<Uuid> = changes.iter().map(|c| c.id).collect();
    let sql = format!(
        "SELECT {ITEM_COLS} FROM project_change_items WHERE change_id = ANY($1) ORDER BY change_id, n"
    );
    for r in tx.query(&sql, &[&ids]).await? {
        let change: Uuid = r.get(0);
        if let Some(c) = changes.iter_mut().find(|c| c.id == change) {
            c.items.push(change_item(&r));
        }
    }
    Ok(changes)
}

fn change_row(r: &Row) -> ChangeRow {
    ChangeRow {
        id: r.get(0),
        label: r.get(1),
        created_at: r.get(2),
        undone_at: r.get(3),
        items: Vec::new(),
    }
}

fn change_item(r: &Row) -> ChangeItem {
    ChangeItem {
        kind: ChangeKind::parse(r.get(1)).expect("the schema checks a change item's kind"),
        item_id: r.get(2),
        name: r.get(3),
        before: r.get(4),
        after: r.get(5),
        before_bytes: r.get(6),
        after_bytes: r.get(7),
        revision: r.get(8),
    }
}

fn import_row(r: &Row) -> ImportRecord {
    ImportRecord {
        slug: r.get(0),
        url: r.get(1),
        git_ref: r.get(2),
        commit_sha: r.get(3),
        imported_at: r.get(4),
        packages: r.get(5),
        items: r.get(6),
        edits: r.get(7),
    }
}
