use std::{future::Future, pin::Pin, sync::Arc};

use serde_json::Value;

use crate::{
    exec::BranchCtx,
    model::{AgentSpec, TokenUsage},
};

pub mod claude;
mod claude_session;
mod cli;
pub mod codex;
pub mod endpoint_auth;
pub mod http;
pub mod jail;
pub mod open;
pub mod session;
mod usage;

pub(crate) use cli::{fresh_workdir, run_in_workdir, scrub_env, tail};
pub use cli::{server_name, Mcp, ModelRoute, CLI_TIMEOUT_SECS};

/// One agent's turn in a branch.
pub struct AgentRun {
    /// Shared with the MCP endpoint so both use the same per-branch gate.
    pub branch: Arc<BranchCtx>,
    pub spec: AgentSpec,
    /// The first user message.
    pub input: String,
    /// A replayed message list (system message is replaced by `spec.prompt`).
    pub resume: Option<Vec<Value>>,
}

/// Why an agent failed, when it is not an infrastructure problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The model's own behavior (too many calls, output cut off): the branch is `failed`.
    Model,
    /// The run's spend estimate passed its limit (`crate::budget`): the branch is
    /// `budget_exceeded`, scored as it stands.
    Budget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentResult {
    pub ok: bool,
    pub error: Option<String>,
    /// Summed over the agent's model calls.
    pub tokens: TokenUsage,
    /// Summed cost; None if any call lacked a cost.
    pub cost_usd: Option<f64>,
    pub cost_source: Option<String>,
    pub seconds: f64,
    /// `None` on failure means an infrastructure error (CLI or gateway/upstream problem).
    pub failure: Option<FailureKind>,
}

impl AgentResult {
    /// A failure with nothing counted, not put down to the model (`failure: None`).
    pub(crate) fn failed(error: impl Into<String>) -> AgentResult {
        AgentResult {
            ok: false,
            error: Some(error.into()),
            tokens: TokenUsage::default(),
            cost_usd: None,
            cost_source: None,
            seconds: 0.0,
            failure: None,
        }
    }
}

pub trait Driver: Send + Sync {
    fn run<'a>(&'a self, ctx: AgentRun) -> Pin<Box<dyn Future<Output = AgentResult> + Send + 'a>>;

    /// Starts the agent as a session that stays alive across turns (for the dispatch runner);
    /// an error when this kind of agent can't keep one.
    fn session(&self, start: session::SessionStart) -> Result<Box<dyn session::Session>, String> {
        Err(format!(
            "a {:?} agent can't keep a session across turns, which a dispatch run needs",
            start.spec.kind
        ))
    }
}

pub(crate) fn prompt_of(run: &AgentRun) -> String {
    format!("{}\n\n{}", run.spec.prompt, run.input)
}

/// `agent`'s endpoint on `branch` under `route` (`mcp` or `gateway`), on the shaked at
/// `self_base`: `{self_base}/{route}/{branch}/{agent}`.
pub(crate) fn endpoint(self_base: &str, route: &str, branch: &str, agent: &str) -> String {
    format!(
        "{}/{route}/{branch}/{agent}",
        self_base.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use crate::model::TokenUsage;

    #[test]
    fn an_agents_tokens_are_openai_shaped() {
        let t = TokenUsage::summed(16, 4);
        let want = serde_json::json!({"prompt_tokens": 16, "completion_tokens": 4,
                                      "total_tokens": 20});
        assert_eq!(serde_json::to_value(t).unwrap(), want);
        assert_eq!(serde_json::from_value::<TokenUsage>(want).unwrap(), t);
    }
}
