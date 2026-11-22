//! A run the test drives itself, as an external system does, without a model: it starts the run,
//! registers its agent, calls the world through the agent's MCP endpoint, finishes, forks.
use serde_json::{json, Value};
use shaked::model::Event;

use super::{records, AppFx};

/// An agent's endpoint on a live branch: where it calls the world, and its token.
pub struct Agent {
    pub mcp_url: String,
    pub token: String,
}

impl Agent {
    pub fn of(answer: &Value) -> Agent {
        Agent {
            mcp_url: answer["mcp_url"].as_str().unwrap().to_string(),
            token: answer["token"].as_str().unwrap().to_string(),
        }
    }

    async fn rpc(&self, method: &str, params: Value) -> Value {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let r = reqwest::Client::new()
            .post(&self.mcp_url)
            .bearer_auth(&self.token);
        let v: Value = r.json(&body).send().await.unwrap().json().await.unwrap();
        v["result"].clone()
    }

    /// A tool call's result (`isError`, `content`, `structuredContent`).
    pub async fn call(&self, name: &str, args: Value) -> Value {
        let params = json!({"name": name, "arguments": args});
        self.rpc("tools/call", params).await
    }

    /// The names of the tools it is offered.
    pub async fn tools(&self) -> Vec<String> {
        let l = self.rpc("tools/list", json!({})).await;
        let names = l["tools"].as_array().unwrap().iter();
        names
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }
}

/// `POST /api/branches/{branch}/{what}`.
pub async fn on(fx: &AppFx, branch: &str, what: &str, body: Value) -> (u16, Value) {
    fx.post(&format!("/api/branches/{branch}/{what}"), body)
        .await
}

/// Starts an external run of task 27 (with the run request's `extra` fields, such as a
/// `scenario`) and registers the agent `solo`: the branch and the agent.
pub async fn start(fx: &AppFx, extra: Value) -> (String, Agent) {
    let team = json!({"id": "own", "name": "Own design", "agents": []});
    let mut body = json!({"task_id": "27", "team": team, "runner": "external"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    let branch = v["branch_id"].as_str().unwrap().to_string();
    let (status, a) = on(fx, &branch, "agents", json!({"id": "solo"})).await;
    assert_eq!(status, 200, "{a}");
    (branch, Agent::of(&a))
}

pub async fn finish(fx: &AppFx, branch: &str) {
    let (status, v) = on(fx, branch, "finish", json!({})).await;
    assert_eq!(status, 200, "{v}");
}

pub async fn events(fx: &AppFx, branch: &str) -> Vec<Event> {
    records(&fx.state.store).events(branch).await.unwrap()
}

/// Forks `branch` as `body` asks: the new branch and its agent `solo`.
pub async fn fork(fx: &AppFx, branch: &str, body: Value) -> (String, Agent) {
    let (status, f) = on(fx, branch, "fork", body).await;
    assert_eq!(status, 200, "{f}");
    let agents = f["agents"].as_array().unwrap();
    let solo = agents.iter().find(|a| a["agent"] == "solo").unwrap();
    let id = f["branch_id"].as_str().unwrap().to_string();
    (id, Agent::of(solo))
}
