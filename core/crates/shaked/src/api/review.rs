//! AI review over HTTP. `POST` starts a review (or returns the stored one; `?again=1` makes a new
//! one) and answers at once with the stored row (`status: running` until the model answers);
//! `GET` reads it (404 before any). Off (`GET /api/review/settings`): `POST` is refused (409) and
//! nothing is sent to a model. What a review is of, and its job, are `review::jobs`.

use crate::auth::{Caller, Who};
use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use futures_util::future::BoxFuture;

use super::{
    compare::{compare_facts, finished_branch, sides, Side},
    pair, suites, ApiError, ApiResult,
};
use crate::{
    app::AppState,
    review::{
        self,
        digest::RunRecord,
        jobs::{self, Subject},
        settings::{settings, ReviewSettings, ReviewerModel},
        ticket, Job,
    },
    store::StoredReview,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/review/settings", get(get_settings))
        .route(
            "/api/branches/{id}/review",
            get(get_branch).post(post_branch),
        )
        .route("/api/compare/review", get(get_compare).post(post_compare))
        .route(
            "/api/suites/compare/review",
            get(get_suites).post(post_suites),
        )
}

type Q = Query<HashMap<String, String>>;

async fn get_settings(State(state): State<Arc<AppState>>) -> Json<ReviewSettings> {
    Json(settings(&state.config.providers).clone())
}

/// The reviewer, or 409 when AI review is off.
fn reviewer(state: &AppState) -> Result<ReviewerModel, ApiError> {
    let s = settings(&state.config.providers);
    match (&s.reviewer, s.enabled) {
        (Some(r), true) => Ok(r.clone()),
        _ => Err(ApiError::conflict(format!(
            "AI review is off: {}",
            s.why.as_deref().unwrap_or("no model")
        ))),
    }
}

async fn stored(state: &AppState, caller: &Caller, key: &str) -> ApiResult<StoredReview> {
    match state.store.at(caller).review(key).await? {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError::not_found("there is no review yet")),
    }
}

fn again(q: &HashMap<String, String>) -> bool {
    q.get("again").is_some_and(|v| v == "1" || v == "true")
}

/// A review's job, built on a task of its own once its row is stored.
type MakeJob = BoxFuture<'static, anyhow::Result<Job>>;

/// `POST` for any subject: 409 when review is off, then the route's parse (400), then the
/// subject's own checks (each branch there and finished, each suite there: 404/409), then the
/// review started with the job to build after.
async fn post(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    parsed: Result<Subject, ApiError>,
    q: &HashMap<String, String>,
) -> ApiResult<StoredReview> {
    let model = reviewer(&state)?;
    let subject = parsed?;
    let (s, c) = (state.clone(), caller.clone());
    let make: MakeJob = match subject.clone() {
        Subject::Branch(id) => {
            finished_branch(&state, &caller, &id).await?;
            Box::pin(async move { jobs::branch_job(&s, &c, &id).await })
        }
        Subject::Compare(a, b) => {
            for id in [&a, &b] {
                finished_branch(&state, &caller, id).await?;
            }
            Box::pin(async move {
                let [sa, sb] = sides(&s, &c, [&a, &b]).await.map_err(job_err)?;
                let facts = compare_facts(&s, &sa, &sb).await.map_err(job_err)?;
                let ticket = ticket(&s, &sa.events).await;
                let t = ticket.as_deref();
                Ok(jobs::compare_job(&record(&sa, t), &record(&sb, t), &facts))
            })
        }
        Subject::Suites(a, b) => {
            let sa = suites::find(&state, &caller, &a).await?;
            let sb = suites::find(&state, &caller, &b).await?;
            Box::pin(async move { jobs::suites_job(&s, &c, sa, sb).await })
        }
    };
    start(state, caller, &subject, model, again(q), make).await
}

/// The stored review when there is one to keep (done or running, and not asked again), else a new
/// one started by `model`: its row, with `make` building the job on a task of its own.
async fn start(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    subject: &Subject,
    model: ReviewerModel,
    again: bool,
    make: MakeJob,
) -> ApiResult<StoredReview> {
    let (key, records) = (subject.key(), state.store.at(&caller));
    if let Some(r) = records.review(&key).await? {
        let keep = r.status == "running" || (r.status == "done" && !again);
        if keep {
            return Ok(Json(r));
        }
    }
    let kind = subject.shape().kind();
    let Some(row) = records.start_review(&key, kind, &model.label()).await? else {
        return stored(&state, &caller, &key).await;
    };
    tokio::spawn(async move {
        let job = make.await;
        review::run(state, caller, key, model, job).await;
    });
    Ok(Json(row))
}

/// `ApiError` as an error a job can carry.
fn job_err(e: ApiError) -> anyhow::Error {
    anyhow!(e.message().to_owned())
}

fn record<'a>(s: &'a Side, ticket: Option<&'a str>) -> RunRecord<'a> {
    RunRecord {
        branch: &s.branch,
        events: &s.events,
        ticket,
    }
}

// ------------------------------------------------------------ routes: each parses its subject

async fn get_branch(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<StoredReview> {
    stored(&state, &caller, &Subject::Branch(id).key()).await
}

async fn post_branch(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    Query(q): Q,
) -> ApiResult<StoredReview> {
    post(state, caller, Ok(Subject::Branch(id)), &q).await
}

async fn get_compare(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(q): Q,
) -> ApiResult<StoredReview> {
    let (a, b) = pair(&q)?;
    stored(&state, &caller, &Subject::Compare(a, b).key()).await
}

async fn post_compare(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(q): Q,
) -> ApiResult<StoredReview> {
    let parsed = pair(&q).map(|(a, b)| Subject::Compare(a, b));
    post(state, caller, parsed, &q).await
}

async fn get_suites(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(q): Q,
) -> ApiResult<StoredReview> {
    let (a, b) = pair(&q)?;
    stored(&state, &caller, &Subject::Suites(a, b).key()).await
}

async fn post_suites(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Query(q): Q,
) -> ApiResult<StoredReview> {
    let parsed = pair(&q).map(|(a, b)| Subject::Suites(a, b));
    post(state, caller, parsed, &q).await
}
