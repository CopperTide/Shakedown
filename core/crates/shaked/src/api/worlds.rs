//! The world builder's routes: templates and previews (the builder's answers passed through),
//! building a world into the worlds folder, and the benchmarks a user can choose between.

use std::{path::Path, path::PathBuf, sync::Arc};

use axum::{
    extract::{Path as UrlPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parts::{
    proto::valid_package_name,
    request::{GenerateWorld, GetTemplate, ListTemplates, PreviewWorld},
    CallError, Manifest, Request,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::extract::JsonBody;
use crate::{app::AppState, world, world_builds};

/// A refusal: its status and JSON body.
struct Refused(StatusCode, Value);

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

type Answer = Result<Response, Refused>;

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/templates", get(templates))
        .route("/api/templates/{id}", get(template))
        .route("/api/templates/{id}/preview", post(preview))
        .route("/api/worlds", get(list).post(build))
        .route("/api/worlds/{name}", get(one))
        .route("/api/worlds/{name}/use", post(choose))
        .route("/api/worlds/{name}/prepare", post(prepare))
}

fn refusal(status: StatusCode, msg: impl Into<String>) -> Refused {
    Refused(status, json!({"error": msg.into()}))
}

/// The builder's answer to `r`. A refusal (`-32602`) is `invalid` with its message, or 422 with
/// its problems when it gives them; no builder installed is a 404, and a shaked that starts no
/// package program a 409 ([`world_builds::PREBUILT_ONLY`]).
async fn ask<R: Request>(state: &AppState, r: &R, invalid: StatusCode) -> Result<Value, Refused> {
    if !state.hub.runs_programs() {
        return Err(refusal(StatusCode::CONFLICT, world_builds::PREBUILT_ONLY));
    }
    let name = (state.hub.registry().builders().next()).map(|m| m.name.clone());
    let name =
        name.ok_or_else(|| refusal(StatusCode::NOT_FOUND, "no world builder is installed"))?;
    let client = (state.hub.client(&name).await)
        .map_err(|e| refusal(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")))?;
    client.request(r).await.map_err(|e| match e {
        CallError::Part {
            code: -32602,
            message,
            data,
        } => match data.as_ref().and_then(|d| d.get("problems")) {
            Some(problems) => problems_answer(problems),
            None => refusal(invalid, message),
        },
        e => refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the world builder: {e}"),
        ),
    })
}

fn problems_answer(problems: &Value) -> Refused {
    Refused(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({"problems": problems}),
    )
}

/// `GET /api/templates`: the builder's `templates`.
async fn templates(State(state): State<Arc<AppState>>) -> Answer {
    ask(&state, &ListTemplates {}, StatusCode::BAD_REQUEST)
        .await
        .map(ok)
}

/// `GET /api/templates/{id}`: the template's form (404 for an unknown one).
async fn template(State(state): State<Arc<AppState>>, UrlPath(id): UrlPath<String>) -> Answer {
    ask(&state, &GetTemplate { id }, StatusCode::NOT_FOUND)
        .await
        .map(ok)
}

/// `POST /api/templates/{id}/preview` (body: the choices): what Review shows.
async fn preview(
    State(state): State<Arc<AppState>>,
    UrlPath(id): UrlPath<String>,
    JsonBody(choices): JsonBody<Value>,
) -> Answer {
    let r = PreviewWorld { id, choices };
    ask(&state, &r, StatusCode::BAD_REQUEST).await.map(ok)
}

fn ok(v: Value) -> Response {
    Json(v).into_response()
}

#[derive(Debug, Deserialize)]
struct BuildReq {
    template: String,
    #[serde(default)]
    choices: Value,
}

/// `POST /api/worlds`: previews the world (422 with its problems), refuses a name that is taken
/// (409 with a free one), generates it into the worlds folder, installs it and prepares it in the
/// background: 202 `{name, state: "preparing"}`.
async fn build(State(state): State<Arc<AppState>>, JsonBody(req): JsonBody<BuildReq>) -> Answer {
    let asked = world_builds::Asked::now(&state);
    let name = previewed_name(&state, &req).await?;
    let dir = worlds_dir(&state)?;
    refuse_taken(&state, &dir, &name)?;
    write_world(&state, &dir, &req, &name).await?;
    world_builds::spawn_prepare(&state, &name, asked);
    Ok(preparing(&name))
}

/// 202 `{name, state: "preparing"}`.
fn preparing(name: &str) -> Response {
    let body = json!({"name": name, "state": "preparing"});
    (StatusCode::ACCEPTED, Json(body)).into_response()
}

fn worlds_dir(state: &AppState) -> Result<PathBuf, Refused> {
    let dir = state.config.worlds_dir.clone();
    dir.ok_or_else(|| refusal(StatusCode::NOT_FOUND, "this shaked has no worlds folder"))
}

/// The package name the builder gives the world; its problems are a 422.
async fn previewed_name(state: &AppState, req: &BuildReq) -> Result<String, Refused> {
    let r = PreviewWorld {
        id: req.template.clone(),
        choices: req.choices.clone(),
    };
    let p = ask(state, &r, StatusCode::BAD_REQUEST).await?;
    if p["problems"].as_array().is_some_and(|a| !a.is_empty()) {
        return Err(problems_answer(&p["problems"]));
    }
    match p["name"].as_str() {
        Some(n) if valid_package_name(n) => Ok(n.to_string()),
        n => Err(refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the world builder named the world {n:?}, which is no package name"),
        )),
    }
}

fn is_taken(state: &AppState, dir: &Path, name: &str) -> bool {
    state.hub.registry().get(name).is_some() || dir.join(name).exists()
}

/// `{name}-{n}`, cut so that it stays a package name (40 characters at most).
fn suffixed(name: &str, n: u32) -> String {
    let tail = format!("-{n}");
    let keep = name.len().min(40 - tail.len());
    format!("{}{tail}", name[..keep].trim_end_matches('-'))
}

/// 409 when `name` is taken, suggesting the first free `{name}-N`.
fn refuse_taken(state: &AppState, dir: &Path, name: &str) -> Result<(), Refused> {
    if !is_taken(state, dir, name) {
        return Ok(());
    }
    let free = (2..).map(|n| suffixed(name, n));
    let suggested = free.into_iter().find(|s| !is_taken(state, dir, s));
    let error = format!("a world or package named {name} exists: give the company another name");
    let body = json!({"error": error, "suggested": suggested});
    Err(Refused(StatusCode::CONFLICT, body))
}

/// Generates the world into a temporary folder, moves it to `<dir>/<name>` and installs it; on
/// any failure nothing is left in `dir`.
async fn write_world(
    state: &AppState,
    dir: &Path,
    req: &BuildReq,
    name: &str,
) -> Result<(), Refused> {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let tmp = dir.join(format!(".{name}.tmp-{}", &hex[..8]));
    let g = GenerateWorld {
        id: req.template.clone(),
        choices: req.choices.clone(),
        dir: tmp.to_string_lossy().into_owned(),
    };
    let generated = ask(state, &g, StatusCode::BAD_REQUEST).await;
    let target = dir.join(name);
    let moved = generated.and_then(|_| {
        std::fs::rename(&tmp, &target).map_err(|e| match refuse_taken(state, dir, name) {
            Err(taken) => taken,
            Ok(()) => refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("moving the world: {e}"),
            ),
        })
    });
    let _ = std::fs::remove_dir_all(&tmp);
    moved?;
    if let Err(e) = state.hub.install(&target) {
        let _ = std::fs::remove_dir_all(&target);
        return Err(refusal(StatusCode::CONFLICT, format!("{e:#}")));
    }
    Ok(())
}

/// The benchmarks a user can choose between: those not opt-in, the built worlds and the current
/// one.
fn listed(state: &AppState) -> Vec<Manifest> {
    let current = state.benchmark();
    let registry = state.hub.registry();
    let shown = |m: &&Manifest| !m.opt_in || m.world.is_some() || m.name == current;
    registry.benchmarks().filter(shown).cloned().collect()
}

/// `preparing`/`ready`/`failed` from this run of shaked, else `ready` when its base saves exist
/// (`not_prepared` when they don't, or that can't be told).
async fn state_of(state: &AppState, name: &str) -> world_builds::BuildState {
    if let Some(b) = state.builds.state(name) {
        return b;
    }
    let based = async {
        let bench = state.hub.bench_describe(name).await?;
        world::has_base(&state.hub, &bench).await
    };
    let ready = based.await.unwrap_or(false);
    world_builds::BuildState {
        state: if ready { "ready" } else { "not_prepared" },
        elapsed_ms: 0,
        error: None,
    }
}

/// A benchmark's row: `{name, title, template, built, current, state}`, and with `detail`
/// `elapsed_ms` and `error`.
async fn row(state: &AppState, m: &Manifest, detail: bool) -> Value {
    let built = state_of(state, &m.name).await;
    let w = m.world.as_ref();
    let template = w.map(|w| json!({"id": w.template, "name": w.template_name}));
    let mut v = json!({
        "name": m.name, "title": w.map_or(&m.name, |w| &w.title), "template": template,
        "built": w.is_some(), "current": state.benchmark() == m.name, "state": built.state,
    });
    if detail {
        v["elapsed_ms"] = json!(built.elapsed_ms);
        v["error"] = json!(built.error);
    }
    v
}

/// `GET /api/worlds`.
async fn list(State(state): State<Arc<AppState>>) -> Json<Vec<Value>> {
    let mut rows = vec![];
    for m in listed(&state) {
        rows.push(row(&state, &m, false).await);
    }
    Json(rows)
}

fn benchmark_or_404(state: &AppState, name: &str) -> Result<Manifest, Refused> {
    let registry = state.hub.registry();
    let found = registry
        .get(name)
        .filter(|m| m.kind == parts::Kind::Benchmark);
    let missing = || refusal(StatusCode::NOT_FOUND, format!("no benchmark named {name}"));
    found.cloned().ok_or_else(missing)
}

/// `GET /api/worlds/{name}`: its row, with how long its preparation took and why it failed.
async fn one(State(state): State<Arc<AppState>>, UrlPath(name): UrlPath<String>) -> Answer {
    let m = benchmark_or_404(&state, &name)?;
    Ok(ok(row(&state, &m, true).await))
}

/// `POST /api/worlds/{name}/use`: new runs use it (remembered across a restart); its row.
async fn choose(State(state): State<Arc<AppState>>, UrlPath(name): UrlPath<String>) -> Answer {
    let m = benchmark_or_404(&state, &name)?;
    (state.use_benchmark(&name)).map_err(|e| refusal(StatusCode::NOT_FOUND, e))?;
    Ok(ok(row(&state, &m, true).await))
}

/// `POST /api/worlds/{name}/prepare`: prepares it again (after a failure): 202, or 409 while it
/// is being prepared.
async fn prepare(State(state): State<Arc<AppState>>, UrlPath(name): UrlPath<String>) -> Answer {
    benchmark_or_404(&state, &name)?;
    if !world_builds::spawn_prepare(&state, &name, world_builds::Asked::now(&state)) {
        return Err(refusal(
            StatusCode::CONFLICT,
            format!("{name} is being prepared"),
        ));
    }
    Ok(preparing(&name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_suffixed_name_stays_a_package_name() {
        use super::suffixed;
        assert_eq!(suffixed("acme-tools", 2), "acme-tools-2");
        let long = "x".repeat(40);
        assert_eq!(suffixed(&long, 12), format!("{}-12", "x".repeat(37)));
        let dashed = format!("{}-y", "x".repeat(37));
        assert_eq!(suffixed(&dashed, 2), format!("{}-2", "x".repeat(37)));
    }
}
