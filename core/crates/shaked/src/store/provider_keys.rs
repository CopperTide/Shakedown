//! Your model keys and OpenAI-compatible servers: each secret sealed to its row
//! ([`crate::vault::key_context`]), lists without it, one key per name (checked under an advisory
//! lock: the base schema has no name unique), and every change audited through the layer.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio_postgres::Row;
use uuid::Uuid;

use super::{Scoped, Tx};
use crate::layer::AuditEvent;
use crate::vault::{key_context, Vault, Zeroed};

/// What a key is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyKind {
    Anthropic,
    OpenAi,
    OpenRouter,
    Bedrock,
    /// Any OpenAI-compatible server (vLLM, Ollama, LM Studio, a RunPod pod).
    Custom,
}

impl KeyKind {
    pub const ALL: [KeyKind; 5] = [
        KeyKind::Anthropic,
        KeyKind::OpenAi,
        KeyKind::OpenRouter,
        KeyKind::Bedrock,
        KeyKind::Custom,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            KeyKind::Anthropic => "anthropic",
            KeyKind::OpenAi => "openai",
            KeyKind::OpenRouter => "openrouter",
            KeyKind::Bedrock => "bedrock",
            KeyKind::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<KeyKind> {
        KeyKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// A key to add.
#[derive(Debug, Clone, Copy)]
pub struct NewKey<'a> {
    pub name: &'a str,
    pub kind: KeyKind,
    pub secret: &'a str,
    /// A custom server's URL; Bedrock's region; else an optional other server for the provider.
    pub base_url: Option<&'a str>,
    pub model: Option<&'a str>,
    pub prices: Option<&'a Value>,
    /// What lists show of the secret.
    pub last4: &'a str,
}

/// A key as lists show it: never the secret.
#[derive(Debug, Clone, Serialize)]
pub struct KeyRow {
    pub id: Uuid,
    pub name: String,
    pub kind: KeyKind,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub key_last4: String,
    pub checked_ok: Option<bool>,
    pub checked_at: Option<DateTime<Utc>>,
    pub check_detail: String,
    pub created_at: DateTime<Utc>,
}

/// A key with its secret opened, for the gateway and Test only.
pub struct OpenKey {
    pub row: KeyRow,
    pub secret: Zeroed,
}

impl OpenKey {
    /// The secret as text (keys are text when added).
    pub fn secret_text(&self) -> &str {
        std::str::from_utf8(&self.secret).unwrap_or_default()
    }
}

/// Another key already has the name.
#[derive(Debug, thiserror::Error)]
#[error("a key named \"{0}\" already exists")]
pub struct NameTaken(pub String);

const COLS: &str =
    "id, name, kind, base_url, model, key_last4, checked_ok, checked_at, check_detail, created_at";
const LOCK: &str = "SELECT pg_advisory_xact_lock(hashtext('provider_keys:name:' || $1))";
const TAKEN: &str = "SELECT 1 FROM provider_keys WHERE name = $1";
const INSERT: &str =
    "INSERT INTO provider_keys (id, name, kind, base_url, model, sealed_key, key_last4, prices)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)";
const REPLACE: &str = "UPDATE provider_keys SET sealed_key = $2, key_last4 = $3, checked_ok = NULL,
     checked_at = NULL, check_detail = '' WHERE id = $1 RETURNING name";
const DELETE: &str = "DELETE FROM provider_keys WHERE id = $1 RETURNING name";
const CHECKED: &str =
    "UPDATE provider_keys SET checked_ok = $2, checked_at = now(), check_detail = $3
     WHERE id = $1";

fn key_row(r: &Row) -> KeyRow {
    let kind: String = r.get(2);
    KeyRow {
        id: r.get(0),
        name: r.get(1),
        kind: KeyKind::parse(&kind).unwrap_or(KeyKind::Custom),
        base_url: r.get(3),
        model: r.get(4),
        key_last4: r.get(5),
        checked_ok: r.get(6),
        checked_at: r.get(7),
        check_detail: r.get(8),
        created_at: r.get(9),
    }
}

impl Scoped<'_> {
    /// Adds a key, its secret sealed to the new row; [`NameTaken`] when the name is used.
    pub async fn add_key(&self, v: &Vault, k: NewKey<'_>) -> Result<KeyRow> {
        let id = Uuid::new_v4();
        let sealed = v.seal(&key_context(id), k.secret.as_bytes());
        let prices = k.prices.cloned().unwrap_or_else(|| json!({}));
        let tx = self.store.tx(self.caller).await?;
        tx.execute(LOCK, &[&k.name]).await?;
        if tx.query_opt(TAKEN, &[&k.name]).await?.is_some() {
            return Err(NameTaken(k.name.to_string()).into());
        }
        let kind = k.kind.as_str();
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 8] = [
            &id,
            &k.name,
            &kind,
            &k.base_url,
            &k.model,
            &sealed,
            &k.last4,
            &prices,
        ];
        tx.execute(INSERT, &params).await?;
        let detail = json!({"name": k.name, "kind": kind, "last4": k.last4});
        self.audit_key(&tx, "provider_key.added", id, detail)
            .await?;
        let row = one_key(&tx, id).await?.expect("the key just added");
        tx.commit().await?;
        Ok(row)
    }

