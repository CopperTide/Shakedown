//! Code connections: save the command that starts a team's driver, list and delete them, test
//! one, and start a run with one (`POST /api/runs` with `connection`). Every route that can start
//! a driver refuses requests a web page could make (`guard_launch`).

use crate::auth::{Caller, Who};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{detached, parse_body, providers_of, ApiError, ApiResult, StartPermit};
use crate::{
    app::AppState,
    driver::{self, Launched},
    external::Started,
    launch::{launch, LaunchSpec},
    model::*,
    store::{Connection, NewConnection},
};

/// The spend limit of a connection test's run, in USD.
pub const TEST_MAX_COST_USD: f64 = 0.50;
/// How long a connection test waits for its run to end.
pub const TEST_WAIT: Duration = Duration::from_secs(5 * 60);

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/connections", get(list).post(create))
        .route("/api/connections/{id}", axum::routing::delete(delete))
        .route("/api/connections/{id}/test", post(test))
}

/// True for an `Origin` on this machine: `http(s)://` and a loopback host, with any port.
fn loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = match rest.strip_prefix('[') {
        Some(v6) => return v6.split_once(']').is_some_and(|(ip, _)| ip == "::1"),
        None => rest.split(':').next().unwrap_or(""),
    };
    host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost")
}

/// 415 unless the request is JSON, 403 when it comes from a web page on another origin. A
/// browser sends a JSON request to another origin only after a CORS preflight, which shaked
/// answers for `SHAKEDOWN_CORS_ORIGIN` alone, so a web page can't make shaked start a program.
pub(super) fn guard_launch(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim_start()
                .to_ascii_lowercase()
                .starts_with("application/json")
        });
    if !json {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "a request that starts a driver must be JSON (content-type: application/json)".into(),
        ));
    }
    let origin = headers
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or("null"));
    let allowed = |o: &str| loopback_origin(o) || state.config.cors_origins.iter().any(|a| a == o);
    match origin {
        Some(o) if !allowed(o) => Err(ApiError(
            StatusCode::FORBIDDEN,
            format!("a request from {o} can't start a driver on this machine"),
        )),
        _ => Ok(()),
    }
}

#[derive(Deserialize)]
struct NewConnectionReq {
    name: String,
    command: Vec<String>,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn validate(req: &NewConnectionReq) -> Result<(), ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::bad("a connection needs a name"));
    }
    if req.command.first().is_none_or(|p| p.trim().is_empty()) {
        return Err(ApiError::bad(
            "a connection needs a command: the program and its arguments",
        ));
    }
    if let Some(dir) = &req.workdir {
        if !std::path::Path::new(dir).is_dir() {
            return Err(ApiError::bad(format!("the folder {dir} does not exist")));
        }
    }
    if let Some(k) = req
        .env
        .keys()
        .find(|k| k.is_empty() || k.contains(['=', '\0']))
    {
        return Err(ApiError::bad(format!(
            "{k:?} is not an environment variable name"
        )));
    }
    Ok(())
}

/// `GET /api/connections`: every connection, newest first, secret env values redacted.
async fn list(State(state): State<Arc<AppState>>, Who(caller): Who) -> ApiResult<Value> {
    let all = state.store.at(&caller).connections().await?;
    let shown: Vec<Connection> = all.iter().map(Connection::redacted).collect();
    Ok(Json(serde_json::to_value(shown)?))
}

