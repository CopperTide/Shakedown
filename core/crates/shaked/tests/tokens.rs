//! API tokens: `sd_` tokens shown once and stored hashed, capped by their scopes, made in the API or
//! with `shaked token create`, and taken as `Authorization: Bearer`.

use std::process::Command;

use serde_json::json;
use shaked::auth::tokens::{hash, resolve_api_token};
use support::{pg, run, AppFx};

mod support;

#[test]
fn a_token_is_shown_once_and_stored_hashed() {
    run(async {
        let fx = AppFx::idle("tokens", "once").start().await;
        let body = json!({"name": "laptop", "scopes": ["read"]});
        let (status, made) = fx.post("/api/tokens", body).await;
        assert_eq!(status, 200, "{made}");
        let token = made["token"].as_str().unwrap().to_string();
        assert!(token.starts_with("sd_") && token.len() == 67, "{token}");
        assert_eq!(made["prefix"], json!(&token[..8]));
        let (status, list) = fx.get("/api/tokens").await;
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
