//! `setups/<id>.yaml`: an agent team to test, built any way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Auth, Format};

/// An agent setup: where the team comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetupFile {
    /// Always `shakedown-setup/1`.
    pub format: String,
    /// The setup's display name.
    pub label: String,
    /// The team.
    pub team: Team,
}

impl Format for SetupFile {
    const FORMAT: &'static str = "shakedown-setup/1";
}

/// Where a team comes from. A team described in MADL gets the full graph and checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Team {
    /// Shakedown's built-in agents.
    Builtin {
        /// The agents.
        agents: Vec<AgentSpec>,
    },
    /// Your own code, started as a command.
    Code {
        /// The command, e.g. `uv run python crew.py`.
        command: String,
    },
    /// An agent already deployed behind HTTP.
    Http {
        /// Where tasks are posted.
        url: String,
        /// Where the credential comes from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<Auth>,
    },
    /// Claude Code, headless.
    ClaudeCode,
    /// Codex, headless.
    Codex,
    /// A team described in MADL, in a file next to this one.
    Madl {
        /// The MADL file's name, e.g. `team.madl.yaml`.
        file: String,
    },
}

/// One built-in agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    /// Unique within the team.
    pub id: String,
    /// The role prompt.
    pub role: String,
    /// The model, e.g. `claude-sonnet`.
    pub model: String,
    /// The key-store name of the model key.
    pub key: String,
    /// Tools it may call; `*` for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}
