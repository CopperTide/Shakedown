//! API tokens: `sd_` tokens shown once and stored hashed, capped by their scopes, made in the API or
//! with `shaked token create`, and taken as `Authorization: Bearer`.

use std::process::Command;

use shaked::auth::tokens::resolve_api_token;
use support::{pg, run};

mod support;

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
