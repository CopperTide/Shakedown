//! Quotas and usage: plan presets from docs/quotas.toml, rows that override them, metering that
//! counts each call and run once, and checks that refuse or warn.

use serde_json::json;
use shaked::{
    auth::local_owner,
    quotas::{Limit, Meter, Period, Presets, Verdict},
    store::LOCAL_ORG_ID,
};
use support::{pg, run, AppFx};

mod support;

#[test]
fn built_in_presets_parse_and_beta_has_the_s4_numbers() {
    let p = Presets::built_in();
    let beta = |m| p.limit("beta", m).map(|l: Limit| (l.value, l.period));
    let gauge = Limit {
        value: 2,
        hard: true,
        period: Period::None,
    };
    assert_eq!(p.limit("beta", Meter::ConcurrentSandboxes), Some(gauge));
    assert_eq!(beta(Meter::SandboxSeconds), Some((180_000, Period::Month)));
    assert_eq!(beta(Meter::BranchSecondsMax), Some((3600, Period::None)));
    assert_eq!(
        beta(Meter::SavesBytes),
        Some((20 * 1024 * 1024 * 1024, Period::None))
    );
    assert_eq!(beta(Meter::TrialUsdMicros), Some((0, Period::Month)));
    assert_eq!(
        beta(Meter::ModelUsdMicros),
        None,
        "model spend is not limited by default"
    );
    assert_eq!(beta(Meter::Seats), None);
    let local = |m| p.limit("local", m).map(|l: Limit| l.value);
    assert_eq!(local(Meter::BranchSecondsMax), Some(86_400));
    assert_eq!(local(Meter::Runs), None);
    assert_eq!(p.limit("no-such-plan", Meter::Runs), None);
}

#[test]
fn metering_is_idempotent_and_totals_monthly() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("metering").await;
        let scope = local_owner();
        let at = store.at(&scope);
        assert!(at
            .meter(Meter::Runs, 1, "api", "api:run:r1", None, None)
            .await
            .unwrap());
        assert!(
            !at.meter(Meter::Runs, 1, "api", "api:run:r1", None, None)
                .await
                .unwrap(),
            "a resend counts once"
        );
        assert!(at
            .meter(Meter::Runs, 2, "api", "api:run:r2", None, None)
            .await
            .unwrap());
        let rows = pg::query_text(&name, "SELECT count(*) FROM shakedown.usage").await;
        assert_eq!(rows.as_deref(), Some("2"));
        let monthly = at.usage_by_month().await.unwrap();
        assert_eq!(
            monthly.iter().map(|(_, m, q)| (*m, *q)).collect::<Vec<_>>(),
            [(Meter::Runs, 3)]
        );
    })
}

#[test]
fn an_override_row_beats_the_preset() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("quotaoverride").await;
        let scope = local_owner();
        assert_eq!(
            store.at(&scope).limit(Meter::Runs).await.unwrap(),
            None,
            "local runs are unlimited"
        );
        let sql = format!(
            "INSERT INTO shakedown.quotas (org_id, meter, period, limit_value, hard) VALUES ('{LOCAL_ORG_ID}', 'runs', 'month', 5, false)"
        );
        pg::execute(&name, &sql).await;
        let got = store.at(&scope).limit(Meter::Runs).await.unwrap();
        assert_eq!(
            got,
            Some(Limit {
                value: 5,
                hard: false,
                period: Period::Month
            })
        );
        assert!(matches!(
            store.at(&scope).check(Meter::Runs, 1).await.unwrap(),
            Verdict::Ok
        ));
        assert!(matches!(
            store.at(&scope).check(Meter::Runs, 6).await.unwrap(),
            Verdict::Warn(_)
        ));
    })
}

fn run_body() -> serde_json::Value {
    json!({"task_id": "27", "team": {"id": "t", "name": "T", "agents": []}, "runner": "external"})
}

#[test]
fn over_a_hard_runs_quota_a_run_is_refused_naming_the_quota() {
    run(async {
        let fx = AppFx::idle("quotas", "hardruns").start().await;
        let sql = format!(
            "INSERT INTO shakedown.quotas (org_id, meter, period, limit_value) VALUES ('{LOCAL_ORG_ID}', 'runs', 'month', 0)"
        );
        pg::execute(&fx.db, &sql).await;
        let (status, v) = fx.post("/api/runs", run_body()).await;
        assert_eq!(status, 429, "{v}");
        assert!(v["error"].as_str().unwrap().contains("runs"), "{v}");
    })
}

