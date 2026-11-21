//! The control MCP server: every registry operation is a tool, over HTTP (`POST /mcp`) and over
//! stdio (`shaked mcp`), and a token can only call the tools its scope covers.

mod support;

use std::io::{BufRead, BufReader, Write};
use std::process::Command;

use serde_json::{json, Value};
use support::{run, AppFx};

fn larkspur() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    dir
}

async fn rpc(fx: &AppFx, token: Option<&str>, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let mut req = fx.http.post(format!("{}/mcp", fx.base)).json(&body);
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    let res = req.send().await.unwrap();
    assert_eq!(res.status().as_u16(), 200);
    res.json().await.unwrap()
}

#[test]
fn tools_list_matches_registry() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("cmcp", "list")
            .project(dir.path())
            .start()
            .await;
        let init = rpc(&fx, None, "initialize", json!({})).await;
        assert_eq!(init["result"]["serverInfo"]["name"], "shakedown");
        let listed = rpc(&fx, None, "tools/list", json!({})).await;
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), shaked::ops::all().len());
        let put = tools
            .iter()
            .find(|t| t["name"] == "scenarios_put")
            .expect("scenarios_put");
        assert_eq!(put["inputSchema"]["type"], "object");
        let get = tools.iter().find(|t| t["name"] == "scenarios_get").unwrap();
        assert_eq!(get["annotations"]["readOnlyHint"], true);
    });
}

#[test]
fn call_worlds_list() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("cmcp", "call")
            .project(dir.path())
            .start()
            .await;
        let params = json!({"name": "worlds_list", "arguments": {}});
        let out = rpc(&fx, None, "tools/call", params).await;
        assert_eq!(out["result"]["isError"], false, "{out}");
        assert_eq!(
            out["result"]["structuredContent"]["items"][0]["id"],
            "larkspur-plant"
        );
    });
}

#[test]
fn tool_failure_is_an_error_result() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("cmcp", "fail")
            .project(dir.path())
            .start()
            .await;
        let params = json!({"name": "scenarios_get", "arguments": {"id": "nope"}});
        let out = rpc(&fx, None, "tools/call", params).await;
        assert_eq!(out["result"]["isError"], true, "{out}");
        let unknown = rpc(&fx, None, "tools/call", json!({"name": "no_such_tool"})).await;
        assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
    });
}

async fn mint_token(fx: &AppFx, scope: &str) -> String {
    let (_, minted) = fx
        .post("/api/tokens", json!({"name": "reader", "scopes": [scope]}))
        .await;
    minted["token"].as_str().expect("a token").to_string()
}

fn put_call(id: &str) -> Value {
    let body = json!({"format": "shakedown-scenario/1", "label": "X",
                      "world": "larkspur-plant", "task": "T"});
    json!({"name": "scenarios_put", "arguments": {"id": id, "body": body, "confirm": true}})
}

#[test]
fn read_token_cannot_put() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("cmcp", "scope")
            .project(dir.path())
            .start()
            .await;
        let token = mint_token(&fx, "read").await;
        let list = json!({"name": "setups_list", "arguments": {}});
        let ok = rpc(&fx, Some(&token), "tools/call", list).await;
        assert_eq!(ok["result"]["isError"], false, "{ok}");
        let refused = rpc(&fx, Some(&token), "tools/call", put_call("x")).await;
        assert_eq!(refused["error"]["code"], -32001, "{refused}");
        let message = refused["error"]["message"].as_str().unwrap();
        assert!(message.contains("admin"), "{refused}");
        assert!(!dir.path().join("scenarios/x.yaml").exists());
    });
}

#[test]
fn notifications_get_no_reply() {
    run(async {
        let fx = AppFx::idle("cmcp", "notify").start().await;
        let body = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        let res = fx
            .http
            .post(format!("{}/mcp", fx.base))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 202);
    });
}

/// `shaked mcp --project dir`, with a way to send one message and read its reply.
struct Stdio {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    lines: std::io::Lines<BufReader<std::process::ChildStdout>>,
}

impl Stdio {
    fn start(dir: &std::path::Path) -> Stdio {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shaked"))
            .env("SHAKEDOWN_TEST_GUARD", "1")
            .args(["mcp", "--project"])
            .arg(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("shaked starts");
        let stdin = child.stdin.take();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Stdio {
            child,
            stdin,
            lines,
        }
    }

    fn ask(&mut self, msg: Value) -> Value {
        writeln!(self.stdin.as_mut().unwrap(), "{msg}").unwrap();
        serde_json::from_str(&self.lines.next().unwrap().unwrap()).unwrap()
    }

    fn finish(mut self) -> bool {
        drop(self.stdin.take());
        self.child.wait().unwrap().success()
    }
}

fn tool_names(listed: &Value) -> Vec<String> {
    let tools = listed["result"]["tools"].as_array().unwrap();
    tools
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn stdio_round_trip() {
    let dir = larkspur();
    let mut mcp = Stdio::start(dir.path());
    let init = mcp.ask(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "shakedown");
    let listed = mcp.ask(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert!(tool_names(&listed).iter().any(|n| n == "worlds_list"));
    let call = json!({"name": "setups_list", "arguments": {}});
    let called =
        mcp.ask(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": call}));
    let items = called["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(items.len(), 3);
    assert!(mcp.finish());
}
