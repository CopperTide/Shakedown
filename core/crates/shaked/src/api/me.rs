//! `GET /api/me`: who is calling. The open-source shaked serves one person: you.

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::app::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/me", get(me))
}

/// `{mode: "local", user: {name: "You"}, features: []}`.
async fn me() -> Json<Value> {
    Json(json!({"mode": "local", "user": {"name": "You"}, "features": []}))
}
