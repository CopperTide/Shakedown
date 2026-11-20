//! The audit log: who did what in which org (`identity.audit_log`), written in the same transaction
//! as the change. Never secrets or tokens.

use anyhow::Result;
use serde_json::Value;

use super::{Principal, Scope};
use crate::store::Tx;

/// The kind, id and label of who acts.
fn actor(p: &Principal) -> (&'static str, Option<uuid::Uuid>, String) {
    match p {
        Principal::User { user, .. } => ("user", Some(*user), user.to_string()),
        Principal::ServiceAccount { sa, .. } => ("service_account", Some(*sa), sa.to_string()),
        Principal::ApiToken { token, .. } => ("api_token", Some(*token), token.to_string()),
        Principal::RunToken { branch, .. } => ("run_token", None, format!("branch {branch}")),
    }
}

/// Records `action` (e.g. `token.created`) on `target` (kind, id) with `detail`.
pub async fn audit(
    tx: &Tx,
    scope: &Scope,
    action: &str,
    target: (&str, &str),
    detail: Value,
) -> Result<()> {
    let (kind, id, label) = actor(scope.principal());
    let sql = "INSERT INTO identity.audit_log (org_id, actor_kind, actor_id, actor_label, action, target_kind, target_id, detail)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)";
    tx.execute(
        sql,
        &[
            &scope.org(),
            &kind,
            &id,
            &label,
            &action,
            &target.0,
            &target.1,
            &detail,
        ],
    )
    .await?;
    Ok(())
}
