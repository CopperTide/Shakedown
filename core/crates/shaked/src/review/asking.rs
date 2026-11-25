//! Asking about a step, end to end: the question checked, the explainer's model resolved on the
//! asker's own key, the question stored (`running`), then the model asked on a task of its own and
//! its checked answer stored under the question. Stored with the reviews (kind `ask`), so a
//! hosting layer scopes questions as it scopes reviews. What the explainer reads is
//! [`super::ask`].

use std::sync::Arc;

use serde_json::{json, Value};

use super::{
    ask::{check, context, AskContext, MAX_QUESTION},
    prompt::ask_prompt,
    reviewer,
    settings::{availability, settings, ReviewerModel},
    ticket, Checked,
};
use crate::{
    api::ApiError,
    app::AppState,
    auth::Caller,
    layer::KeySource,
    providers::{keys, Provider},
    store::StoredReview,
};

/// Why the explainer may not run on a key that is not the asker's.
pub const NOT_YOURS: &str = "the explainer runs on your own model key only: add one under Models";

/// The key prefix of a branch's questions.
pub fn branch_prefix(branch: &str) -> String {
    format!("ask:{branch}:")
}

/// The key prefix of one step's questions.
pub fn step_prefix(branch: &str, seq: i64) -> String {
    format!("{}{seq}:", branch_prefix(branch))
}

/// A question about step `seq` of `branch`, to `model` (blank: the review model).
pub struct Question<'a> {
    pub branch: &'a str,
    pub seq: i64,
    pub question: &'a str,
    pub model: Option<&'a str>,
}

/// The model that answers, with the key it runs on.
pub enum Explainer {
    ClaudeCode(String),
    Provider(Provider, String),
}

/// Stores the question and starts answering it: the stored row (`running`) and the task that
/// answers. 400 for a blank or long question or a bad model, 404 for no such branch or step,
/// 409 when asking is off or the model would not run on the asker's own key.
pub async fn start(
    state: &Arc<AppState>,
    caller: &Arc<Caller>,
    q: Question<'_>,
) -> Result<(StoredReview, tokio::task::JoinHandle<()>), ApiError> {
    let model = checked(state, &q)?;
    let (branch, ctx) = step_context(state, caller, &q).await?;
    let explainer = explainer(state, caller, &model).await?;
    let asked = json!({
        "question": ctx.question,
        "step": q.seq,
        "agent": ctx.step.agent,
        "asked_by": state.store.layer().principal(caller),
    });
    let key = format!(
        "{}{}",
        step_prefix(&branch, q.seq),
        uuid::Uuid::new_v4().simple()
    );
    let row = state
        .store
        .at(caller)
        .start_ask(&key, &model.label(), &asked)
        .await?;
    let (st, c) = (state.clone(), caller.clone());
    let task = tokio::spawn(async move {
        let outcome = answer(&st, &explainer, &ctx, asked).await;
        super::settle(&st, &c, &key, outcome).await;
    });
    Ok((row, task))
}

/// The model to ask, once asking is on and the question is one.
fn checked(state: &AppState, q: &Question<'_>) -> Result<ReviewerModel, ApiError> {
    let s = settings(&state.config.providers);
    if !s.ask {
        let why = s.why.as_deref().unwrap_or("off");
        return Err(ApiError::conflict(format!(
            "asking about a step is off: {why}"
        )));
    }
    let question = q.question.trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION {
        return Err(ApiError::bad(format!(
            "ask a question of 1 to {MAX_QUESTION} characters"
        )));
    }
    let label = q
        .model
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or(&s.model);
    ReviewerModel::parse(label).map_err(ApiError::bad)
}

/// The branch's id and what the explainer reads about the step: 404 for no such branch or step.
async fn step_context(
    state: &AppState,
    caller: &Caller,
    q: &Question<'_>,
) -> Result<(String, AskContext), ApiError> {
    let records = state.store.at(caller);
    let branch = (records.find_branch(q.branch).await?)
        .ok_or_else(|| ApiError::missing("branch", q.branch))?;
    let events = records.events(&branch.id).await?;
    let ticket = ticket(state, &events).await;
    let ctx = context(
        &events,
        q.seq,
        &branch.team,
        ticket.as_deref(),
        q.question.trim(),
    )
    .ok_or_else(|| ApiError::not_found(format!("there is no step {} in this branch", q.seq)))?;
    Ok((branch.id, ctx))
}

/// `model` on the asker's own key. Claude Code and the environment's keys are the asker's only
/// where the layer lends them (personal use); the layer's fallback key never is.
async fn explainer(
    state: &AppState,
    caller: &Caller,
    model: &ReviewerModel,
) -> Result<Explainer, ApiError> {
    let lends = state.store.layer().policy().environment_keys;
    match model {
        ReviewerModel::ClaudeCode { model: m } => {
            if !lends {
                return Err(ApiError::conflict(NOT_YOURS));
            }
            availability(model, &state.config.providers).map_err(ApiError::conflict)?;
            Ok(Explainer::ClaudeCode(m.clone()))
        }
        ReviewerModel::Provider { provider, model } => {
            let (found, source) = keys::provider_for(state, caller, Some(provider), model).await;
            let yours = match source {
                KeySource::Own(_) => true,
                KeySource::Environment => lends,
                KeySource::Fallback(_) => false,
            };
            if !yours {
                return Err(ApiError::conflict(NOT_YOURS));
            }
            let p = found.map_err(ApiError::conflict)?;
            if let Some(u) = p.unavailable() {
                return Err(ApiError::conflict(u.why()));
            }
            Ok(Explainer::Provider(p, model.clone()))
        }
    }
}

/// Asks the explainer: its answer checked, with the question.
async fn answer(
    state: &AppState,
    explainer: &Explainer,
    ctx: &AskContext,
    asked: Value,
) -> anyhow::Result<Checked> {
    let prompt = ask_prompt(ctx);
    let a = match explainer {
        Explainer::ClaudeCode(m) => reviewer::ask_claude(m, &prompt).await?,
        Explainer::Provider(p, m) => reviewer::ask_provider(p, m, &prompt, &state.http).await?,
    };
    let (explained, issues) = check(&a.value, ctx)?;
    let mut stored = asked;
    if let (Value::Object(to), Value::Object(from)) =
        (&mut stored, serde_json::to_value(explained)?)
    {
        to.extend(from);
        to.insert("sources".into(), json!(ctx.sources.len()));
        to.insert("sources_left_out".into(), json!(ctx.sources_left_out));
    }
    Ok(Checked {
        review: stored,
        issues,
        answer: a,
        bytes: prompt.user.len(),
    })
}

/// A stored question as listed: with `mine` when `caller` asked it.
pub fn listed(state: &AppState, caller: &Caller, mut r: StoredReview) -> Value {
    let me = state.store.layer().principal(caller);
    let mine = r.review.as_ref().and_then(|v| v["asked_by"].as_str()) == Some(me.as_str());
    if let Some(Value::Object(v)) = r.review.as_mut() {
        v.insert("mine".into(), json!(mine));
    }
    serde_json::to_value(r).unwrap_or(Value::Null)
}
