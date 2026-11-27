//! A project's state in Postgres (migration `shakedown 10`), as the caller: its settings (a
//! [`ProjectFile`] in typed columns) and its scenarios, setups and experiments, each the item's body
//! exactly as its file says. Every write of a row names the revision it was based on and is
//! refused with [`Stale`] when the row has moved past it; a delete only marks the row. The undo
//! history and the git import records are in [`super::project_history`]. Nothing reads these yet:
//! the project still lives in its folder or the hosting layer's files.
//!
//! ```text
//! Scoped::put_project_item ─► in_tx ─► put_item_in ─► UPDATE … WHERE revision = base
//!                                                       └─ no row ─► Stale { current }
//! ```
//!
//! The `_in` functions take an open transaction, so one change can write several rows at once.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use shakedown_project::format::{
    AssistantSettings, Defaults, EditPolicy, Permissions, ProjectFile, SpendPolicy,
};
use tokio_postgres::{error::SqlState, Row};
use uuid::Uuid;

use super::{Scoped, Tx};

/// The kinds of item a project keeps as rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Scenario,
    Setup,
    Experiment,
}

impl ItemKind {
    pub const ALL: [ItemKind; 3] = [ItemKind::Scenario, ItemKind::Setup, ItemKind::Experiment];

    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Scenario => "scenario",
            ItemKind::Setup => "setup",
            ItemKind::Experiment => "experiment",
        }
    }

    fn table(self) -> &'static str {
        match self {
            ItemKind::Scenario => "project_scenarios",
            ItemKind::Setup => "project_setups",
            ItemKind::Experiment => "project_experiments",
        }
    }
}

/// A project: its settings and their revision.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRow {
    pub id: Uuid,
    pub settings: ProjectFile,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
}

/// A scenario, setup or experiment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectItem {
    pub id: Uuid,
    pub kind: ItemKind,
    pub slug: String,
    /// The item as its file says, `format:` included.
    pub body: Value,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
}

/// A project with its live items, by kind then slug.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectItems {
    pub project: ProjectRow,
    pub items: Vec<ProjectItem>,
}

/// The row a write is based on: its id and the revision the writer read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base {
    pub id: Uuid,
    pub revision: i64,
}

/// A write of an item: a new one when `base` is None, else the row `base` names (its slug may
/// change: a rename).
#[derive(Debug, Clone, Copy)]
pub struct ItemWrite<'a> {
    pub kind: ItemKind,
    pub slug: &'a str,
    pub body: &'a Value,
    pub base: Option<Base>,
}

/// A row changed since the revision a write was based on (`current`: None when it is gone).
#[derive(Debug, thiserror::Error)]
#[error("{what} was changed by someone else since revision {based_on}; reload")]
pub struct Stale {
    pub what: String,
    pub based_on: i64,
    pub current: Option<i64>,
}

/// Another live item of the kind has the slug.
#[derive(Debug, thiserror::Error)]
#[error("a {kind} named \"{slug}\" already exists")]
pub struct SlugTaken {
    pub kind: &'static str,
    pub slug: String,
}

const PROJECT_COLS: &str = "id, name, default_world, cost_cap_usd, edit_policy, spend_policy, \
     spend_usd_per_day, assistant_model, assistant_provider, revision, updated_at";
const SETTINGS: &str = "name = $2, default_world = $3, cost_cap_usd = $4, edit_policy = $5, \
     spend_policy = $6, spend_usd_per_day = $7, assistant_model = $8, assistant_provider = $9";
const ITEM_COLS: &str = "id, slug, body, revision, updated_at";

