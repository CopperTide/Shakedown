//! Run tokens in the metadata database: their hash only, never the token.

use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::Scoped;
use crate::auth::{
    run_tokens::prefix,
    tokens::{hash, new_secret},
    RunTokenKind,
};

impl Scoped<'_> {
    /// A new token of `kind` for `branch` of `run` (an agent token names its agent, a worker or
    /// ticket token its sandbox), good until `expires`: the token, which only its hash is kept of.
    pub async fn mint_run_token(
        &self,
        run: &str,
        branch: &str,
        kind: RunTokenKind,
        agent: Option<&str>,
        sandbox: Option<Uuid>,
        expires: DateTime<Utc>,
    ) -> Result<String> {
        let token = new_secret(prefix(kind));
        let sql = "INSERT INTO run_tokens (run_id, branch_id, kind, agent, sandbox_id, token_hash, expires_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)";
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 7] = [
            &run,
            &branch,
            &kind.as_str(),
            &agent,
            &sandbox,
            &hash(&token),
            &expires,
        ];
        self.execute(sql, &params).await?;
        Ok(token)
    }

    /// Moves `branch`'s live tokens' expiry `by` later (the branch was paused that long).
    pub async fn extend_run_tokens(&self, branch: &str, by: std::time::Duration) -> Result<u64> {
        let sql = "UPDATE run_tokens SET expires_at = expires_at + make_interval(secs => $2)
                   WHERE branch_id = $1 AND revoked_at IS NULL";
        self.execute(sql, &[&branch, &by.as_secs_f64()]).await
    }

    /// Revokes `branch`'s live tokens (only `agent`'s, when given): how many.
    pub async fn revoke_run_tokens(&self, branch: &str, agent: Option<&str>) -> Result<u64> {
        let sql = "UPDATE run_tokens SET revoked_at = now()
                   WHERE branch_id = $1 AND revoked_at IS NULL AND ($2::text IS NULL OR agent = $2)";
        self.execute(sql, &[&branch, &agent]).await
    }
}
