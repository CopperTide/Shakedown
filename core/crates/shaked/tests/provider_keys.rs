//! Your own model keys and OpenAI-compatible servers: sealed to their row, write-only, one per
//! name, used before the environment's, tested with a one-token call, and a server on a private
//! address refused where the layer's policy says so (no real model: a scripted fake server).
mod support;

use std::sync::Arc;

use serde_json::{json, Value};
use shaked::{
    harness::SpyLayer,
    layer::{KeySource, Policy},
    providers::{keys::provider_for, Endpoint, ProviderConfig, Secret},
    vault::{key_context, Vault},
};
use support::fake_llm::{fake_llm, gpt_oss};
use support::{local, run, AppFx};
use uuid::Uuid;

const K: &str = "0001020304050607080910111213141516171819202122232425262728293031";

/// The app's vault: a fixed test key, never the repository's key file.
fn with_vault(fx: &AppFx) {
    let _ = fx.state.vault.set(Arc::new(Vault::from_hex(K).unwrap()));
}

async fn app(tag: &str) -> AppFx {
    let fx = AppFx::idle("keys", tag).start().await;
    with_vault(&fx);
    fx
}

fn openai_key(name: &str, secret: &str) -> Value {
    json!({"name": name, "kind": "openai", "secret": secret})
}

#[test]
fn a_sealed_key_opens_only_for_its_row() {
    let v = Vault::from_hex(K).unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(key_context(a), format!("provider_key:{a}:key"));
    let sealed = v.seal(&key_context(a), b"sk-very-secret-1234");
    assert!(!sealed.windows(9).any(|w| w == b"sk-very-s"));
    assert_eq!(
        &*v.open(&key_context(a), &sealed).unwrap(),
        b"sk-very-secret-1234"
    );
    assert!(
        v.open(&key_context(b), &sealed).is_err(),
        "bound to its row"
    );
}

#[test]
fn keys_are_write_only() {
    run(async {
        let fx = app("writeonly").await;
        let (status, made) = fx
            .post("/api/models/keys", openai_key("mine", "sk-test-abcd1234"))
            .await;
        assert_eq!(status, 200, "{made}");
        let (status, list) = fx.get("/api/models/keys").await;
        assert_eq!(status, 200);
        for text in [made.to_string(), list.to_string()] {
            assert!(!text.contains("sk-test-abcd1234"), "{text}");
            assert!(
                !text.contains("\"secret\"") && !text.contains("sealed_key"),
                "{text}"
            );
        }
        assert_eq!(list[0]["key_last4"], "1234");
        assert_eq!(
            (&list[0]["name"], &list[0]["kind"]),
            (&json!("mine"), &json!("openai"))
        );
    })
}

#[test]
fn two_keys_cannot_share_a_name() {
    run(async {
        let fx = app("names").await;
        let first = fx
            .post("/api/models/keys", openai_key("mine", "sk-one-1111"))
            .await;
        assert_eq!(first.0, 200, "{}", first.1);
        let (status, v) = fx
            .post("/api/models/keys", openai_key("mine", "sk-two-2222"))
            .await;
        assert_eq!(status, 409, "{v}");
        assert!(v["error"].as_str().unwrap().contains("mine"), "{v}");
    })
}

/// The key `p` carries upstream.
fn key_of(p: Result<shaked::providers::Provider, String>) -> String {
    let p = p.unwrap();
    p.endpoint.key.as_ref().unwrap().expose().to_string()
}

#[test]
fn your_key_wins_over_the_environment() {
    run(async {
        let llm = fake_llm(vec![(200, gpt_oss("t", Some("hi")))]).await;
        let env = ProviderConfig {
            openai: Endpoint::new(&llm.base, Some(Secret::new("k-env"))),
            ..ProviderConfig::default()
        };
        let fx = AppFx::idle("keys", "wins").providers(env).start().await;
        with_vault(&fx);
        let (_, made) = fx
            .post("/api/models/keys", openai_key("mine", "k-own-0000"))
            .await;
        let id: Uuid = made["id"].as_str().unwrap().parse().unwrap();
        let (p, paid) = provider_for(&fx.state, local(), Some("openai"), "m").await;
        assert_eq!(paid, KeySource::Own(id));
        assert_eq!(
            p.as_ref().unwrap().endpoint.base,
            llm.base,
            "on the configured server"
        );
        assert_eq!(key_of(p), "k-own-0000");
        assert_eq!(fx.delete(&format!("/api/models/keys/{id}")).await.0, 200);
        let (p, paid) = provider_for(&fx.state, local(), Some("openai"), "m").await;
        assert_eq!((paid, key_of(p)), (KeySource::Environment, "k-env".into()));
    })
}

