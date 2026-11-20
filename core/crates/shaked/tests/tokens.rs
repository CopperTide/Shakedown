//! API tokens: `sd_` tokens shown once and stored hashed, capped by their scopes, made in the API or
//! with `shaked token create`, and taken as `Authorization: Bearer`.

use std::process::Command;

use serde_json::{json, Value};
use shaked::auth::tokens::{hash, resolve_api_token};
use support::{pg, run, AppFx};

mod support;

/// A token minted over the API for fixture user `user` with `scopes`: `(status, answer)`.
async fn mint(fx: &AppFx, user: &str, scopes: &[&str]) -> (u16, Value) {
    let body = json!({"name": "ci", "scopes": scopes});
    fx.signed("POST", "/api/tokens", user, Some(body)).await
}

/// A token of `user` with the one scope `scope`.
async fn token_of(fx: &AppFx, user: &str, scope: &str) -> String {
    let made = mint(fx, user, &[scope]).await.1;
    made["token"].as_str().unwrap().to_string()
}

async fn with_token(fx: &AppFx, method: &str, path: &str, token: &str, body: Option<Value>) -> u16 {
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
    let req = fx
        .http
        .request(m, format!("{}{path}", fx.base))
        .bearer_auth(token);
    let req = match body {
        Some(b) => req.json(&b),
        None => req,
    };
    req.send().await.unwrap().status().as_u16()
}

#[test]
fn a_token_is_shown_once_and_stored_hashed() {
    run(async {
        let fx = AppFx::idle("tokens", "once").hosted().start().await;
        let (status, made) = mint(&fx, "a_member", &["read"]).await;
        assert_eq!(status, 200, "{made}");
        let token = made["token"].as_str().unwrap().to_string();
        assert!(token.starts_with("sd_") && token.len() == 67, "{token}");
        assert_eq!(made["prefix"], json!(&token[..8]));
        let (status, list) = fx.signed("GET", "/api/tokens", "a_member", None).await;
        assert_eq!(status, 200, "{list}");
        let text = list.to_string();
        assert!(
            !text.contains(&token[3..]) && !text.contains(&hash(&token)),
            "{text}"
        );
        assert_eq!(list[0]["prefix"], json!(&token[..8]));
        let id = made["id"].as_str().unwrap();
        let stored = format!("SELECT token_hash FROM shakedown.api_tokens WHERE id = '{id}'");
        assert_eq!(pg::query_text(&fx.db, &stored).await, Some(hash(&token)));
    })
}

#[test]
fn bearer_tokens_act_as_their_holder_capped_by_scope() {
    run(async {
        let fx = AppFx::idle("tokens", "scopes").hosted().start().await;
        let read = token_of(&fx, "a_member", "read").await;
        let runs = token_of(&fx, "a_member", "run").await;
        assert_eq!(with_token(&fx, "GET", "/api/runs", &read, None).await, 200);
        let body = json!({"task_id": "t1", "team": {"id": "t", "name": "T", "agents": []}});
        assert_eq!(
            with_token(&fx, "POST", "/api/runs", &read, Some(body.clone())).await,
            403
        );
        let started = with_token(&fx, "POST", "/api/runs", &runs, Some(body)).await;
        assert!(
            ![401, 403].contains(&started),
            "a run token may start runs: {started}"
        );
        assert_eq!(
            with_token(&fx, "GET", "/api/runs", "sd_00", None).await,
            401
        );
        let expire = "UPDATE shakedown.api_tokens SET expires_at = now() - interval '1 minute'";
        pg::execute(&fx.db, expire).await;
        assert_eq!(
            with_token(&fx, "GET", "/api/runs", &read, None).await,
            401,
            "expired"
        );
    })
}

