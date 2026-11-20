//! The routes an external system drives a run with: start it (`POST /api/runs` with `runner:
//! "external"`), register agents and get their endpoints, report an agent done, and finish it.

use crate::auth::{Caller, Scope};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};

use super::{detached, providers_of, ApiError, ApiResult, JsonBody, OptionalJson, StartPermit};
use crate::{
    app::AppState,
    exec::{tool_schemas, BranchCtx},
    external::{self, endpoints, AgentDone, Join, UsageReport},
    launch::{launch, LaunchSpec},
    model::*,
    proxy::env::env_for,
    reasoning::{self, ReasoningReport},
    runner,
};

pub(super) fn router() -> Router<Arc<AppState>> {
    let agent = "/api/branches/{id}/agents/{agent}";
    Router::new()
        .route("/api/branches/{id}/agents", post(register))
        .route(&format!("{agent}/end"), post(end_agent))
        .route(&format!("{agent}/usage"), post(report_usage))
        .route(&format!("{agent}/reasoning"), post(report_reasoning))
        .route("/api/branches/{id}/finish", post(finish))
}

/// Starts a run whose agents an external system drives: its world started, its clock ticking,
/// the team's agents (there may be none yet) registered. Answers `{run_id, branch_id, agents}`
/// with each agent's endpoints. `(max_cost_usd, world_minutes)` are checked after the team and
/// the disk, the order external runs have always been refused in.
pub(super) async fn create_run(
    state: Arc<AppState>,
    caller: Arc<Scope>,
    permit: StartPermit,
    benchmark: String,
    (task_id, team): (String, Team),
    scenario: crate::scenario::Scenario,
    (max_cost_usd, world_minutes): (Option<f64>, Option<f64>),
) -> ApiResult<Value> {
    team::validate_agents(providers_of(&state), &team.agents)?;
    permit.check_disk(&state).await?;
    let rules = Rules::new(Runner::External)
        .with_max_cost(super::max_cost(max_cost_usd)?)
        .with_world_minutes(super::world_minutes(world_minutes)?);
    detached(async move {
        let _permit = permit;
        let spec = LaunchSpec {
            scope: caller,
            task_id: &task_id,
            team,
            scenario: &scenario,
            rules,
            driver: None,
            benchmark: Some(&benchmark),
        };
        let l = launch(&state, spec).await?;
        let mut started = external::Started::of(l.run_id, l.branch_id, l.agents);
        started.run_token = l.run_token;
        Ok(Json(serde_json::to_value(started)?))
    })
    .await
}

/// The live branch `id` an external system drives: 404 when it isn't live, 409 when shaked runs
/// its agents.
fn external_branch(state: &AppState, caller: &Scope, id: &str) -> Result<Arc<BranchCtx>, ApiError> {
    let ctx = state
        .live
        .get(id)
        .filter(|c| c.scope.org() == caller.org())
        .ok_or_else(|| ApiError::not_found(format!("no live branch \"{id}\"")))?;
    if ctx.rules.runner != Runner::External {
        return Err(ApiError::conflict(
            "shaked runs this branch's agents itself",
        ));
    }
    Ok(ctx)
}

/// The live external branch `id` (see `external_branch`) when it has `agent`: 404 when it hasn't.
fn external_agent(
    state: &AppState,
    caller: &Scope,
    id: &str,
    agent: &str,
) -> Result<Arc<BranchCtx>, ApiError> {
    let ctx = external_branch(state, caller, id)?;
    match ctx.roster.has_agent(agent) {
        true => Ok(ctx),
        false => Err(ApiError::not_found(format!(
            "no agent \"{agent}\" on this branch"
        ))),
    }
}

/// `{seq}`: the event a report was recorded as.
#[derive(Debug, Serialize)]
struct Recorded {
    seq: i64,
}

/// `{seqs}`: the events a report was recorded as, in order.
#[derive(Debug, Serialize)]
struct RecordedAll {
    seqs: Vec<i64>,
}

/// `{error, status}`: how a finished branch ended (fields in the order the keys were sorted in).
#[derive(Debug, Serialize)]
struct Finished {
    error: Option<String>,
    status: BranchStatus,
}

/// `POST /api/branches/{id}/agents`: registers an agent (or starts a known one again) and
/// answers its endpoints and the tools it gets.
async fn register(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path(id): Path<String>,
    JsonBody(join): JsonBody<Join>,
) -> ApiResult<Value> {
    team::validate_id(&join.id)?;
    let ctx = external_branch(&state, &caller, &id)?;
    if ctx.roster.agent(&join.id).is_none() {
        team::validate_agents(providers_of(&state), &[join.spec()])?;
    }
    let spec = external::register(&state.store, &ctx, &join).await?;
    let env = env_for(&state, &ctx, Some(&spec.id));
    let ends = endpoints(state.config.public_base(), &id, &spec.id, env);
    let mut v = serde_json::to_value(ends.with_token(ctx.agent_token(&spec.id)))?;
    v["tools"] = json!(tool_schemas(&ctx, &spec.id));
    v["spec"] = serde_json::to_value(&spec)?;
    Ok(Json(v))
}

