//! Hosted request auth: requests the gateway signed for a user in an org, that user's role and
//! teams read live on every request, and `/api/me`.

use serde_json::{json, Value};
use shaked::auth::signing::{sign_v2, verify_v2, ORG_HEADER, V2_VECTOR};
use support::{pg, run, AppFx};

mod support;

#[test]
fn v2_vector_verifies_and_any_field_change_fails() {
    let (secret, method, path, user, org, ts, want) = V2_VECTOR;
    let s = secret.as_bytes();
    assert_eq!(sign_v2(s, method, path, user, org, ts), want);
    let ok = |s: &[u8], m: &str, p: &str, o: &str, sig: &str, now: i64| {
        verify_v2(s, m, p, user, o, ts, sig, now)
    };
    assert!(ok(s, method, path, org, want, ts));
    assert!(
        !ok(
            s,
            method,
            path,
            "0b000000-0000-0000-0000-000000000001",
            want,
            ts
        ),
        "another org"
    );
    assert!(!ok(s, "POST", path, org, want, ts), "another method");
    assert!(!ok(s, method, "/api/runs", org, want, ts), "another path");
    assert!(!ok(s, method, path, org, want, ts + 61), "too old");
    assert!(
        !ok(b"another secret", method, path, org, want, ts),
        "another secret"
    );
    assert!(!ok(s, method, path, org, "zz", ts), "not hex");
}

/// Org A's and org B's owners each start a run (through the store, as their scopes): A's run.
async fn one_run_each(fx: &AppFx) -> String {
    let orgs = fx.orgs();
    let team = shaked::model::Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![],
    };
    let mut a_run = String::new();
    for key in ["a_owner", "b_owner"] {
        let scope = shaked::auth::test_scope(orgs.principal(key));
        let run = fx
            .state
            .store
            .at(&scope)
            .create_run("t", &team)
            .await
            .unwrap();
        if key == "a_owner" {
            a_run = run;
        }
    }
    a_run
}

fn run_ids(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn hosted_signed_member_lists_only_their_orgs_runs() {
    run(async {
        let fx = AppFx::idle("auth", "lists").hosted().start().await;
        let a_run = one_run_each(&fx).await;
        let (status, v) = fx.signed("GET", "/api/runs", "a_member", None).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(run_ids(&v), [a_run]);
        let (status, _) = fx.unsigned("GET", "/api/runs").await;
        assert_eq!(status, 401);
    })
}

#[test]
fn a_bad_or_late_signature_is_refused() {
    run(async {
        let fx = AppFx::idle("auth", "badsig").hosted().start().await;
        let late = chrono::Utc::now().timestamp() - 120;
        assert_eq!(
            fx.signed_at("GET", "/api/runs", "a_member", late).await.0,
            401
        );
        let (status, _) = fx
            .signed_with("GET", "/api/runs", "a_member", b"not the secret")
            .await;
        assert_eq!(status, 401);
    })
}

#[test]
fn hosted_removed_member_is_refused_on_the_next_request() {
    run(async {
        let fx = AppFx::idle("auth", "removed").hosted().start().await;
        assert_eq!(fx.signed("GET", "/api/runs", "a_member", None).await.0, 200);
        let member = fx.orgs().user("a_member").id;
        pg::execute(
            &fx.db,
            &format!("DELETE FROM identity.memberships WHERE user_id = '{member}'"),
        )
        .await;
        let (status, v) = fx.signed("GET", "/api/runs", "a_member", None).await;
        assert_eq!(
            (status, v["error"].clone()),
            (403, json!("not a member of this org"))
        );
    })
}

#[test]
fn hosted_deleted_org_is_refused() {
    run(async {
        let fx = AppFx::idle("auth", "deletedorg").hosted().start().await;
        let a = fx.orgs().org("a").id;
        let sql = format!(
            "UPDATE identity.orgs SET deleted_at = now(), purge_after = now() + interval '30 days' WHERE id = '{a}'"
        );
        pg::execute(&fx.db, &sql).await;
        assert_eq!(fx.signed("GET", "/api/runs", "a_owner", None).await.0, 403);
    })
}

#[test]
fn local_ignores_signature_headers() {
    run(async {
        let fx = AppFx::idle("auth", "localforged").start().await;
        let url = format!("{}/api/me", fx.base);
        let forged = fx
            .http
            .get(url)
            .header(ORG_HEADER, "0b000000-0000-0000-0000-000000000001");
        let (status, v) = support::app::send(forged).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["org"]["kind"], "local");
        assert_eq!(v["mode"], "local");
    })
}

#[test]
fn a_viewer_gets_403_starting_a_run_and_404_for_other_orgs() {
    run(async {
        let fx = AppFx::idle("auth", "viewer").hosted().start().await;
        let body = json!({"task_id": "t1", "team": {"id": "t", "name": "T", "agents": []}});
        let (status, v) = fx.signed("POST", "/api/runs", "a_viewer", Some(body)).await;
        assert_eq!(status, 403, "{v}");
        let a_run = one_run_each(&fx).await;
        let path = format!("/api/runs/{a_run}/name");
        let (status, _) = fx
            .signed("POST", &path, "b_owner", Some(json!({"name": "x"})))
            .await;
        assert_eq!(status, 404);
    })
}

#[test]
fn me_says_mode_and_org() {
    run(async {
        let fx = AppFx::idle("auth", "melocal").start().await;
        let (status, v) = fx.get("/api/me").await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(
            (v["mode"].clone(), v["org"]["kind"].clone()),
            (json!("local"), json!("local"))
        );
        assert_eq!(v["role"], "owner");
        assert!(v.get("orgs").is_none(), "no org list locally: {v}");
        let hosted = AppFx::idle("auth", "mehosted").hosted().start().await;
        let (status, v) = hosted.signed("GET", "/api/me", "a_member", None).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(
            (v["mode"].clone(), v["role"].clone()),
            (json!("hosted"), json!("member"))
        );
        assert_eq!(v["orgs"].as_array().map(Vec::len), Some(1));
        assert_eq!(v["user"]["email"], "a_member@example.test");
    })
}