/// `POST /api/connections`: saves a Code connection; answers it, secrets redacted.
async fn create(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Value> {
    guard_launch(&state, &headers)?;
    let req: NewConnectionReq = parse_body(&body)?;
    validate(&req)?;
    let new = NewConnection {
        name: req.name.trim(),
        command: &req.command,
        workdir: req.workdir.as_deref(),
        env: &req.env,
    };
    let made = state.store.at(&caller).create_connection(new).await?;
    Ok(Json(serde_json::to_value(made.redacted())?))
}

/// `DELETE /api/connections/{id}`: 404 when there is none. Runs that used it keep its id.
async fn delete(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    match state.store.at(&caller).delete_connection(&id).await? {
        true => Ok(Json(json!({"deleted": id}))),
        false => Err(ApiError::not_found(format!("no connection \"{id}\""))),
    }
}

/// The connection `id`, or 404.
pub(super) async fn connection_or_404(
    state: &AppState,
    caller: &Caller,
    id: &str,
) -> Result<Connection, ApiError> {
    let found = state.store.at(caller).connection(id).await?;
    found.ok_or_else(|| ApiError::not_found(format!("no connection \"{id}\"")))
}

/// A run to start with a Code connection.
pub(super) struct CodeRun {
    pub task_id: String,
    pub team: Team,
    pub scenario: crate::scenario::Scenario,
    pub max_cost_usd: Option<f64>,
    /// Its world time window in minutes (`crate::window`).
    pub world_minutes: Option<f64>,
    /// The benchmark it runs on (read once when it was asked for).
    pub benchmark: String,
}

/// `POST /api/runs` with `connection`: 403/415 for a request a web page could make, 400 for a
/// runner other than `external` (or `chain`, the default), 404 for an unknown connection.
pub(super) async fn create_run(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    headers: &HeaderMap,
    (id, runner): (&str, Runner),
    run: CodeRun,
) -> ApiResult<Value> {
    guard_launch(&state, headers)?;
    if !matches!(runner, Runner::Chain | Runner::External) {
        return Err(ApiError::bad(
            "a run with a Code connection is driven by its driver",
        ));
    }
    let conn = connection_or_404(&state, &caller, id).await?;
    Ok(Json(
        start_code_run(state, caller, permit, conn, run).await?.0,
    ))
}

/// A run a connection's driver drives, just started: the run's ids, its agents' endpoints (none
/// yet), and whether the driver started (`launched`) or not (`failed`, the run ended saying why).
pub(super) async fn start_code_run(
    state: Arc<AppState>,
    caller: Arc<Caller>,
    permit: StartPermit,
    conn: Connection,
    run: CodeRun,
) -> Result<(Value, Option<Launched>), ApiError> {
    team::validate_agents(&providers_of(&state, &caller).await, &run.team.agents)?;
    permit.check_disk(&state).await?;
    detached(async move {
        let _permit = permit;
        let spec = LaunchSpec {
            scope: caller,
            task_id: &run.task_id,
            team: run.team,
            scenario: &run.scenario,
            rules: driver::rules_for(&conn.id, run.max_cost_usd, run.world_minutes),
            driver: Some(&conn),
            benchmark: Some(&run.benchmark),
        };
        let l = launch(&state, spec).await?;
        let (started, launched) = (Started::of(l.run_id, l.branch_id, l.agents), l.driver);
        let mut v = serde_json::to_value(&started)?;
        v["connection"] = json!(conn.id);
        v["driver"] = json!(match launched {
            Some(_) => "launched",
            None => "failed",
        });
        Ok((v, launched))
    })
    .await
}

/// The run a connection test makes: the benchmark's first task, no agents yet (the driver
/// registers them), and a spend limit of [`TEST_MAX_COST_USD`].
async fn test_run(state: &AppState, conn: &Connection) -> Result<CodeRun, ApiError> {
    let bench = state.bench().await?;
    let first = bench.tasks.first();
    let task = first.ok_or_else(|| ApiError::bad("the benchmark has no task to test with"))?;
    let team = Team {
        id: "connection-test".into(),
        name: conn.name.clone(),
        agents: vec![],
    };
    Ok(CodeRun {
        task_id: task.id.clone(),
        team,
        scenario: Default::default(),
        max_cost_usd: Some(TEST_MAX_COST_USD),
        world_minutes: None,
        benchmark: bench.name,
    })
}

/// `POST /api/connections/{id}/test`: runs the benchmark's first task with the connection (see
/// `test_run`), waits up to [`TEST_WAIT`] for it to end, and answers what happened: `{ok,
/// branch_id, status, agents, tool_calls, error?, exit, stderr_tail}`. `ok` when the driver
/// registered an agent that called a tool, finished the run and exited cleanly.
async fn test(
    State(state): State<Arc<AppState>>,
    Who(caller): Who,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Value> {
    guard_launch(&state, &headers)?;
    let conn = connection_or_404(&state, &caller, &id).await?;
    let permit = StartPermit::take(&state)?;
    let run = test_run(&state, &conn).await?;
    let name = format!("Connection test · {}", conn.name);
    let (started, launched) =
        start_code_run(state.clone(), caller.clone(), permit, conn, run).await?;
    let text = |k: &str| started[k].as_str().unwrap_or_default().to_string();
    let (run_id, branch) = (text("run_id"), text("branch_id"));
    state.store.at(&caller).name_run(&run_id, &name).await?;
    detached(async move { Ok(test_outcome(&state, &caller, &branch, launched).await?) }).await
}

/// The branch once it ended, or once [`TEST_WAIT`] passed (then it is stopped, and the second
/// value says so).
async fn ended_or_stopped(
    state: &AppState,
    caller: &Caller,
    branch: &str,
) -> anyhow::Result<(Branch, Option<String>)> {
    let deadline = tokio::time::Instant::now() + TEST_WAIT;
    let records = state.store.at(caller);
    let mut b = records.branch(branch).await?;
    while b.status == BranchStatus::Running && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
        b = records.branch(branch).await?;
    }
    if b.status != BranchStatus::Running {
        return Ok((b, None));
    }
    state.live.stop(state, branch).await;
    let late = "the run was still going after 5 minutes, so it was stopped".to_string();
    Ok((records.branch(branch).await?, Some(late)))
}

/// Waits a little for the driver's exit, which is recorded just after its run ends.
async fn exit_of(launched: Option<&Launched>) -> Option<String> {
    let l = launched?;
    for _ in 0..20 {
        if l.exit().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    l.exit()
}

/// The agents a run's driver registered, and how many tool calls they made.
fn work_of(events: &[Event]) -> (Vec<String>, usize) {
    let mut agents: Vec<String> = (events.iter())
        .filter(|e| e.kind == EventKind::AgentStart)
        .filter_map(|e| e.agent.clone())
        .collect();
    agents.dedup();
    let calls = events.iter().filter(|e| e.kind == EventKind::ToolCall);
    (agents, calls.count())
}

/// What a connection test's run came to (see `test`).
async fn test_outcome(
    state: &Arc<AppState>,
    caller: &Caller,
    branch: &str,
    launched: Option<Launched>,
) -> anyhow::Result<Json<Value>> {
    let (b, late) = ended_or_stopped(state, caller, branch).await?;
    let exit = exit_of(launched.as_ref()).await;
    let (agents, tool_calls) = work_of(&state.store.at(caller).events(branch).await?);
    let scored = matches!(b.status, BranchStatus::Passed | BranchStatus::Failed);
    let by_driver = |e: &str| e.starts_with("the driver") || e.starts_with("driver raised");
    let driver_failed = b.error.as_deref().is_some_and(by_driver);
    let worked = !agents.is_empty() && tool_calls > 0;
    let ok = scored && !driver_failed && exit.as_deref() == Some("exit 0") && worked;
    let error = late
        .or(b.error.clone())
        .or_else(|| match (agents.is_empty(), tool_calls) {
            (true, _) => Some("the driver registered no agent".to_string()),
            (false, 0) => Some("no agent called a tool".to_string()),
            _ => None,
        });
    let stderr_tail = launched.as_ref().map(Launched::stderr_tail);
    Ok(Json(json!({
        "ok": ok, "branch_id": branch, "status": b.status, "agents": agents,
        "tool_calls": tool_calls, "error": error, "exit": exit,
        "stderr_tail": stderr_tail.unwrap_or_default(),
    })))
}

#[cfg(test)]
mod tests {
    use super::loopback_origin;

    #[test]
    fn only_loopback_origins_are_this_machine() {
        for o in [
            "http://127.0.0.1:4510",
            "http://localhost:5173",
            "https://localhost",
            "http://[::1]:4510",
        ] {
            assert!(loopback_origin(o), "{o}");
        }
        for o in [
            "null",
            "http://evil.test",
            "http://127.0.0.1.evil.test",
            "file://",
            "http://[::2]:1",
        ] {
            assert!(!loopback_origin(o), "{o}");
        }
    }
}