/// Tests key `id`: the key as answered.
async fn tested(fx: &AppFx, id: &str) -> Value {
    let (status, v) = fx
        .post(&format!("/api/models/keys/{id}/test"), json!({}))
        .await;
    assert_eq!(status, 200, "{v}");
    v
}

/// Adds the custom server `pod` at `base`, model m1, key sk-good-5678: the key as answered.
async fn add_pod(fx: &AppFx, base: &str) -> Value {
    let server = json!({"name": "pod", "kind": "custom", "base_url": base,
                        "model": "m1", "secret": "sk-good-5678"});
    let (status, made) = fx.post("/api/models/keys", server).await;
    assert_eq!(status, 200, "{made}");
    made
}

#[test]
fn test_makes_a_one_token_call_and_records_the_outcome() {
    run(async {
        let refused =
            json!({"error": {"message": "Incorrect API key provided: sk-bad-secret-wxyz"}});
        let llm = fake_llm(vec![(200, gpt_oss("t", Some("ok"))), (401, refused)]).await;
        let fx = app("test").await;
        let made = add_pod(&fx, &llm.base).await;
        let id = made["id"].as_str().unwrap();
        let first = tested(&fx, id).await;
        assert_eq!(first["checked_ok"], true, "{first}");
        let seen = llm.seen.lock().unwrap()[0].clone();
        assert_eq!(
            (&seen.body["max_tokens"], &seen.body["model"]),
            (&json!(1), &json!("m1"))
        );
        assert_eq!(seen.authorization.as_deref(), Some("Bearer sk-good-5678"));
        let new = json!({"secret": "sk-bad-secret-wxyz"});
        let replaced = fx.post(&format!("/api/models/keys/{id}/secret"), new).await;
        assert_eq!(replaced.0, 200, "{}", replaced.1);
        let second = tested(&fx, id).await;
        assert_eq!(second["checked_ok"], false, "{second}");
        assert_eq!(second["key_last4"], "wxyz");
        let detail = second["check_detail"].as_str().unwrap();
        assert!(
            !detail.is_empty() && !detail.contains("sk-bad-s"),
            "{detail}"
        );
        let (_, list) = fx.get("/api/models/keys").await;
        assert_eq!(list[0]["checked_ok"], false);
    })
}

fn custom(name: &str, url: &str) -> Value {
    json!({"name": name, "kind": "custom", "base_url": url, "secret": ""})
}

/// An app whose layer forbids private model URLs.
async fn strict_app() -> AppFx {
    let strict = Policy {
        private_model_urls: false,
        ..Policy::LOCAL
    };
    let spy = SpyLayer::new().with_policy(strict);
    let fx = AppFx::idle("keys", "reach-hosted")
        .layer(Arc::new(spy))
        .start()
        .await;
    with_vault(&fx);
    fx
}

#[test]
fn private_urls_are_fine_here_and_refused_when_the_policy_says_so() {
    run(async {
        let fx = app("reach-local").await;
        let (status, v) = fx
            .post(
                "/api/models/keys",
                custom("box", "http://127.0.0.1:8000/v1"),
            )
            .await;
        assert_eq!(
            status, 200,
            "a vLLM on this machine is the normal case here: {v}"
        );
        let fx = strict_app().await;
        for url in [
            "http://models.example.com/v1",
            "https://10.0.0.5/v1",
            "https://169.254.169.254/v1",
            "https://[fc00::1]/v1",
            "https://127.0.0.1/v1",
        ] {
            let (status, v) = fx.post("/api/models/keys", custom("srv", url)).await;
            assert_eq!(status, 400, "{url}: {v}");
        }
        let (_, list) = fx.get("/api/models/keys").await;
        assert_eq!(list, json!([]));
    })
}