impl Scoped<'_> {
    /// Makes a project with `settings`, at revision 1.
    pub async fn create_project(&self, settings: &ProjectFile) -> Result<ProjectRow> {
        let sql = format!(
            "INSERT INTO projects (id, name, default_world, cost_cap_usd, edit_policy, spend_policy,
                spend_usd_per_day, assistant_model, assistant_provider)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {PROJECT_COLS}"
        );
        let row = self
            .in_tx(async |tx| settings_query(tx, &sql, Uuid::new_v4(), settings).await)
            .await?;
        Ok(project_row(&row.expect("the project just made")))
    }

    /// Replaces project `base.id`'s settings: [`Stale`] when it moved past `base.revision`.
    pub async fn update_project(&self, base: Base, settings: &ProjectFile) -> Result<ProjectRow> {
        let sql = format!(
            "UPDATE projects SET {SETTINGS}, revision = revision + 1, updated_at = now()
              WHERE id = $1 AND revision = $10 RETURNING {PROJECT_COLS}"
        );
        self.in_tx(async |tx| {
            let p = settings_params(base.id, settings);
            let mut params = p.refs();
            params.push(&base.revision);
            match tx.query_opt(&sql, &params).await? {
                Some(r) => Ok(project_row(&r)),
                None => Err(stale(tx, "projects", "the project", base).await),
            }
        })
        .await
    }

    /// Project `id`'s settings and its live items; None when there is no such project.
    pub async fn project_items(&self, id: Uuid) -> Result<Option<ProjectItems>> {
        self.in_tx(async |tx| {
            let sql = format!("SELECT {PROJECT_COLS} FROM projects WHERE id = $1");
            let Some(row) = tx.query_opt(&sql, &[&id]).await? else {
                return Ok(None);
            };
            let items = items_in(tx, id).await?;
            let project = project_row(&row);
            Ok(Some(ProjectItems { project, items }))
        })
        .await
    }

    /// Writes an item of project `project`: [`SlugTaken`] when another live item of its kind has
    /// the slug, [`Stale`] when its row moved past the base revision (or is deleted).
    pub async fn put_project_item(&self, project: Uuid, w: ItemWrite<'_>) -> Result<ProjectItem> {
        self.in_tx(async |tx| put_item_in(tx, project, w).await)
            .await
    }

    /// Marks item `base.id` deleted, bumping its revision: [`Stale`] as for a write.
    pub async fn delete_project_item(
        &self,
        project: Uuid,
        kind: ItemKind,
        base: Base,
    ) -> Result<ProjectItem> {
        self.in_tx(async |tx| delete_item_in(tx, project, kind, base).await)
            .await
    }
}

/// The live items of `project`, by kind then slug.
pub(super) async fn items_in(tx: &Tx, project: Uuid) -> Result<Vec<ProjectItem>> {
    let mut out = Vec::new();
    for kind in ItemKind::ALL {
        let sql = format!(
            "SELECT {ITEM_COLS} FROM {} WHERE project_id = $1 AND deleted_at IS NULL ORDER BY slug",
            kind.table()
        );
        let rows = tx.query(&sql, &[&project]).await?;
        out.extend(rows.iter().map(|r| item_row(kind, r)));
    }
    Ok(out)
}

/// Writes `w` in `tx` (see [`Scoped::put_project_item`]).
pub(super) async fn put_item_in(tx: &Tx, project: Uuid, w: ItemWrite<'_>) -> Result<ProjectItem> {
    match write_row(tx, project, w).await {
        Ok(Some(r)) => Ok(item_row(w.kind, &r)),
        Ok(None) => {
            let base = w.base.expect("an INSERT … RETURNING answers its row");
            let what = format!("{} \"{}\"", w.kind.as_str(), w.slug);
            Err(stale(tx, w.kind.table(), &what, base).await)
        }
        Err(e) if is_unique_violation(&e) => Err(SlugTaken {
            kind: w.kind.as_str(),
            slug: w.slug.to_string(),
        }
        .into()),
        Err(e) => Err(e),
    }
}

/// Inserts `w`'s row, or updates the row its base names when the revision still holds.
async fn write_row(tx: &Tx, project: Uuid, w: ItemWrite<'_>) -> Result<Option<Row>> {
    let t = w.kind.table();
    let Some(b) = w.base else {
        let sql = format!(
            "INSERT INTO {t} (project_id, slug, body) VALUES ($1, $2, $3) RETURNING {ITEM_COLS}"
        );
        return tx.query_opt(&sql, &[&project, &w.slug, w.body]).await;
    };
    let sql = format!(
        "UPDATE {t} SET slug = $3, body = $4, revision = revision + 1, updated_at = now()
          WHERE id = $1 AND project_id = $2 AND revision = $5 AND deleted_at IS NULL
          RETURNING {ITEM_COLS}"
    );
    tx.query_opt(&sql, &[&b.id, &project, &w.slug, w.body, &b.revision])
        .await
}

