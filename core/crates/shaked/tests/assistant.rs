//! The assistant: a chat loop over the operation registry on the user's own model. Reads run,
//! edits come back as diff cards, and spending or deleting comes back as a confirm card.

mod support;

use serde_json::{json, Value};
use shaked::providers::{Endpoint, ProviderConfig, Secret};
use support::fake_llm::{fake_llm, FakeLlm};
use support::{run, AppFx};

fn answer(message: Value) -> (u16, Value) {
    let finish = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else {
        "stop"
    };
    (
        200,
        json!({"id": "c", "object": "chat.completion", "model": "m",
        "choices": [{"index": 0, "finish_reason": finish, "message": message}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}}),
    )
}

fn tool_call(name: &str, args: Value) -> (u16, Value) {
    answer(json!({"role": "assistant", "content": null,
        "tool_calls": [{"id": "t1", "type": "function", "function": {"name": name, "arguments": args.to_string()}}]}))
}

fn text(content: &str) -> (u16, Value) {
    answer(json!({"role": "assistant", "content": content}))
}

/// The example project with the assistant on model `m` of provider `openai`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    let config = dir.path().join("shakedown.yaml");
    let text = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        config,
        format!("{text}assistant:\n  model: m\n  provider: openai\n"),
    )
    .unwrap();
    dir
}

async fn app(tag: &str, llm: &FakeLlm, dir: &std::path::Path) -> AppFx {
    let providers = ProviderConfig {
        openai: Endpoint::new(&llm.base, Some(Secret::new("k"))),
        ..ProviderConfig::default()
    };
    AppFx::idle("assistant", tag)
        .providers(providers)
        .project(dir)
        .start()
        .await
}

async fn ask(fx: &AppFx, said: &str) -> (u16, Value) {
    let body = json!({"messages": [{"role": "user", "content": said}], "page": "Home"});
    fx.post("/api/assistant/chat", body).await
}