    /// Replaces key `id`'s secret (its last check is forgotten): false when there is no such key.
    pub async fn replace_key_secret(
        &self,
        v: &Vault,
        id: Uuid,
        secret: &str,
        last4: &str,
    ) -> Result<bool> {
        let sealed = v.seal(&key_context(id), secret.as_bytes());
        let tx = self.store.tx(self.caller).await?;
        let Some(row) = tx.query_opt(REPLACE, &[&id, &sealed, &last4]).await? else {
            return Ok(false);
        };
        let detail = json!({"name": row.get::<_, String>(0), "last4": last4});
        self.audit_key(&tx, "provider_key.replaced", id, detail)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Every key, newest first.
    pub async fn list_keys(&self) -> Result<Vec<KeyRow>> {
        let sql = format!("SELECT {COLS} FROM provider_keys ORDER BY created_at DESC, id");
        Ok(self.query(&sql, &[]).await?.iter().map(key_row).collect())
    }

    /// Deletes key `id`: false when there is no such key.
    pub async fn delete_key(&self, id: Uuid) -> Result<bool> {
        let tx = self.store.tx(self.caller).await?;
        let Some(row) = tx.query_opt(DELETE, &[&id]).await? else {
            return Ok(false);
        };
        let detail = json!({"name": row.get::<_, String>(0)});
        self.audit_key(&tx, "provider_key.deleted", id, detail)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Every key with its secret opened, oldest first (a newer key of a provider wins).
    pub async fn open_keys(&self, v: &Vault) -> Result<Vec<OpenKey>> {
        let sql = format!("SELECT {COLS}, sealed_key FROM provider_keys ORDER BY created_at, id");
        let rows = self.query(&sql, &[]).await?;
        rows.iter().map(|r| open_row(v, r)).collect()
    }

    /// Key `id` with its secret opened.
    pub async fn open_key(&self, v: &Vault, id: Uuid) -> Result<Option<OpenKey>> {
        let sql = format!("SELECT {COLS}, sealed_key FROM provider_keys WHERE id = $1");
        let row = self.query_opt(&sql, &[&id]).await?;
        row.map(|r| open_row(v, &r)).transpose()
    }

    /// Records the outcome of a Test of key `id` (`detail` already without the key).
    pub async fn record_key_check(
        &self,
        id: Uuid,
        ok: bool,
        detail: &str,
    ) -> Result<Option<KeyRow>> {
        let tx = self.store.tx(self.caller).await?;
        tx.execute(CHECKED, &[&id, &ok, &detail]).await?;
        let row = one_key(&tx, id).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn audit_key(
        &self,
        tx: &Tx,
        action: &'static str,
        id: Uuid,
        detail: Value,
    ) -> Result<()> {
        let id = id.to_string();
        let e = AuditEvent {
            action,
            target: ("provider_key", &id),
            detail,
        };
        self.store.layer().audit(tx, self.caller, e).await
    }
}

async fn one_key(tx: &Tx, id: Uuid) -> Result<Option<KeyRow>> {
    let sql = format!("SELECT {COLS} FROM provider_keys WHERE id = $1");
    Ok(tx.query_opt(&sql, &[&id]).await?.as_ref().map(key_row))
}

fn open_row(v: &Vault, r: &Row) -> Result<OpenKey> {
    let row = key_row(r);
    let sealed: Vec<u8> = r.get(10);
    let secret = v
        .open(&key_context(row.id), &sealed)
        .map_err(|e| anyhow::anyhow!("key \"{}\": {e}", row.name))?;
    Ok(OpenKey { row, secret })
}
