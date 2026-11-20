//! `GET /api/me`: who is calling, in which org, and what this shaked offers them.

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use super::ApiResult;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{identity, Caller, Scope},
    mode::Mode,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/me", get(me))
}

/// The caller's user (`{id, name, email}`, null for none) and their org's feature flags.
async fn user_and_features(
    state: &AppState,
    caller: &Scope,
    org: Uuid,
) -> anyhow::Result<(Value, Vec<String>)> {
    let tx = state.store.tx(caller).await?;
    let user = match caller.user() {
        Some(u) => {
            tx.query_opt(
                "SELECT id, name, email FROM identity.users WHERE id = $1",
                &[&u],
            )
            .await?
        }
        None => None,
    };
    let row = tx
        .query_one("SELECT features FROM identity.orgs WHERE id = $1", &[&org])
        .await?;
    tx.commit().await?;
    let user = user.map(|r| json!({"id": r.get::<_, Uuid>(0), "name": r.get::<_, String>(1), "email": r.get::<_, String>(2)}));
    Ok((user.unwrap_or(Value::Null), row.get(0)))
}

/// `{mode, user: {id, name, email}, org, role, orgs (hosted only), features, switch_org_url?}`.
async fn me(State(state): State<Arc<AppState>>, Caller(caller): Caller) -> ApiResult<Value> {
    let org = identity::org_of(&state.store, &caller).await?;
    let (user, features) = user_and_features(&state, &caller, org.id).await?;
    let mode = match state.config.mode {
        Mode::Local => "local",
        Mode::Hosted => "hosted",
    };
    let mut v = json!({"mode": mode, "user": user, "role": caller.role().as_str(), "org": org, "features": features});
    if state.config.mode == Mode::Hosted {
        let orgs = match caller.user() {
            Some(u) => identity::orgs_of_user(&state.store, u).await?,
            None => vec![],
        };
        v["orgs"] = json!(orgs);
        if let Some(url) = &state.config.switch_org_url {
            v["switch_org_url"] = json!(url);
        }
    }
    Ok(Json(v))
}