#[test]
fn a_soft_quota_warns_and_lets_the_run_start() {
    run(async {
        let fx = AppFx::idle("quotas", "softruns").start().await;
        let sql = format!(
            "INSERT INTO shakedown.quotas (org_id, meter, period, limit_value, hard) VALUES ('{LOCAL_ORG_ID}', 'runs', 'month', 0, false)"
        );
        pg::execute(&fx.db, &sql).await;
        let (status, v) = fx.post("/api/runs", run_body()).await;
        assert_eq!(status, 200, "{v}");
        assert!(
            v["warning"].as_str().unwrap_or_default().contains("runs"),
            "{v}"
        );
    })
}

#[test]
fn local_runs_are_unlimited_but_metered() {
    run(async {
        let fx = AppFx::idle("quotas", "localruns").start().await;
        let (status, v) = fx.post("/api/runs", run_body()).await;
        assert_eq!(status, 200, "{v}");
        let run_id = v["run_id"].as_str().unwrap();
        let key = format!("SELECT meter || ' ' || quantity FROM shakedown.usage WHERE idempotency_key = 'api:run:{run_id}'");
        assert_eq!(
            pg::query_text(&fx.db, &key).await.as_deref(),
            Some("runs 1")
        );
    })
}

/// An app whose only provider is the fake server at `base`.
async fn on_fake_server(base: &str) -> AppFx {
    use shaked::providers::{Endpoint, LocalEndpoint, ProviderConfig, Secret};
    let mut providers = ProviderConfig::default();
    let endpoint = Endpoint::new(base, Some(Secret::new("k-pod")));
    providers.locals.push(LocalEndpoint {
        name: "pod".into(),
        endpoint,
        rate: None,
    });
    providers.default = "local:pod".into();
    AppFx::idle("quotas", "modelcall")
        .providers(providers)
        .start()
        .await
}

/// An external run with agent `a` on the fake server, which makes one model call: the branch.
async fn one_model_call(fx: &AppFx) -> String {
    let (_, v) = fx.post("/api/runs", run_body()).await;
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let join = json!({"id": "a", "provider": "local:pod", "model": "m"});
    let (_, reg) = fx
        .post(&format!("/api/branches/{branch}/agents"), join)
        .await;
    let chat = format!("{}/chat/completions", reg["gateway_url"].as_str().unwrap());
    let body = json!({"model": "m", "messages": [{"role": "user", "content": "go"}]});
    let req = fx
        .http
        .post(chat)
        .bearer_auth(reg["token"].as_str().unwrap())
        .json(&body);
    assert_eq!(req.send().await.unwrap().status().as_u16(), 200);
    branch
}

#[test]
fn every_recorded_model_call_is_metered_once() {
    use support::fake_llm::{fake_llm, gpt_oss};
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", None))]).await;
        let fx = on_fake_server(&llm.base).await;
        let branch = one_model_call(&fx).await;
        let sql = format!(
            "SELECT string_agg(meter || ' ' || idempotency_key, ',' ORDER BY meter) FROM shakedown.usage WHERE branch_id = '{branch}'"
        );
        let got = pg::query_text(&fx.db, &sql).await.unwrap();
        assert!(got.contains("model_tokens gateway:"), "{got}");
        assert_eq!(got.matches("model_tokens").count(), 1, "{got}");
    })
}

#[test]
fn org_usage_is_for_owners_and_billing_and_members_see_their_own() {
    run(async {
        let fx = AppFx::idle("quotas", "usageroute").hosted().start().await;
        let owner = shaked::auth::test_scope(fx.orgs().principal("a_owner"));
        let at = fx.state.store.at(&owner);
        at.meter(Meter::Runs, 1, "api", "api:run:x", None, None)
            .await
            .unwrap();
        let (status, v) = fx.signed("GET", "/api/usage/org", "a_billing", None).await;
        assert_eq!((status, v["scope"].clone()), (200, json!("org")), "{v}");
        assert_eq!(v["months"][0]["quantity"], 1);
        let (status, v) = fx.signed("GET", "/api/usage/org", "a_member", None).await;
        assert_eq!((status, v["scope"].clone()), (200, json!("own")), "{v}");
        assert_eq!(
            v["months"],
            json!([]),
            "the owner's run is not the member's"
        );
        assert_eq!(
            fx.signed("GET", "/api/usage/org", "a_viewer", None).await.0,
            403
        );
    })
}