/// Marks item `base.id` deleted in `tx` (see [`Scoped::delete_project_item`]).
pub(super) async fn delete_item_in(
    tx: &Tx,
    project: Uuid,
    kind: ItemKind,
    base: Base,
) -> Result<ProjectItem> {
    let t = kind.table();
    let sql = format!(
        "UPDATE {t} SET deleted_at = now(), revision = revision + 1, updated_at = now()
          WHERE id = $1 AND project_id = $2 AND revision = $3 AND deleted_at IS NULL
          RETURNING {ITEM_COLS}"
    );
    match tx
        .query_opt(&sql, &[&base.id, &project, &base.revision])
        .await?
    {
        Some(r) => Ok(item_row(kind, &r)),
        None => Err(stale(tx, t, &format!("{} {}", kind.as_str(), base.id), base).await),
    }
}

/// [`Stale`] for the row of `table` that `base` names, with its live revision now (an item marked
/// deleted has none).
async fn stale(tx: &Tx, table: &str, what: &str, base: Base) -> anyhow::Error {
    let sql = match table {
        "projects" => "SELECT revision FROM projects WHERE id = $1".to_string(),
        t => format!("SELECT revision FROM {t} WHERE id = $1 AND deleted_at IS NULL"),
    };
    let current = match tx.query_opt(&sql, &[&base.id]).await {
        Ok(row) => row.map(|r| r.get(0)),
        Err(e) => return e,
    };
    Stale {
        what: what.to_string(),
        based_on: base.revision,
        current,
    }
    .into()
}

/// True when `e` is Postgres refusing a duplicate key.
fn is_unique_violation(e: &anyhow::Error) -> bool {
    e.downcast_ref::<tokio_postgres::Error>()
        .and_then(tokio_postgres::Error::code)
        == Some(&SqlState::UNIQUE_VIOLATION)
}

fn item_row(kind: ItemKind, r: &Row) -> ProjectItem {
    ProjectItem {
        id: r.get(0),
        kind,
        slug: r.get(1),
        body: r.get(2),
        revision: r.get(3),
        updated_at: r.get(4),
    }
}

/// A project's settings as query parameters `$1` (its id) to `$9`.
struct SettingsParams<'a> {
    id: Uuid,
    s: &'a ProjectFile,
    edit: &'static str,
    spend: &'static str,
    per_day: Option<f64>,
    model: Option<&'a str>,
    provider: Option<&'a str>,
}

fn settings_params(id: Uuid, s: &ProjectFile) -> SettingsParams<'_> {
    let (spend, per_day) = match s.permissions.spend {
        SpendPolicy::AlwaysAsk => ("always_ask", None),
        SpendPolicy::UnderCap { usd_per_day } => ("under_cap", Some(usd_per_day)),
    };
    let edit = match s.permissions.edit {
        EditPolicy::DiffFirst => "diff_first",
        EditPolicy::ApplyThenTell => "apply_then_tell",
    };
    let assistant = s.assistant.as_ref();
    SettingsParams {
        id,
        s,
        edit,
        spend,
        per_day,
        model: assistant.map(|a| a.model.as_str()),
        provider: assistant.and_then(|a| a.provider.as_deref()),
    }
}

impl SettingsParams<'_> {
    fn refs(&self) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
        let d = &self.s.defaults;
        vec![
            &self.id,
            &self.s.name,
            &d.world,
            &d.cost_cap_usd,
            &self.edit,
            &self.spend,
            &self.per_day,
            &self.model,
            &self.provider,
        ]
    }
}

async fn settings_query(tx: &Tx, sql: &str, id: Uuid, s: &ProjectFile) -> Result<Option<Row>> {
    let p = settings_params(id, s);
    tx.query_opt(sql, &p.refs()).await
}

fn project_row(r: &Row) -> ProjectRow {
    let spend = match r.get::<_, Option<f64>>(6) {
        Some(usd_per_day) => SpendPolicy::UnderCap { usd_per_day },
        None => SpendPolicy::AlwaysAsk,
    };
    let edit = match r.get::<_, &str>(4) {
        "apply_then_tell" => EditPolicy::ApplyThenTell,
        _ => EditPolicy::DiffFirst,
    };
    let model: Option<String> = r.get(7);
    let settings = ProjectFile {
        format: "shakedown-project/1".to_string(),
        name: r.get(1),
        defaults: Defaults {
            world: r.get(2),
            cost_cap_usd: r.get(3),
        },
        permissions: Permissions { edit, spend },
        assistant: model.map(|model| AssistantSettings {
            model,
            provider: r.get(8),
        }),
    };
    ProjectRow {
        id: r.get(0),
        settings,
        revision: r.get(9),
        updated_at: r.get(10),
    }
}
