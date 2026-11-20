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
    layer::{Admission, Policy, Refusal},
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

/// The runs meter calls the spy saw.
fn runs_metered(spy: &SpyLayer) -> Vec<Call> {
    spy.calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Meter(shaked::layer::Meter::Runs, _)))
        .collect()
}

fn refused_runs() -> Admission {
    Admission::Refuse {
        status: StatusCode::TOO_MANY_REQUESTS,
        message: "runs quota reached".into(),
    }
}

/// An external suite of one run on task 27: its id.
async fn external_suite(fx: &AppFx) -> String {
    let body = json!({"team": team(), "tasks": ["27"], "repeats": 1, "runner": "external"});
    let (status, v) = fx.post("/api/suites", body).await;
    assert_eq!(status, 200, "{v}");
    v["id"].as_str().unwrap().to_string()
}

#[test]
fn a_claimed_suite_run_is_admitted_and_metered_once() {
    run(async {
        let spy = SpyLayer::new();
        let fx = AppFx::idle("layer", "claimrun")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let id = external_suite(&fx).await;
        let (status, c) = fx.post(&format!("/api/suites/{id}/claim"), json!({})).await;
        assert_eq!(status, 200, "{c}");
        let run_id = c["run_id"].as_str().unwrap();
        assert!(
            spy.calls().contains(&Call::Admit("StartRun".into())),
            "{:?}",
            spy.calls()
        );
        let want = Call::Meter(shaked::layer::Meter::Runs, format!("run:{run_id}"));
        assert_eq!(runs_metered(&spy), [want]);
    })
}

#[test]
fn a_refused_admission_starts_no_claimed_suite_run() {
    run(async {
        let spy = SpyLayer::new().admitting(refused_runs());
        let fx = AppFx::idle("layer", "claimrefuse")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let id = external_suite(&fx).await;
        let (status, v) = fx.post(&format!("/api/suites/{id}/claim"), json!({})).await;
        assert_eq!((status, &v["error"]), (429, &json!("runs quota reached")));
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
        assert!(runs_metered(&spy).is_empty());
    })
}

