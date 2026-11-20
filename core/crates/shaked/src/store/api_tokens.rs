//! API tokens in the metadata database: made for the scope's user, listed without their hash,
//! revoked; each change audited.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use super::Scoped;
use crate::auth::{
    audit::audit,
    tokens::{hash, new_secret, prefix_of, Minted, API_PREFIX},
    TokenScope,
};

/// A token as lists show it: never the token or its hash.
#[derive(Debug, Clone, Serialize)]
pub struct ApiTokenRow {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub team_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

const INSERT: &str = "INSERT INTO api_tokens (org_id, user_id, team_id, name, token_hash, token_prefix, scopes, created_by, expires_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $2, $8) RETURNING id";
const LIST: &str =
    "SELECT id, name, token_prefix, scopes, team_id, created_at, last_used_at, expires_at
     FROM api_tokens WHERE org_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC, id";

impl Scoped<'_> {
    /// Makes a token for the scope's user: the token, shown once.
    pub async fn create_api_token(
        &self,
        name: &str,
        scopes: &[TokenScope],
        team: Option<Uuid>,
        expires: Option<DateTime<Utc>>,
    ) -> Result<Minted> {
        let user = self.user().context("only a person holds API tokens here")?;
        let token = new_secret(API_PREFIX);
        let prefix = prefix_of(&token);
        let scopes: Vec<&str> = scopes.iter().map(|s| s.as_str()).collect();
        let tx = self.store.tx(self.scope).await?;
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 8] = [
            &self.org(),
            &user,
            &team,
            &name,
            &hash(&token),
            &prefix,
            &scopes,
            &expires,
        ];
        let id: Uuid = tx.query_one(INSERT, &params).await?.get(0);
        let detail = json!({"name": name, "prefix": prefix, "scopes": scopes});
        audit(
            &tx,
            self.scope,
            "token.created",
            ("api_token", &id.to_string()),
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(Minted { id, token, prefix })
    }

    /// The org's live tokens the scope may see (its own; an admin's: all), newest first.
    pub async fn list_api_tokens(&self) -> Result<Vec<ApiTokenRow>> {
        let rows = self.query(LIST, &[&self.org()]).await?;
        Ok(rows
            .iter()
            .map(|r| ApiTokenRow {
                id: r.get(0),
                name: r.get(1),
                prefix: r.get(2),
                scopes: r.get(3),
                team_id: r.get(4),
                created_at: r.get(5),
                last_used_at: r.get(6),
                expires_at: r.get(7),
            })
            .collect())
    }

    /// Revokes token `id`: false when the scope sees no such live token.
    pub async fn revoke_api_token(&self, id: Uuid) -> Result<bool> {
        let tx = self.store.tx(self.scope).await?;
        let sql = "UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND org_id = $2 AND revoked_at IS NULL
                   RETURNING token_prefix";
        let Some(row) = tx.query_opt(sql, &[&id, &self.org()]).await? else {
            return Ok(false);
        };
        let detail = json!({"prefix": row.get::<_, String>(0)});
        audit(
            &tx,
            self.scope,
            "token.revoked",
            ("api_token", &id.to_string()),
            detail,
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}
