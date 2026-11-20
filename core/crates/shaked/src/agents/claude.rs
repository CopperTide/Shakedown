//! Claude Code CLI driver (`claude -p`), signed in with the user's subscription.
//!
//! Flags (confirmed live against claude 2.1.314):
//!   -p <prompt> --model <m> --output-format json --setting-sources ""
//!   --strict-mcp-config --mcp-config '{"mcpServers":{"<server>":{"type":"http","url":..}}}'
//!   --disable-slash-commands --tools "" --allowedTools "mcp__<server>__*"
//! `<server>` is the benchmark's name (`super::server_name`).
//! `--tools ""` removes the built-in tools; `--allowedTools` pre-approves the MCP tools so
//! no permission prompt blocks the non-interactive run. A dispatch run's session takes
//! `--input-format stream-json --output-format stream-json --verbose` instead of `--output-format
//! json` and the prompt (confirmed live against claude 2.1.319).

use std::{future::Future, pin::Pin, process::Command};

use serde_json::{json, Value};

use super::{
    session::{Session, SessionStart},
    AgentResult, AgentRun, Driver, Mcp, CLI_TIMEOUT_SECS,
};
use crate::model::{AgentSpec, TokenUsage};

pub struct ClaudeDriver {
    pub self_base: String,
    pub binary: String,
    pub timeout_secs: u64,
}

impl ClaudeDriver {
    pub fn new(self_base: impl Into<String>) -> ClaudeDriver {
        ClaudeDriver {
            self_base: self_base.into(),
            binary: "claude".into(),
            timeout_secs: CLI_TIMEOUT_SECS,
        }
    }
}

pub fn command(spec: &AgentSpec, mcp: &Mcp, prompt: &str) -> Command {
    command_with("claude", spec, mcp, prompt)
}

fn command_with(binary: &str, spec: &AgentSpec, mcp: &Mcp, prompt: &str) -> Command {
    command_for(binary, spec, mcp, &inline_config(mcp), prompt)
}

/// The command for a run in `dir`: an agent with a token gets its MCP config in a 0600 file there
/// (in argv the token would show in `ps`).
pub fn command_in(
    binary: &str,
    spec: &AgentSpec,
    mcp: &Mcp,
    prompt: &str,
    dir: &std::path::Path,
) -> std::io::Result<Command> {
    Ok(command_for(
        binary,
        spec,
        mcp,
        &config_arg(mcp, dir)?,
        prompt,
    ))
}

fn command_for(binary: &str, spec: &AgentSpec, mcp: &Mcp, config: &str, prompt: &str) -> Command {
    let mut c = isolated(binary, spec, mcp, config);
    // The prompt is positional and may start with "-": keep it apart from the flags.
    c.args(["--output-format", "json"]).args(["--", prompt]);
    c
}

/// The MCP config inline (no token).
fn inline_config(mcp: &Mcp) -> String {
    json!({"mcpServers": {&mcp.server: {"type": "http", "url": mcp.url}}}).to_string()
}

/// `--mcp-config`'s value: inline without a token; with one, a 0600 file in `dir` whose server
/// entry carries the `Authorization` header (its path).
fn config_arg(mcp: &Mcp, dir: &std::path::Path) -> std::io::Result<String> {
    let Some(token) = &mcp.token else {
        return Ok(inline_config(mcp));
    };
    let headers = json!({"Authorization": format!("Bearer {token}")});
    let config =
        json!({"mcpServers": {&mcp.server: {"type": "http", "url": mcp.url, "headers": headers}}});
    let path = dir.join(".shakedown-mcp.json");
    write_private(&path, config.to_string().as_bytes())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Writes `bytes` to a new file only its owner may read.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)
}

/// Claude Code headless with only the world's MCP tools (`config`), and none of the user's
/// settings, sessions, slash commands or built-in tools; its environment scrubbed of other secrets.
fn isolated(binary: &str, spec: &AgentSpec, mcp: &Mcp, config: &str) -> Command {
    let mut c = Command::new(binary);
    c.arg("-p")
        .arg("--model")
        .arg(spec.model.as_deref().unwrap_or("sonnet"))
        .args(["--setting-sources", ""])
        .arg("--strict-mcp-config")
        .arg("--mcp-config")
        .arg(config)
        .arg("--disable-slash-commands")
        .args(["--tools", ""])
        .arg("--allowedTools")
        .arg(format!("mcp__{}__*", mcp.server))
        .arg("--no-session-persistence");
    super::scrub_env(&mut c, "ANTHROPIC_API_KEY");
    c
}

/// A long-lived session (see `claude_session`): the isolated command taking its turns as a stream
/// of user messages on stdin and answering in stream-json; its MCP config in `dir`.
pub fn session_command(
    binary: &str,
    spec: &AgentSpec,
    mcp: &Mcp,
    dir: &std::path::Path,
) -> std::io::Result<Command> {
    let mut c = isolated(binary, spec, mcp, &config_arg(mcp, dir)?);
    c.args(["--input-format", "stream-json"])
        .args(["--output-format", "stream-json"])
        .arg("--verbose");
    Ok(c)
}

/// Parse `--output-format json` stdout. `seconds` is left 0 for the caller.
pub fn parse(stdout: &str) -> AgentResult {
    let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) else {
        return super::fail(format!(
            "unparseable claude output: {}",
            super::tail(stdout)
        ));
    };
    let u = &v["usage"];
    let n = |k: &str| u[k].as_i64().unwrap_or(0);
    let prompt =
        n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens");
    let completion = n("output_tokens");
    let is_error = v["is_error"].as_bool().unwrap_or(false);
    let text = v["result"].as_str().unwrap_or("").to_string();
    AgentResult {
        ok: !is_error,
        error: is_error.then(|| {
            if text.is_empty() {
                "claude reported an error".into()
            } else {
                text
            }
        }),
        tokens: TokenUsage::summed(prompt, completion),
        cost_usd: v["total_cost_usd"].as_f64(),
        cost_source: Some("api_equivalent_estimate".into()),
        seconds: 0.0,
        failure: None,
    }
}

impl Driver for ClaudeDriver {
    fn session(&self, start: SessionStart) -> Result<Box<dyn Session>, String> {
        let s = super::claude_session::ClaudeSession::start(&self.self_base, &self.binary, start)?;
        Ok(Box::new(s))
    }

    fn run<'a>(&'a self, run: AgentRun) -> Pin<Box<dyn Future<Output = AgentResult> + Send + 'a>> {
        Box::pin(async move {
            let mcp = Mcp::of(&self.self_base, &run);
            let prompt = super::prompt_of(&run);
            let build = |workdir: &std::path::Path| {
                let mut cmd = command_in(&self.binary, &run.spec, &mcp, &prompt, workdir)
                    .unwrap_or_else(|_| command_with(&self.binary, &run.spec, &mcp, &prompt));
                cmd.current_dir(workdir);
                cmd
            };
            super::run_in_workdir(&run, self.timeout_secs, build, |out, _| parse(out)).await
        })
    }
}
