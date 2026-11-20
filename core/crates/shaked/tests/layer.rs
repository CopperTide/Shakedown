//! The layer's hooks: every transaction begins through it, costly steps ask it first, token changes
//! are audited through it, its routes sit behind authentication, and a local Bearer token is
//! checked and capped by the engine.

use std::sync::Arc;

use axum::{http::StatusCode, routing::get, Router};
use serde_json::json;
use shaked::{
    app::AppState,
    auth::SystemJob,
    harness::{Call, SpyLayer},
    layer::{Admission, Refusal},
};
use support::{app, records, run, AppFx};

mod support;

fn team() -> serde_json::Value {
    json!({"id": "t", "name": "T", "agents": []})
}

#[test]
fn begin_runs_in_every_transaction_with_its_caller() {
    run(async {
        let spy = SpyLayer::new();
        let (store, _clean) =
            app::meta_store_layered("layer", "begin", Arc::new(spy.clone())).await;
        let before = spy.begins();
        records(&store).find_branch("one").await.unwrap();
        records(&store).find_branch("two").await.unwrap();
        assert_eq!(spy.begins() - before, 2, "one begin per transaction");
        assert!(
            spy.calls().contains(&Call::Begin(None)),
            "{:?}",
            spy.calls()
        );
        shaked::jobs::interrupt_at_start(&store).await.unwrap();
        let job = Call::Begin(Some(SystemJob::InterruptAtStart));
        assert!(spy.calls().contains(&job), "{:?}", spy.calls());
    })
}

#[test]
fn a_refused_admission_starts_nothing() {
    run(async {
        let spy = SpyLayer::new().admitting(Admission::Refuse {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "runs quota reached".into(),
        });
        let fx = AppFx::idle("layer", "refuse")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let (status, v) = fx.post("/api/runs", external_run()).await;
        assert_eq!((status, &v["error"]), (429, &json!("runs quota reached")));
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
        assert!(!spy.calls().iter().any(|c| matches!(c, Call::Meter(..))));
    })
}

#[test]
fn an_admitted_run_is_metered_once() {
    run(async {
        let spy = SpyLayer::new();
        let fx = AppFx::idle("layer", "meterrun")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let (status, v) = fx.post("/api/runs", external_run()).await;
        assert_eq!(status, 200, "{v}");
        let runs: Vec<_> = spy
            .calls()
            .into_iter()
            .filter(|c| matches!(c, Call::Meter(shaked::layer::Meter::Runs, _)))
            .collect();
        let run_id = v["run_id"].as_str().unwrap();
        assert_eq!(
            runs,
            [Call::Meter(
                shaked::layer::Meter::Runs,
                format!("run:{run_id}")
            )]
        );
    })
}

/// `GET /api/runs` with `bearer`: the status.
async fn runs_with(fx: &AppFx, bearer: &str) -> u16 {
    let r = fx
        .http
        .get(format!("{}/api/runs", fx.base))
        .bearer_auth(bearer);
    r.send().await.unwrap().status().as_u16()
}

fn external_run() -> serde_json::Value {
    json!({"task_id": "27", "team": team(), "runner": "external"})
}

#[test]
fn local_bearer_tokens_are_checked_and_capped() {
    run(async {
        let fx = AppFx::idle("layer", "bearer").start().await;
        assert_eq!(
            fx.get("/api/runs").await.0,
            200,
            "no header: the person here"
        );
        let asked = json!({"name": "r", "scopes": ["read"]});
        let (_, made) = fx.post("/api/tokens", asked).await;
        let read = made["token"].as_str().unwrap().to_string();
        assert_eq!(runs_with(&fx, &read).await, 200);
        let post = fx.http.post(format!("{}/api/runs", fx.base));
        let post = post.bearer_auth(&read).json(&external_run());
        assert_eq!(post.send().await.unwrap().status(), 403);
        let id = made["id"].as_str().unwrap();
        assert_eq!(fx.delete(&format!("/api/tokens/{id}")).await.0, 200);
        assert_eq!(runs_with(&fx, &read).await, 401, "revoked");
        let unknown = format!("sd_{}", "0".repeat(64));
        assert_eq!(runs_with(&fx, &unknown).await, 401, "unknown");
    })
}

#[test]
fn a_local_run_token_reaches_only_its_branch() {
    run(async {
        let fx = AppFx::idle("layer", "runtoken").start().await;
        let (status, v) = fx.post("/api/runs", external_run()).await;
        assert_eq!(status, 200, "{v}");
        let (run_id, b1) = (
            v["run_id"].as_str().unwrap(),
            v["branch_id"].as_str().unwrap(),
        );
        let soon = chrono::Utc::now() + chrono::Duration::hours(1);
        let kind = shaked::auth::RunTokenKind::Run;
        let sdr = records(&fx.state.store)
            .mint_run_token(run_id, b1, kind, None, None, soon)
            .await
            .unwrap();
        let finish = fx.http.post(format!("{}/api/branches/b2/finish", fx.base));
        let finish = finish.bearer_auth(&sdr).json(&json!({}));
        assert_eq!(finish.send().await.unwrap().status(), 404);
        assert_eq!(runs_with(&fx, &sdr).await, 403);
    })
}

fn spy_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/spy", get(|| async { "spied" }))
}

#[test]
fn layer_routes_sit_behind_authenticate() {
    run(async {
        let open = SpyLayer::new().with_routes(spy_routes);
        let fx = AppFx::idle("layer", "routesok")
            .layer(Arc::new(open))
            .start()
            .await;
        let r = fx
            .http
            .get(format!("{}/api/spy", fx.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let shut = SpyLayer::refuse_everyone().with_routes(spy_routes);
        let fx = AppFx::idle("layer", "routesno")
            .layer(Arc::new(shut))
            .start()
            .await;
        let r = fx
            .http
            .get(format!("{}/api/spy", fx.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    })
}

#[test]
fn token_changes_are_audited_through_the_layer_without_the_secret() {
    run(async {
        let spy = SpyLayer::new();
        let fx = AppFx::idle("layer", "audit")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let (_, made) = fx
            .post("/api/tokens", json!({"name": "ci", "scopes": ["run"]}))
            .await;
        let (token, id) = (
            made["token"].as_str().unwrap(),
            made["id"].as_str().unwrap(),
        );
        assert_eq!(fx.delete(&format!("/api/tokens/{id}")).await.0, 200);
        let audits: Vec<_> = spy
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Audit(action, detail) => Some((action, detail)),
                _ => None,
            })
            .collect();
        let actions: Vec<_> = audits.iter().map(|(a, _)| *a).collect();
        assert_eq!(actions, ["token.created", "token.revoked"]);
        for (_, detail) in &audits {
            assert!(!detail.contains(&token[3..]), "{detail}");
        }
    })
}

#[test]
fn a_public_bind_needs_a_layer_that_serves_others() {
    let get = |k: &str| (k == "SHAKEDOWN_BIND").then(|| "0.0.0.0:4510".to_string());
    assert!(shaked::mode::bind_from(get, false).is_err());
    assert!(shaked::mode::bind_from(get, true).is_ok());
    let refusal = Refusal::new(StatusCode::UNAUTHORIZED, "x");
    assert_eq!(refusal.status, 401);
}