/// True once suite `id` is no longer running (within 10 s).
async fn suite_ended(fx: &AppFx, id: &str) -> bool {
    for _ in 0..100 {
        let (_, s) = fx.get(&format!("/api/suites/{id}")).await;
        if s["status"] != "running" {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
}

#[test]
fn a_refused_admission_starts_no_builtin_suite_run() {
    run(async {
        let spy = SpyLayer::new().admitting(refused_runs());
        let fx = AppFx::idle("layer", "suiterefuse")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        let agent = json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m",
                           "prompt": "Poke the box."});
        let team = json!({"id": "o", "name": "Open", "agents": [agent]});
        let body = json!({"team": team, "tasks": ["27"], "repeats": 1});
        let (status, v) = fx.post("/api/suites", body).await;
        assert_eq!(status, 200, "{v}");
        assert!(
            suite_ended(&fx, v["id"].as_str().unwrap()).await,
            "the suite ended"
        );
        assert!(
            spy.calls().contains(&Call::Admit("StartRun".into())),
            "{:?}",
            spy.calls()
        );
        assert!(records(&fx.state.store).runs().await.unwrap().is_empty());
        assert!(runs_metered(&spy).is_empty());
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

/// A gateway's headers reach the layer only when it serves others (it verifies them); locally
/// they are dropped before anything reads them.
#[test]
fn gateway_headers_reach_only_a_layer_that_serves_others() {
    run(async {
        let serving = Policy {
            serves_others: true,
            ..Policy::LOCAL
        };
        for (policy, kept) in [(Policy::LOCAL, false), (serving, true)] {
            let spy = SpyLayer::new().with_policy(policy);
            let fx = AppFx::idle("layer", "gatewayheaders")
                .layer(Arc::new(spy.clone()))
                .start()
                .await;
            let url = format!("{}/api/runs", fx.base);
            let r = fx.http.get(url).header("x-sculptured-user", "someone");
            assert_eq!(r.send().await.unwrap().status(), 200);
            let seen = spy
                .seen_headers()
                .contains(&"x-sculptured-user".to_string());
            assert_eq!(seen, kept, "{policy:?}");
        }
    })
}

/// The operator's environment: keys for OpenAI and Anthropic, and a local server.
fn operator_env() -> shaked::providers::ProviderConfig {
    use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig, Secret};
    let mut env = ProviderConfig::default();
    env.openai.key = Some(Secret::new("sk-operator-env-key"));
    env.anthropic.key = Some(Secret::new("sk-ant-operator-env-key"));
    env.locals.push(LocalEndpoint {
        name: "lab".into(),
        endpoint: Endpoint::new("http://10.0.0.9:8000/v1", Some(Secret::new("lab-key"))),
        rate: None,
    });
    env
}

/// Bedrock with no key of the caller's says so, and never signs with the machine's credentials.
async fn bedrock_needs_an_own_key(fx: &AppFx) {
    let providers = &fx.state.config.providers;
    let bedrock = providers.resolve(Some("bedrock:us-east-1")).unwrap();
    let why = bedrock.unavailable().map(|u| u.why()).unwrap_or_default();
    assert!(why.contains("key of your own"), "{why}");
    let call = json!({"model": "m", "max_tokens": 1, "messages": []});
    let e = bedrock.send(&fx.state.http, &call).await.unwrap_err();
    assert!(e.contains("key of your own"), "{e}");
}

/// A layer without environment keys (hosted) never lends a caller the operator's model keys or
/// local servers, nor Bedrock the machine's AWS credentials (env, the aws CLI, an instance role);
/// locally the environment's keys serve as before.
#[test]
fn a_layer_without_environment_keys_lends_none_of_the_operators() {
    use shaked::providers::keys::provider_for;
    run(async {
        let hosted = Policy {
            serves_others: true,
            environment_keys: false,
            ..Policy::LOCAL
        };
        for (policy, lent) in [(Policy::LOCAL, true), (hosted, false)] {
            let spy = SpyLayer::new().with_policy(policy);
            let fx = AppFx::idle("layer", "envkeys")
                .layer(Arc::new(spy))
                .providers(operator_env())
                .start()
                .await;
            let caller = support::local();
            for asked in ["openai", "anthropic"] {
                let (p, _) = provider_for(&fx.state, caller, Some(asked), "m").await;
                assert_eq!(
                    p.unwrap().endpoint.key.is_some(),
                    lent,
                    "{asked} {policy:?}"
                );
            }
            let (lab, _) = provider_for(&fx.state, caller, Some("local:lab"), "m").await;
            assert_eq!(lab.is_ok(), lent, "{policy:?}");
            if !lent {
                bedrock_needs_an_own_key(&fx).await;
            }
        }
    })
}

/// A shaked that serves others never seals with the source checkout's vault key: without
/// SHAKEDOWN_VAULT_KEY(_FILE) its vault is refused (and so is its start).
#[test]
fn a_layer_that_serves_others_has_no_default_vault() {
    run(async {
        let serving = Policy {
            serves_others: true,
            ..Policy::LOCAL
        };
        let spy = SpyLayer::new().with_policy(serving);
        let fx = AppFx::idle("layer", "novault")
            .layer(Arc::new(spy))
            .start()
            .await;
        if std::env::var_os("SHAKEDOWN_VAULT_KEY").is_none()
            && std::env::var_os("SHAKEDOWN_VAULT_KEY_FILE").is_none()
        {
            let e = fx.state.vault().err().unwrap_or_default();
            assert!(e.contains("SHAKEDOWN_VAULT_KEY"), "{e}");
        }
    })
}

/// A route whose error carries the database's text, as a failed query's does.
fn failing_routes() -> Router<Arc<AppState>> {
    let fail = || async {
        let e = anyhow::anyhow!("relation \"shakedown.secret_table\" does not exist");
        shaked::api::ApiError::from(e.context("reading the secret table"))
    };
    Router::new().route("/api/spy/fails", get(fail))
}

/// A shaked that serves others answers a 500 with a generic message and a reference, and logs the
/// detail; locally the detail is the answer.
#[test]
fn a_serving_layer_answers_500s_without_their_detail() {
    run(async {
        let serving = Policy {
            serves_others: true,
            ..Policy::LOCAL
        };
        for (policy, shown) in [(Policy::LOCAL, true), (serving, false)] {
            let spy = SpyLayer::new()
                .with_policy(policy)
                .with_routes(failing_routes);
            let fx = AppFx::idle("layer", "hide500")
                .layer(Arc::new(spy))
                .start()
                .await;
            let (status, v) = fx.get("/api/spy/fails").await;
            assert_eq!(status, 500, "{v}");
            let text = v["error"].as_str().unwrap_or_default();
            assert_eq!(text.contains("secret_table"), shown, "{policy:?}: {text}");
        }
    })
}

/// A unique violation the database raised is 409 (the record exists), never 500.
#[test]
fn a_unique_violation_is_a_conflict() {
    use axum::response::IntoResponse;
    run(async {
        let fx = AppFx::idle("layer", "unique").start().await;
        let url = support::pg::admin().db_url(&fx.db);
        let (c, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        let task = tokio::spawn(conn);
        c.batch_execute(
            "CREATE TEMP TABLE u (x int CONSTRAINT u_x_key UNIQUE); INSERT INTO u VALUES (1)",
        )
        .await
        .unwrap();
        let e = c
            .execute("INSERT INTO u VALUES (1)", &[])
            .await
            .unwrap_err();
        let r =
            shaked::api::ApiError::from(anyhow::Error::from(e).context("adding")).into_response();
        assert_eq!(r.status(), StatusCode::CONFLICT);
        drop(c);
        let _ = task.await;
    })
}
