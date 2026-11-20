//! `GET /api/usage/org`: the org's usage by month (owners, admins and billing), or a member's
//! own (what they started).

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use super::ApiResult;
use crate::{
    app::AppState,
    auth::{can, Action, Caller, Target},
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/usage/org", get(org_usage))
}

/// `{scope: "org" | "own", months: [{month, meter, quantity}]}`.
async fn org_usage(State(state): State<Arc<AppState>>, Caller(caller): Caller) -> ApiResult<Value> {
    let records = state.store.at(&caller);
    let whole = can(
        caller.principal(),
        &Action::Usage,
        &Target::Org(caller.org()),
        &Default::default(),
    );
    let (scope, rows) = match (whole, caller.user()) {
        (true, _) => ("org", records.usage_by_month().await?),
        (false, Some(_)) if caller.role() != crate::auth::OrgRole::Viewer => {
            ("own", records.own_usage_by_month().await?)
        }
        _ => {
            return Err(super::ApiError::forbidden(
                "your role in this org may not see its usage",
            ))
        }
    };
    let months: Vec<Value> = rows
        .iter()
        .map(|(m, meter, q)| json!({"month": m, "meter": meter.as_str(), "quantity": q}))
        .collect();
    Ok(Json(json!({"scope": scope, "months": months})))
}
