//! `POST /api/assistant/chat`: the Ask drawer's questions, answered by the assistant on the
//! caller's own model keys.

use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};

use super::{ApiError, ApiResult, JsonBody};
use crate::{
    app::AppState,
    assistant::{run_chat, ChatAnswer, ChatRequest},
    auth::{Caller, Who},
    ops::OpCtx,
    providers::Provider,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/assistant/chat", post(chat))
}

const NO_MODEL: &str = "The assistant needs a model: add a key in Connections & models";

async fn chat(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    JsonBody(req): JsonBody<ChatRequest>,
) -> ApiResult<ChatAnswer> {
    let (provider, model) = model_for(&state, &caller).await?;
    let ctx = OpCtx::as_caller(&state, caller);
    Ok(Json(
        run_chat(&ctx, &provider, &state.http, &model, req).await?,
    ))
}

/// The assistant's provider and model: the project's settings, else the default provider's
/// default model. 409 when there is none to call.
async fn model_for(state: &AppState, caller: &Caller) -> Result<(Provider, String), ApiError> {
    let project = state.project_for(caller).current().await;
    let settings = project.ok().and_then(|p| p.config.assistant);
    let config = super::providers_of(state, caller).await;
    let no_model = || ApiError::conflict(NO_MODEL);
    let provider = config
        .resolve(settings.as_ref().and_then(|s| s.provider.as_deref()))
        .map_err(|_| no_model())?;
    if provider.unavailable().is_some() {
        return Err(no_model());
    }
    let model = settings
        .map(|s| s.model)
        .or_else(|| provider.default_model().map(str::to_string));
    Ok((provider, model.ok_or_else(no_model)?))
}
