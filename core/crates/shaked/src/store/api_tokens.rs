//! API tokens in the metadata database: made here (the secret is shown once), listed without their
//! hash, revoked; each change audited through the layer, in the transaction that made it.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::Scoped;
use crate::auth::{
    tokens::{hash, new_secret, prefix_of, Minted, API_PREFIX},
    TokenScope,
};
use crate::layer::AuditEvent;

/// A token as lists show it: never the token or its hash.
#[derive(Debug, Clone, Serialize)]
pub struct ApiTokenRow {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

const INSERT: &str = "INSERT INTO api_tokens (name, token_hash, token_prefix, scopes, expires_at)
     VALUES ($1, $2, $3, $4, $5) RETURNING id";
const LIST: &str = "SELECT id, name, token_prefix, scopes, created_at, last_used_at, expires_at
     FROM api_tokens WHERE revoked_at IS NULL ORDER BY created_at DESC, id";
const REVOKE: &str =
    "UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL RETURNING token_prefix";

impl Scoped<'_> {
    /// Makes a token: the token, shown once.
    pub async fn create_api_token(
        &self,
        name: &str,
        scopes: &[TokenScope],
        expires: Option<DateTime<Utc>>,
    ) -> Result<Minted> {
        let token = new_secret(API_PREFIX);
        let prefix = prefix_of(&token);
        let scopes: Vec<&str> = scopes.iter().map(|s| s.as_str()).collect();
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 5] =
            [&name, &hash(&token), &prefix, &scopes, &expires];
        let tx = self.store.tx(self.caller).await?;
        let id: Uuid = tx.query_one(INSERT, &params).await?.get(0);
        let detail = json!({"name": name, "prefix": prefix, "scopes": scopes});
        self.audit(&tx, "token.created", &id, detail).await?;
        tx.commit().await?;
        Ok(Minted { id, token, prefix })
    }

    /// The live tokens, newest first.
    pub async fn list_api_tokens(&self) -> Result<Vec<ApiTokenRow>> {
        let rows = self.query(LIST, &[]).await?;
        Ok(rows
            .iter()
            .map(|r| ApiTokenRow {
                id: r.get(0),
                name: r.get(1),
                prefix: r.get(2),
                scopes: r.get(3),
                created_at: r.get(4),
                last_used_at: r.get(5),
                expires_at: r.get(6),
            })
            .collect())
    }

    /// Revokes token `id`: false when there is no such live token.
    pub async fn revoke_api_token(&self, id: Uuid) -> Result<bool> {
        let tx = self.store.tx(self.caller).await?;
        let Some(row) = tx.query_opt(REVOKE, &[&id]).await? else {
            return Ok(false);
        };
        let detail = json!({"prefix": row.get::<_, String>(0)});
        self.audit(&tx, "token.revoked", &id, detail).await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn audit(
        &self,
        tx: &crate::store::Tx,
        action: &'static str,
        id: &Uuid,
        detail: serde_json::Value,
    ) -> Result<()> {
        let id = id.to_string();
        let e = AuditEvent {
            action,
            target: ("api_token", &id),
            detail,
        };
        self.store.layer().audit(tx, self.caller, e).await
    }
}