/// M1: the auth scheme is matched without case, so `bearer sd_…` is the token, capped by its
/// scopes, and never the person at this machine; an Authorization header that is no Bearer is
/// refused rather than read as no token at all.
#[test]
fn any_case_of_bearer_is_the_token_and_another_scheme_is_refused() {
    run(async {
        let fx = app("scheme").await;
        let (_, made) = fx
            .post("/api/tokens", json!({"name": "r", "scopes": ["read"]}))
            .await;
        let token = made["token"].as_str().unwrap();
        let url = format!("{}/api/models/keys", fx.base);
        let cases = [
            (format!("bearer {token}"), 403),
            (format!("BEARER  {token}"), 403),
            (token.to_string(), 401),
            (format!("Token {token}"), 401),
            ("Basic eDp5".to_string(), 401),
        ];
        for (auth, want) in cases {
            let req = fx.http.post(&url).header("authorization", &auth);
            let r = req
                .json(&openai_key("mine", "sk-x-9999"))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), want, "{auth}");
        }
        let (_, list) = fx.get("/api/models/keys").await;
        assert_eq!(list, json!([]));
    })
}

#[test]
fn a_read_token_cannot_add_keys() {
    run(async {
        let fx = app("scope").await;
        for scope in ["read", "run"] {
            let (_, made) = fx
                .post("/api/tokens", json!({"name": scope, "scopes": [scope]}))
                .await;
            let token = made["token"].as_str().unwrap();
            let req = fx.http.post(format!("{}/api/models/keys", fx.base));
            let req = req
                .bearer_auth(token)
                .json(&openai_key("mine", "sk-x-9999"));
            assert_eq!(req.send().await.unwrap().status(), 403, "{scope}");
        }
        let (_, list) = fx.get("/api/models/keys").await;
        assert_eq!(list, json!([]));
    })
}

/// Adds key `body`: its id.
async fn added(fx: &AppFx, body: Value) -> Uuid {
    let (status, made) = fx.post("/api/models/keys", body).await;
    assert_eq!(status, 200, "{made}");
    made["id"].as_str().unwrap().parse().unwrap()
}

/// Adds `their-openai` on its own server at `base`, and `team-claude`: team-claude's id.
async fn add_shared(fx: &AppFx, base: &str) -> Uuid {
    let proxy = json!({"name": "their-openai", "kind": "openai", "secret": "sk-theirs-1111",
                       "base_url": base});
    added(fx, proxy).await;
    let claude = json!({"name": "team-claude", "kind": "anthropic", "secret": "sk-ant-team-2222"});
    added(fx, claude).await
}

/// A shared key (another member's, visible to the caller) applies only when an agent names it
/// (`key:<name>`), and never one that names its own server: its owner's server would receive
/// everyone's prompts. Where every key is the caller's own (here), they apply by kind as before.
#[test]
fn a_shared_key_applies_only_when_named_and_never_with_its_own_server() {
    run(async {
        let llm = fake_llm(vec![]).await;
        let fx = AppFx::idle("keys", "shared")
            .layer(Arc::new(SpyLayer::new().sharing_keys()))
            .start()
            .await;
        with_vault(&fx);
        let claude = add_shared(&fx, &llm.base).await;
        let for_ = |asked: &'static str| provider_for(&fx.state, local(), Some(asked), "m");
        let (p, paid) = for_("openai").await;
        assert!(p.unwrap().endpoint.key.is_none(), "not applied by kind");
        assert_eq!(paid, KeySource::Environment);
        let (p, _) = for_("anthropic").await;
        assert!(p.unwrap().endpoint.key.is_none(), "not applied by kind");
        let (p, paid) = for_("key:team-claude").await;
        assert_eq!(paid, KeySource::Own(claude));
        let p = p.unwrap();
        assert_eq!(p.id, "anthropic");
        assert_eq!(key_of(Ok(p)), "sk-ant-team-2222");
        let (p, _) = for_("key:their-openai").await;
        let e = p.unwrap_err();
        assert!(e.contains("its own server"), "{e}");
    })
}

/// Locally every key is the person's own: it applies by kind, and also answers to its name.
#[test]
fn your_own_key_applies_by_kind_and_by_name() {
    run(async {
        let fx = app("ownbyname").await;
        let id = added(&fx, openai_key("mine", "k-own-3333")).await;
        for asked in ["openai", "key:mine"] {
            let (p, paid) = provider_for(&fx.state, local(), Some(asked), "m").await;
            assert_eq!(
                (paid, key_of(p)),
                (KeySource::Own(id), "k-own-3333".into()),
                "{asked}"
            );
        }
    })
}
