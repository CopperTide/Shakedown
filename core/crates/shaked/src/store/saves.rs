//! World saves uploaded to object storage: a row per save, naming its store and the prefix of
//! its objects (never a URL). Fork-ready when `state = 'ready'`; kept 30 days after it is
//! recorded (D19) unless a later layer hook says otherwise.

use std::time::Duration;

use anyhow::Result;
use uuid::Uuid;

use super::{Scoped, Store};
use crate::{
    auth::SystemScope,
    objects::{copy::SaveOnStore, ObjectKey, Stored},
};

/// How long a save is kept by default.
pub const SAVE_RETENTION: Duration = Duration::from_secs(30 * 86_400);

/// A save to record, its objects already stored.
pub struct NewSave<'a> {
    pub run: &'a str,
    pub branch: &'a str,
    pub integration: &'a str,
    pub save_id: &'a str,
    pub seq: i64,
    /// The prefix of its objects ([`crate::objects::keys::save`]).
    pub prefix: &'a ObjectKey,
    /// Its object (one in M1): where, its size and checksum.
    pub stored: &'a Stored,
    pub retention: Duration,
}

/// A save as listed.
#[derive(Debug, Clone)]
pub struct SaveRow {
    pub id: Uuid,
    pub integration: String,
    pub save_id: String,
    pub store: String,
    pub object_key: String,
    pub bytes: Option<i64>,
    pub sha256: Option<String>,
    pub state: String,
}

const INSERT: &str =
    "INSERT INTO saves (run_id, branch_id, integration, save_id, seq, store, object_key,
     bytes, sha256, state, ready_at, expires_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'ready', now(), now() + make_interval(secs => $10))
     RETURNING id";
const LIST: &str = "SELECT id, integration, save_id, store, object_key, bytes, sha256, state
     FROM saves ORDER BY created_at, id";

impl Scoped<'_> {
    /// Records a stored save, ready to fork from.
    pub async fn record_save(&self, s: NewSave<'_>) -> Result<Uuid> {
        let bytes = s.stored.bytes as i64;
        let secs = s.retention.as_secs_f64();
        let (store, key) = (s.stored.r.store.as_str(), s.prefix.as_str());
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 10] = [
            &s.run,
            &s.branch,
            &s.integration,
            &s.save_id,
            &s.seq,
            &store,
            &key,
            &bytes,
            &s.stored.sha256,
            &secs,
        ];
        Ok(self.query_one(INSERT, &params).await?.get(0))
    }

    /// Every save, oldest first.
    pub async fn saves(&self) -> Result<Vec<SaveRow>> {
        let rows = self.query(LIST, &[]).await?;
        Ok(rows
            .iter()
            .map(|r| SaveRow {
                id: r.get(0),
                integration: r.get(1),
                save_id: r.get(2),
                store: r.get(3),
                object_key: r.get(4),
                bytes: r.get(5),
                sha256: r.get(6),
                state: r.get(7),
            })
            .collect())
    }
}

/// The live saves on store `from`, for a copy (a system job: every owner's).
pub(crate) async fn saves_on(store: &Store, from: &str) -> Result<Vec<SaveOnStore>> {
    let tx = store.system_tx(SystemScope::store_copy()).await?;
    let sql = "SELECT id, object_key, sha256 FROM saves WHERE store = $1 AND state <> 'deleted'";
    let rows = tx.query(sql, &[&from]).await?;
    tx.commit().await?;
    Ok(rows
        .iter()
        .map(|r| SaveOnStore {
            id: r.get(0),
            prefix: r.get(1),
            sha256: r.get(2),
        })
        .collect())
}

/// Points saves `ids` at store `to` (those still on `from`): how many moved.
pub(crate) async fn repoint_saves(
    store: &Store,
    ids: &[Uuid],
    from: &str,
    to: &str,
) -> Result<u64> {
    let tx = store.system_tx(SystemScope::store_copy()).await?;
    let sql = "UPDATE saves SET store = $3 WHERE id = ANY($1) AND store = $2";
    let n = tx.execute(sql, &[&ids, &from, &to]).await?;
    tx.commit().await?;
    Ok(n)
}
