//! Every agent carries a token that dies with it: no MCP or model-gateway call gets in without
//! one, CLI agents get theirs outside argv, and run and worker tokens are bound to their branch.

use std::os::unix::fs::PermissionsExt;

use serde_json::{json, Value};
use shaked::{
    agents::{claude, codex, Mcp},
    auth::{run_tokens::resolve_run_token, Principal, RunTokenKind},
    model::{AgentKind, AgentSpec},
};
use support::{pg, records, run, AppFx};

mod support;

const MCP_LIST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

/// An external run with one registered agent `a`: the branch and the agent's register answer.
async fn external_agent(fx: &AppFx) -> (String, Value) {
    let team = json!({"id": "t", "name": "T", "agents": []});
    let (status, v) = fx
        .post(
            "/api/runs",
            json!({"task_id": "27", "team": team, "runner": "external"}),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let (status, reg) = fx
        .post(
            &format!("/api/branches/{branch}/agents"),
            json!({"id": "a"}),
        )
        .await;
    assert_eq!(status, 200, "{reg}");
    (branch, reg)
}

async fn post(fx: &AppFx, url: &str, token: Option<&str>, body: &str) -> u16 {
    let mut req = fx
        .http
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string());
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    req.send().await.unwrap().status().as_u16()
}

#[test]
fn tokenless_mcp_and_gateway_calls_get_401() {
    run(async {
        let fx = AppFx::idle("runtokens", "tokenless").start().await;
        let (branch, reg) = external_agent(&fx).await;
        let token = reg["token"]
            .as_str()
            .expect("the register answer has the agent's token");
        assert!(token.starts_with("sda_"), "{token}");
        let mcp = reg["mcp_url"].as_str().unwrap();
        assert_eq!(
            post(&fx, mcp, None, MCP_LIST).await,
            401,
            "mcp without a token"
        );
        assert_eq!(
            post(&fx, mcp, Some("sda_nope"), MCP_LIST).await,
            401,
            "a wrong one"
        );
        assert_eq!(
            post(&fx, mcp, Some(token), MCP_LIST).await,
            200,
            "with its token"
        );
        let ctx = fx.state.live.get(&branch).unwrap();
        assert_eq!(ctx.roster.token_of("a").as_deref(), Some(token));
    })
}

#[test]
fn a_tokenless_gateway_call_gets_401() {
    run(async {
        let fx = AppFx::idle("runtokens", "gateway").start().await;
        let (_, reg) = external_agent(&fx).await;
        let chat = format!("{}/chat/completions", reg["gateway_url"].as_str().unwrap());
        assert_eq!(post(&fx, &chat, None, "{}").await, 401);
        assert_eq!(post(&fx, &chat, Some("sda_nope"), "{}").await, 401);
    })
}