#[test]
fn reads_run_and_answer() {
    run(async {
        let llm = fake_llm(vec![
            tool_call("worlds_list", json!({})),
            text("One world."),
        ])
        .await;
        let dir = project();
        let fx = app("reads", &llm, dir.path()).await;
        let (status, out) = ask(&fx, "How many worlds?").await;
        assert_eq!(status, 200, "{out}");
        assert_eq!(out["reply"], "One world.");
        assert_eq!(out["tools"][0]["name"], "worlds_list");
        let first = &llm.seen.lock().unwrap()[0].body;
        let tools: Vec<&str> = first["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(tools.contains(&"scenarios_put"), "{tools:?}");
        assert_eq!(first["model"], "m");
    });
}

fn renamed_scenario() -> Value {
    json!({"id": "supplier-slip", "body": {"format": "shakedown-scenario/1", "label": "Renamed",
        "world": "larkspur-plant", "task": "Keep SO-1001 on schedule."}})
}

#[test]
fn edit_becomes_a_diff_card() {
    run(async {
        let llm = fake_llm(vec![
            tool_call("scenarios_put", renamed_scenario()),
            text("Here is the change."),
        ])
        .await;
        let dir = project();
        let before =
            std::fs::read_to_string(dir.path().join("scenarios/supplier-slip.yaml")).unwrap();
        let fx = app("diff", &llm, dir.path()).await;
        let (status, out) = ask(&fx, "Rename it").await;
        assert_eq!(status, 200, "{out}");
        assert_eq!(out["cards"][0]["kind"], "diff", "{out}");
        assert!(out["cards"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("+label: Renamed"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("scenarios/supplier-slip.yaml")).unwrap(),
            before
        );
    });
}

#[test]
fn spend_becomes_a_confirm_card() {
    run(async {
        let deploy = tool_call(
            "experiments_deploy",
            json!({"id": "supplier-slip-solo-vs-team"}),
        );
        let llm = fake_llm(vec![deploy, text("Confirm to start the runs.")]).await;
        let dir = project();
        let fx = app("confirm", &llm, dir.path()).await;
        let (status, out) = ask(&fx, "Run it").await;
        assert_eq!(status, 200, "{out}");
        assert_eq!(out["cards"][0]["kind"], "confirm");
        assert_eq!(out["cards"][0]["tool"], "experiments_deploy");
        let found = support::records(&fx.state.store)
            .deployments_of("supplier-slip-solo-vs-team")
            .await
            .unwrap();
        assert!(found.is_empty(), "nothing may run without a click");
    });
}

#[test]
fn unknown_tool_is_a_tool_error() {
    run(async {
        let llm = fake_llm(vec![tool_call("no_such_tool", json!({})), text("Sorry.")]).await;
        let dir = project();
        let fx = app("unknown", &llm, dir.path()).await;
        let (status, out) = ask(&fx, "Do it").await;
        assert_eq!(status, 200, "{out}");
        assert_eq!(out["reply"], "Sorry.");
        let second = &llm.seen.lock().unwrap()[1].body;
        let tool_msg = second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool")
            .unwrap();
        assert!(tool_msg["content"]
            .as_str()
            .unwrap()
            .contains("no tool named no_such_tool"));
    });
}

#[test]
fn stops_after_eight_turns() {
    run(async {
        let llm = fake_llm(vec![tool_call("worlds_list", json!({}))]).await;
        let dir = project();
        let fx = app("loop", &llm, dir.path()).await;
        let (status, out) = ask(&fx, "Loop").await;
        assert_eq!(status, 200, "{out}");
        assert!(out["reply"]
            .as_str()
            .unwrap()
            .contains("Stopped after 8 steps"));
        assert_eq!(llm.seen.lock().unwrap().len(), 8);
    });
}

#[test]
fn no_model_is_explained() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        support::copy_dir(&support::project_fixture(), dir.path());
        let fx = AppFx::idle("assistant", "nomodel")
            .providers(ProviderConfig::default())
            .project(dir.path())
            .start()
            .await;
        let (status, out) = ask(&fx, "Hi").await;
        assert_eq!(status, 409, "{out}");
        assert!(
            out["error"]
                .as_str()
                .unwrap()
                .contains("The assistant needs a model"),
            "{out}"
        );
    });
}

#[test]
fn apply_diff_card_writes() {
    run(async {
        let llm = fake_llm(vec![
            tool_call("scenarios_put", renamed_scenario()),
            text("Apply it if it looks right."),
        ])
        .await;
        let dir = project();
        let fx = app("apply", &llm, dir.path()).await;
        let (_, out) = ask(&fx, "Rename it").await;
        let mut args = out["cards"][0]["arguments"].clone();
        args["confirm"] = json!(true);
        let id = args["id"].as_str().unwrap().to_string();
        let res = fx
            .http
            .put(format!("{}/api/project/scenarios/{id}", fx.base))
            .json(&args)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 200);
        let text =
            std::fs::read_to_string(dir.path().join("scenarios/supplier-slip.yaml")).unwrap();
        assert!(text.contains("label: Renamed"), "{text}");
    });
}

#[test]
fn read_token_gets_cards_not_writes() {
    run(async {
        let llm = fake_llm(vec![
            tool_call("scenarios_list", json!({})),
            text("Listed."),
        ])
        .await;
        let dir = project();
        let fx = app("readtoken", &llm, dir.path()).await;
        let (_, minted) = fx
            .post("/api/tokens", json!({"name": "r", "scopes": ["read"]}))
            .await;
        let token = minted["token"].as_str().unwrap().to_string();
        let body = json!({"messages": [{"role": "user", "content": "List"}]});
        let res = fx
            .http
            .post(format!("{}/api/assistant/chat", fx.base))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 200);
        let put = fx
            .http
            .put(format!("{}/api/project/scenarios/supplier-slip", fx.base))
            .bearer_auth(&token)
            .json(&json!({"body": renamed_scenario()["body"], "confirm": true}))
            .send()
            .await
            .unwrap();
        assert_eq!(put.status().as_u16(), 403);
    });
}