/// `POST /api/branches/{id}/agents/{agent}/end`: records that the agent is done, with the tokens
/// and cost the system reports for it.
async fn end_agent(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path((id, agent)): Path<(String, String)>,
    OptionalJson(done): OptionalJson<AgentDone>,
) -> ApiResult<Recorded> {
    let ctx = external_agent(&state, &caller, &id, &agent)?;
    let ev = ctx
        .record(&state.store, Some(&agent), &done.payload())
        .await?;
    Ok(Json(Recorded { seq: ev.seq }))
}

/// `POST /api/branches/{id}/agents/{agent}/usage`: records one model call (or compaction) the
/// agent's client reported, as a `ModelCall` event.
async fn report_usage(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path((id, agent)): Path<(String, String)>,
    JsonBody(report): JsonBody<UsageReport>,
) -> ApiResult<Recorded> {
    let ctx = external_agent(&state, &caller, &id, &agent)?;
    let ev = ctx.append(
        &state.store,
        Some(&agent),
        EventKind::ModelCall,
        report.payload(),
    );
    Ok(Json(Recorded { seq: ev.await?.seq }))
}

/// `POST /api/branches/{id}/agents/{agent}/reasoning`: records what the agent thought or said it
/// would do, one `Reasoning` event per non-blank item, in order. Answers their `seqs`.
async fn report_reasoning(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path((id, agent)): Path<(String, String)>,
    JsonBody(report): JsonBody<ReasoningReport>,
) -> ApiResult<RecordedAll> {
    if report.items.is_empty() || report.items.len() > reasoning::MAX_ITEMS {
        return Err(ApiError::bad(format!(
            "a reasoning report carries 1 to {} items",
            reasoning::MAX_ITEMS
        )));
    }
    let ctx = external_agent(&state, &caller, &id, &agent)?;
    let mut seqs = Vec::new();
    for item in &report.items {
        let Some(p) = reasoning::payload(item, reasoning::SOURCE_AGENT) else {
            continue;
        };
        let ev = ctx.record(&state.store, Some(&agent), &p);
        seqs.push(ev.await?.seq);
    }
    Ok(Json(RecordedAll { seqs }))
}

/// What a system that drove a run says when it finishes: the error that ended it, if one did.
#[derive(Debug, Default, serde::Deserialize)]
struct FinishReq {
    #[serde(default)]
    error: Option<String>,
}

/// `POST /api/branches/{id}/finish`: the run is over. shaked freezes and scores the world, stops
/// it and answers how the branch ended. With `{"error"}` the run went wrong on the system's side:
/// it is still scored, and ends `failed` with that error. Before the end of the run's world time
/// window the finish is held (`window::hold`): it answers `{status: "running",
/// held_until_clock_ms}` and the run is scored when the window ends.
async fn finish(
    State(state): State<Arc<AppState>>,
    Caller(caller): Caller,
    Path(id): Path<String>,
    OptionalJson(req): OptionalJson<FinishReq>,
) -> ApiResult<Value> {
    let ctx = external_branch(&state, &caller, &id)?;
    let error = req.error.filter(|e| !e.trim().is_empty());
    if let Some(until) = crate::window::hold(&ctx, error.clone()) {
        return Ok(Json(
            json!({"status": BranchStatus::Running, "held_until_clock_ms": until}),
        ));
    }
    detached(async move {
        let ended = match error {
            Some(why) => runner::finish_failed(&state, &ctx, why).await,
            None => runner::finish(&state, &ctx).await,
        };
        let runner::Ending { status, error } =
            ended.ok_or_else(|| ApiError::conflict("the branch is ending already"))?;
        Ok(Json(serde_json::to_value(Finished { error, status })?))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes<T: Serialize>(v: &T) -> String {
        serde_json::to_string(v).unwrap()
    }

    /// The typed answers are byte for byte the sorted JSON objects they replaced.
    #[test]
    fn answers_are_the_bytes_they_were() {
        let finished = Finished {
            error: None,
            status: BranchStatus::Passed,
        };
        let was = json!({"status": BranchStatus::Passed, "error": None::<String>});
        assert_eq!(bytes(&finished), was.to_string());
        assert_eq!(bytes(&Recorded { seq: 4 }), json!({"seq": 4}).to_string());
        let all = RecordedAll { seqs: vec![1, 2] };
        assert_eq!(bytes(&all), json!({"seqs": [1, 2]}).to_string());
    }
}