#[test]
fn tokens_die_with_the_branch_and_never_reach_events() {
    run(async {
        let fx = AppFx::idle("runtokens", "die").start().await;
        let (branch, reg) = external_agent(&fx).await;
        let (token, mcp) = (
            reg["token"].as_str().unwrap(),
            reg["mcp_url"].as_str().unwrap(),
        );
        let (status, v) = fx
            .post(&format!("/api/branches/{branch}/finish"), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        assert_ne!(
            post(&fx, mcp, Some(token), MCP_LIST).await,
            200,
            "a token dies with its branch"
        );
        let events = records(&fx.state.store).events(&branch).await.unwrap();
        let text = serde_json::to_string(&events).unwrap();
        for prefix in ["sd_", "sdr_", "sda_"] {
            assert!(!text.contains(prefix), "{prefix} in the events");
        }
    })
}

fn spec() -> AgentSpec {
    AgentSpec::new("triage", AgentKind::ClaudeCode)
}

fn with_token() -> Mcp {
    Mcp {
        server: "bench".into(),
        url: "http://127.0.0.1:1/mcp/b1/triage".into(),
        token: Some("sda_secret".into()),
    }
}

#[test]
fn claude_gets_its_token_in_a_private_config_file() {
    let dir = tempfile_dir("claude");
    let c = claude::command_in("claude", &spec(), &with_token(), "do it", &dir).unwrap();
    let a: Vec<String> = c
        .get_args()
        .map(|x| x.to_string_lossy().into_owned())
        .collect();
    assert!(
        !a.iter().any(|x| x.contains("sda_secret")),
        "the token is not in argv: {a:?}"
    );
    let i = a.iter().position(|x| x == "--mcp-config").unwrap();
    let path = std::path::Path::new(&a[i + 1]);
    assert!(path.starts_with(&dir));
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let cfg: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        cfg["mcpServers"]["bench"]["headers"]["Authorization"],
        "Bearer sda_secret"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn codex_reads_its_token_from_the_env() {
    let dir = tempfile_dir("codex");
    let c = codex::command_with("codex", &spec(), &with_token(), &dir, "do it");
    let a: Vec<String> = c
        .get_args()
        .map(|x| x.to_string_lossy().into_owned())
        .collect();
    assert!(!a.iter().any(|x| x.contains("sda_secret")), "{a:?}");
    let want = "mcp_servers.bench.bearer_token_env_var=\"SHAKEDOWN_AGENT_TOKEN\"";
    assert!(a.iter().any(|x| x == want), "{a:?}");
    let env = c
        .get_envs()
        .find(|(k, _)| *k == "SHAKEDOWN_AGENT_TOKEN")
        .and_then(|(_, v)| v);
    assert_eq!(
        env.map(|v| v.to_string_lossy().into_owned()).as_deref(),
        Some("sda_secret")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let d = std::env::temp_dir().join(format!("shaked-runtokens-{tag}-{id}"));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A run with one branch, in the local org: the run and the branch.
async fn one_branch(store: &shaked::store::Store) -> (String, String) {
    let team = shaked::model::Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![],
    };
    let r = records(store);
    let run_id = r.create_run("t", &team).await.unwrap();
    let b1 = r
        .create_branch(&run_id, None, &team, &json!([]), None)
        .await
        .unwrap();
    (run_id, b1)
}

/// A `fake` sandbox for branch `b1` of `run_id`: its id.
async fn sandbox(db: &str, run_id: &str, b1: &str) -> Option<uuid::Uuid> {
    let org = shaked::store::LOCAL_ORG_ID;
    let sql = format!(
        "INSERT INTO shakedown.sandboxes (id, org_id, run_id, branch_id, backend, image_digest)
         VALUES ('00000000-0000-0000-0000-0000000000f5', '{org}', '{run_id}', '{b1}', 'fake', 'sha256:x')"
    );
    pg::execute(db, &sql).await;
    Some(uuid::Uuid::from_u128(0xf5))
}

fn soon() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now() + chrono::Duration::hours(1)
}

#[test]
fn run_tokens_resolve_to_their_branch_until_revoked() {
    run(async {
        let (_name, store, _clean) = pg::fresh_store("runtokenresolve").await;
        let (run_id, b1) = one_branch(&store).await;
        let r = records(&store);
        let t = r
            .mint_run_token(&run_id, &b1, RunTokenKind::Run, None, None, soon())
            .await;
        let t = t.unwrap();
        assert!(t.starts_with("sdr_"));
        let got = resolve_run_token(&store, &t).await.unwrap();
        let Some(Principal::RunToken { branch, kind, .. }) = got else {
            panic!("{got:?}")
        };
        assert_eq!((branch, kind), (b1.clone(), RunTokenKind::Run));
        assert_eq!(r.revoke_run_tokens(&b1, None).await.unwrap(), 1);
        assert!(
            resolve_run_token(&store, &t).await.unwrap().is_none(),
            "revoked"
        );
    })
}

#[test]
fn a_worker_token_needs_its_sandbox() {
    run(async {
        let (name, store, _clean) = pg::fresh_store("workertoken").await;
        let (run_id, b1) = one_branch(&store).await;
        let r = records(&store);
        let kind = RunTokenKind::Worker;
        assert!(r
            .mint_run_token(&run_id, &b1, kind, None, None, soon())
            .await
            .is_err());
        let sb = sandbox(&name, &run_id, &b1).await;
        let w = r
            .mint_run_token(&run_id, &b1, kind, None, sb, soon())
            .await
            .unwrap();
        assert!(w.starts_with("sdw_"));
        let worker = resolve_run_token(&store, &w).await.unwrap();
        assert!(
            matches!(
                worker,
                Some(Principal::RunToken {
                    sandbox: Some(_),
                    ..
                })
            ),
            "{worker:?}"
        );
    })
}
