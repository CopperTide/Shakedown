//! `GET /api/me`: who is calling, with the layer's fields (`mode`, `features`, an account menu).

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::{app::AppState, auth::Who};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/me", get(me))
}

/// `{mode, user: {name: "You"}, features, account?}`; locally `{mode: "local", user: {name:
/// "You"}, features: []}`.
async fn me(State(state): State<Arc<AppState>>, Who(caller): Who) -> Json<Value> {
    let extra = state.store.layer().me(&caller).await;
    let mut v = json!({"mode": extra.mode, "user": {"name": "You"}, "features": extra.features});
    if let Some(account) = extra.account {
        v["account"] = account;
    }
    Json(v)
}
