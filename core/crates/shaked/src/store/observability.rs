//! Where runs are exported (Settings → Observability): each target's headers sealed to its row
//! ([`target_context`]), lists without them (only their names), one target per name (checked under
//! an advisory lock, as for model keys), and every change audited through the layer. The exports
//! themselves are in [`super::obs_exports`].

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_postgres::Row;
use uuid::Uuid;

use super::{NameTaken, Scoped, Tx};
use crate::layer::AuditEvent;
use crate::vault::Vault;

/// Queues a new branch (`$1`) for every target that exports each branch: sent once it ends.
pub(super) const ENQUEUE_NEW: &str = "INSERT INTO observability_exports (target_id, branch_id)
     SELECT id, $1 FROM observability_targets WHERE auto_export ON CONFLICT DO NOTHING";

/// The vault context of target `id`'s headers.
pub fn target_context(id: Uuid) -> String {
    format!("observability_target:{id}:headers")
}

/// What a target is: a preset (Langfuse, Arize Phoenix) or any OTLP/HTTP collector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Langfuse,
    Phoenix,
    Otlp,
}

impl TargetKind {
    pub const ALL: [TargetKind; 3] = [TargetKind::Langfuse, TargetKind::Phoenix, TargetKind::Otlp];

    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Langfuse => "langfuse",
            TargetKind::Phoenix => "phoenix",
            TargetKind::Otlp => "otlp",
        }
    }

    pub fn parse(s: &str) -> Option<TargetKind> {
        TargetKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// What a target sends, besides where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TargetFlags {
    /// Prompts, completions, tool arguments and results (else only their shape and numbers).
    pub include_content: bool,
    /// Every new branch, once it ends (else only when asked).
    pub auto_export: bool,
    /// OTLP metrics to the same collector.
    pub export_metrics: bool,
}

/// A target to add, its endpoint and headers already worked out from its preset.
#[derive(Debug, Clone, Copy)]
pub struct NewTarget<'a> {
    pub name: &'a str,
    pub kind: TargetKind,
    pub endpoint: &'a str,
    pub project: Option<&'a str>,
    pub headers: &'a BTreeMap<String, String>,
    pub flags: TargetFlags,
}

/// A target as lists show it: never a header's value.
#[derive(Debug, Clone, Serialize)]
pub struct TargetRow {
    pub id: Uuid,
    pub name: String,
    pub kind: TargetKind,
    pub endpoint: String,
    pub project: Option<String>,
    pub header_names: Vec<String>,
    #[serde(flatten)]
    pub flags: TargetFlags,
    pub checked_ok: Option<bool>,
    pub checked_at: Option<DateTime<Utc>>,
    pub check_detail: String,
    pub created_at: DateTime<Utc>,
}

/// A target with its headers opened, for the exporter and Test only.
pub struct OpenTarget {
    pub row: TargetRow,
    pub headers: BTreeMap<String, String>,
}

impl std::fmt::Debug for OpenTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenTarget")
            .field("row", &self.row)
            .finish_non_exhaustive()
    }
}

pub(super) const COLS: &str = "id, name, kind, endpoint, project, header_names, include_content, \
     auto_export, export_metrics, checked_ok, checked_at, check_detail, created_at";
const LOCK: &str = "SELECT pg_advisory_xact_lock(hashtext('observability_targets:name:' || $1))";
const TAKEN: &str = "SELECT 1 FROM observability_targets WHERE name = $1";
const INSERT: &str = "INSERT INTO observability_targets (id, name, kind, endpoint, project,
     sealed_headers, header_names, include_content, auto_export, export_metrics)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)";
const FLAGS: &str = "UPDATE observability_targets SET include_content = $2, auto_export = $3,
     export_metrics = $4 WHERE id = $1 RETURNING name";
const DELETE: &str = "DELETE FROM observability_targets WHERE id = $1 RETURNING name";
const CHECKED: &str =
    "UPDATE observability_targets SET checked_ok = $2, checked_at = now(), check_detail = $3
     WHERE id = $1";

pub(super) fn target_row(r: &Row) -> TargetRow {
    let kind: String = r.get(2);
    TargetRow {
        id: r.get(0),
        name: r.get(1),
        kind: TargetKind::parse(&kind).unwrap_or(TargetKind::Otlp),
        endpoint: r.get(3),
        project: r.get(4),
        header_names: r.get(5),
        flags: TargetFlags {
            include_content: r.get(6),
            auto_export: r.get(7),
            export_metrics: r.get(8),
        },
        checked_ok: r.get(9),
        checked_at: r.get(10),
        check_detail: r.get(11),
        created_at: r.get(12),
    }
}

/// Target row `r` (of [`COLS`] then `sealed_headers`) with its headers opened.
pub(super) fn open_row(v: &Vault, r: &Row) -> Result<OpenTarget> {
    let row = target_row(r);
    let sealed: Option<Vec<u8>> = r.get(13);
    let headers = match sealed {
        None => BTreeMap::new(),
        Some(s) => {
            let plain = v
                .open(&target_context(row.id), &s)
                .map_err(|e| anyhow::anyhow!("target \"{}\": {e}", row.name))?;
            serde_json::from_slice(&plain)?
        }
    };
    Ok(OpenTarget { row, headers })
}