#[test]
fn a_revoked_token_is_refused() {
    run(async {
        let fx = AppFx::idle("tokens", "revoked").hosted().start().await;
        let made = mint(&fx, "a_member", &["read"]).await.1;
        let (token, id) = (
            made["token"].as_str().unwrap(),
            made["id"].as_str().unwrap(),
        );
        let path = format!("/api/tokens/{id}");
        assert_eq!(fx.signed("DELETE", &path, "a_member", None).await.0, 200);
        assert_eq!(with_token(&fx, "GET", "/api/runs", token, None).await, 401);
        assert_eq!(
            fx.signed("DELETE", &path, "b_owner", None).await.0,
            404,
            "another org's token"
        );
    })
}

#[test]
fn removing_a_member_kills_their_tokens() {
    run(async {
        let fx = AppFx::idle("tokens", "removed").hosted().start().await;
        let token = token_of(&fx, "a_member", "read").await;
        let member = fx.orgs().user("a_member").id;
        pg::execute(
            &fx.db,
            &format!("DELETE FROM identity.memberships WHERE user_id = '{member}'"),
        )
        .await;
        let left = pg::query_text(&fx.db, "SELECT count(*) FROM shakedown.api_tokens").await;
        assert_eq!(left.as_deref(), Some("0"));
        assert_eq!(with_token(&fx, "GET", "/api/runs", &token, None).await, 401);
    })
}

#[test]
fn creating_and_revoking_a_token_is_audited_without_the_secret() {
    run(async {
        let fx = AppFx::idle("tokens", "audit").hosted().start().await;
        let made = mint(&fx, "a_member", &["read"]).await.1;
        let (token, id) = (
            made["token"].as_str().unwrap(),
            made["id"].as_str().unwrap(),
        );
        fx.signed("DELETE", &format!("/api/tokens/{id}"), "a_member", None)
            .await;
        let sql = "SELECT string_agg(action || ' ' || detail::text, '|' ORDER BY id) FROM identity.audit_log";
        let log = pg::query_text(&fx.db, sql).await.unwrap();
        assert!(
            log.starts_with("token.created") && log.contains("|token.revoked"),
            "{log}"
        );
        assert!(
            log.contains(&token[..8]) && !log.contains(&token[8..]),
            "{log}"
        );
    })
}

#[test]
fn a_viewer_may_mint_read_tokens_only() {
    run(async {
        let fx = AppFx::idle("tokens", "viewer").hosted().start().await;
        assert_eq!(mint(&fx, "a_viewer", &["run"]).await.0, 403);
        assert_eq!(mint(&fx, "a_viewer", &["read"]).await.0, 200);
        assert_eq!(mint(&fx, "a_billing", &["read"]).await.0, 403);
    })
}

#[test]
fn cli_token_create_prints_a_working_token() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("tokencli").await;
        let out = Command::new(env!("CARGO_BIN_EXE_shaked"))
            .args(["token", "create", "--name", "laptop", "--scope", "read"])
            .env("SHAKEDOWN_TEST_GUARD", "1")
            .env("SHAKEDOWN_DB", &name)
            .env("SHAKEDOWN_PG_URL", pg::pg_url())
            .output()
            .unwrap();
        let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            printed.starts_with("sd_") && !printed.contains(' '),
            "{printed}"
        );
        let holder = resolve_api_token(&store, &printed).await.unwrap();
        assert!(holder.is_some(), "the printed token resolves");
        assert!(resolve_api_token(&store, "sd_nope")
            .await
            .unwrap()
            .is_none());
    })
}

#[test]
fn read_tokens_and_viewers_cannot_write() {
    run(async {
        let fx = AppFx::idle("tokens", "readonly").hosted().start().await;
        let read = token_of(&fx, "a_owner", "read").await;
        assert_eq!(
            with_token(&fx, "DELETE", "/api/runs/nope", &read, None).await,
            403
        );
        let fork = json!({"at_seq": 1});
        let (status, _) = fx
            .signed("POST", "/api/branches/nope/fork", "a_viewer", Some(fork))
            .await;
        assert_eq!(status, 403, "a viewer cannot fork");
        assert_eq!(
            mint(&fx, "a_viewer", &["read"]).await.0,
            200,
            "but may mint its own read token"
        );
    })
}