impl Scoped<'_> {
    /// Adds a target, its headers sealed to the new row; [`NameTaken`] when the name is used. A
    /// target that exports every branch is queued for none of the existing ones.
    pub async fn add_target(&self, v: &Vault, t: NewTarget<'_>) -> Result<TargetRow> {
        let id = Uuid::new_v4();
        let tx = self.store.tx(self.caller).await?;
        tx.execute(LOCK, &[&t.name]).await?;
        if tx.query_opt(TAKEN, &[&t.name]).await?.is_some() {
            return Err(NameTaken(t.name.to_string()).into());
        }
        insert(&tx, v, id, &t).await?;
        let detail = json!({"name": t.name, "kind": t.kind.as_str(), "endpoint": t.endpoint});
        self.audit_target(&tx, "observability_target.added", id, detail)
            .await?;
        let row = one_target(&tx, id).await?.expect("the target just added");
        tx.commit().await?;
        Ok(row)
    }

    /// Every target, newest first.
    pub async fn list_targets(&self) -> Result<Vec<TargetRow>> {
        let sql = format!("SELECT {COLS} FROM observability_targets ORDER BY created_at DESC, id");
        Ok(self
            .query(&sql, &[])
            .await?
            .iter()
            .map(target_row)
            .collect())
    }

    /// Target `id` with its headers opened.
    pub async fn open_target(&self, v: &Vault, id: Uuid) -> Result<Option<OpenTarget>> {
        let sql = format!("SELECT {COLS}, sealed_headers FROM observability_targets WHERE id = $1");
        let row = self.query_opt(&sql, &[&id]).await?;
        row.map(|r| open_row(v, &r)).transpose()
    }

    /// Every target with its headers opened, oldest first.
    pub async fn open_targets(&self, v: &Vault) -> Result<Vec<OpenTarget>> {
        let sql = format!(
            "SELECT {COLS}, sealed_headers FROM observability_targets ORDER BY created_at, id"
        );
        let rows = self.query(&sql, &[]).await?;
        rows.iter().map(|r| open_row(v, r)).collect()
    }

    /// Sets what target `id` sends: false when there is no such target.
    pub async fn set_target_flags(&self, id: Uuid, f: TargetFlags) -> Result<bool> {
        let tx = self.store.tx(self.caller).await?;
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 4] =
            [&id, &f.include_content, &f.auto_export, &f.export_metrics];
        let Some(row) = tx.query_opt(FLAGS, &params).await? else {
            return Ok(false);
        };
        let detail = json!({"name": row.get::<_, String>(0), "flags": f});
        self.audit_target(&tx, "observability_target.changed", id, detail)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Deletes target `id` (its exports with it): false when there is no such target.
    pub async fn delete_target(&self, id: Uuid) -> Result<bool> {
        let tx = self.store.tx(self.caller).await?;
        let Some(row) = tx.query_opt(DELETE, &[&id]).await? else {
            return Ok(false);
        };
        let detail = json!({"name": row.get::<_, String>(0)});
        self.audit_target(&tx, "observability_target.deleted", id, detail)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Records the outcome of a Test of target `id` (`detail` already without its headers).
    pub async fn record_target_check(
        &self,
        id: Uuid,
        ok: bool,
        detail: &str,
    ) -> Result<Option<TargetRow>> {
        let tx = self.store.tx(self.caller).await?;
        tx.execute(CHECKED, &[&id, &ok, &detail]).await?;
        let row = one_target(&tx, id).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn audit_target(
        &self,
        tx: &Tx,
        action: &'static str,
        id: Uuid,
        detail: Value,
    ) -> Result<()> {
        let id = id.to_string();
        let e = AuditEvent {
            action,
            target: ("observability_target", &id),
            detail,
        };
        self.store.layer().audit(tx, self.caller, e).await
    }
}

/// Inserts target `t` as row `id`, its headers sealed to it.
async fn insert(tx: &Tx, v: &Vault, id: Uuid, t: &NewTarget<'_>) -> Result<()> {
    let sealed = (!t.headers.is_empty())
        .then(|| serde_json::to_vec(t.headers))
        .transpose()?
        .map(|plain| v.seal(&target_context(id), &plain));
    let names: Vec<String> = t.headers.keys().cloned().collect();
    let (kind, f) = (t.kind.as_str(), t.flags);
    let params: [&(dyn tokio_postgres::types::ToSql + Sync); 10] = [
        &id,
        &t.name,
        &kind,
        &t.endpoint,
        &t.project,
        &sealed,
        &names,
        &f.include_content,
        &f.auto_export,
        &f.export_metrics,
    ];
    tx.execute(INSERT, &params).await?;
    Ok(())
}

async fn one_target(tx: &Tx, id: Uuid) -> Result<Option<TargetRow>> {
    let sql = format!("SELECT {COLS} FROM observability_targets WHERE id = $1");
    Ok(tx.query_opt(&sql, &[&id]).await?.as_ref().map(target_row))
}
