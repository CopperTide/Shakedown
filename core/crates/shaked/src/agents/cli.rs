//! What a CLI agent (Claude Code, Codex) runs with: the world's MCP endpoint, the gateway route of
//! its model calls in a `shaked worker`, an environment scrubbed of secrets, and the process run
//! with a time limit in a fresh work folder.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use super::{
    endpoint,
    jail::{confine, Jail},
    AgentResult, AgentRun,
};
use crate::{
    exec::BranchCtx,
    flow::FlowControl,
    providers::{LOCAL_KEY_PREFIX, LOCAL_MODELS_ENV},
};

pub const CLI_TIMEOUT_SECS: u64 = 900;
const STDERR_TAIL: usize = 2048;

/// Last `STDERR_TAIL` bytes of `s`, cut on a char boundary.
pub(crate) fn tail(s: &str) -> &str {
    crate::text::last_bytes(s, STDERR_TAIL)
}

/// Spawn `cmd` (stdin null, output captured, killed on drop) and wait up to `timeout_secs`, the
/// time the branch is paused (`flow`) left out.
/// `parse` maps (stdout, exit code) to a result; `seconds` is filled in here.
async fn run_cli(
    flow: &FlowControl,
    cmd: Command,
    timeout_secs: u64,
    parse: impl FnOnce(&str, i32) -> AgentResult,
) -> AgentResult {
    let start = Instant::now();
    let mut cmd = tokio::process::Command::from(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return AgentResult::failed(format!("failed to start CLI: {e}")),
    };
    let limit = Duration::from_secs(timeout_secs);
    // Time the branch spends paused doesn't count against the CLI's limit.
    let waited = flow.timeout(limit, child.wait_with_output()).await;
    let seconds = start.elapsed().as_secs_f64();
    let mut r = match waited {
        None => AgentResult::failed(format!("timed out after {timeout_secs} s")),
        Some(Err(e)) => AgentResult::failed(format!("CLI failed: {e}")),
        Some(Ok(out)) => finish_cli(&out, parse),
    };
    r.seconds = seconds;
    r
}

/// The result of a CLI that ran to completion; a failure carries the tail of its stderr.
fn finish_cli(out: &Output, parse: impl FnOnce(&str, i32) -> AgentResult) -> AgentResult {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let code = out.status.code().unwrap_or(-1);
    let mut r = parse(&stdout, code);
    if !r.ok && !stderr.trim().is_empty() {
        let msg = r
            .error
            .take()
            .unwrap_or_else(|| format!("exit code {code}"));
        r.error = Some(format!("{msg}\nstderr: {}", tail(stderr.trim())));
    }
    r
}

/// Runs a CLI agent in a fresh work folder (removed afterwards), in a container when the worker
/// has a `jail`. `build` makes the command for that folder; CLI agents can't resume.
pub(crate) async fn run_in_workdir(
    run: &AgentRun,
    jail: Option<&Jail>,
    timeout_secs: u64,
    build: impl FnOnce(&Path) -> Result<Command, String>,
    parse: impl FnOnce(&str, i32) -> AgentResult,
) -> AgentResult {
    if run.resume.is_some() {
        return AgentResult::failed("CLI agents cannot resume mid-run");
    }
    let workdir = match fresh_workdir() {
        Ok(d) => d,
        Err(e) => return AgentResult::failed(format!("cannot create workdir: {e}")),
    };
    let r = match build(&workdir).and_then(|cmd| confine(jail, cmd, &workdir)) {
        // The container (if any) is kept until the CLI is done.
        Ok((cmd, _container)) => run_cli(&run.branch.flow, cmd, timeout_secs, parse).await,
        Err(why) => AgentResult::failed(why),
    };
    let _ = std::fs::remove_dir_all(&workdir);
    r
}

pub(crate) fn fresh_workdir() -> std::io::Result<PathBuf> {
    let d = std::env::temp_dir().join(format!("shakedown-cli-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// Where a CLI agent finds the world's tools: shaked's MCP endpoint and the name it goes by.
pub struct Mcp {
    pub server: String,
    pub url: String,
    /// The agent's token (`sda_…`), sent as `Authorization: Bearer`; never put in argv.
    pub token: Option<String>,
}

impl Mcp {
    /// `run`'s endpoint on the shaked at `self_base`, named after the benchmark.
    pub(crate) fn of(self_base: &str, run: &AgentRun) -> Mcp {
        Mcp::for_agent(self_base, &run.branch, &run.spec.id)
    }

    /// `agent`'s endpoint on `branch`, on the shaked at `self_base`.
    pub(crate) fn for_agent(self_base: &str, branch: &BranchCtx, agent: &str) -> Mcp {
        Mcp {
            server: server_name(branch.world.bench()),
            url: endpoint(self_base, "mcp", &branch.branch_id, agent),
            token: Some(branch.agent_token(agent)),
        }
    }
}

/// Where a CLI agent in a `shaked worker` sends its model calls: the worker's gateway for its
/// branch and agent (`…/gateway/<branch>/<agent>`), its agent token as the key. A sandbox holds
/// no model key: the control box adds the run's own.
pub struct ModelRoute {
    pub base: String,
    pub token: String,
}

impl ModelRoute {
    /// `agent`'s route on `branch`, on the shaked at `self_base`.
    pub(crate) fn for_agent(self_base: &str, branch: &BranchCtx, agent: &str) -> ModelRoute {
        ModelRoute {
            base: endpoint(self_base, "gateway", &branch.branch_id, agent),
            token: branch.agent_token(agent),
        }
    }
}

/// The name the CLIs give the world's MCP server: the benchmark's name, each character outside
/// `[A-Za-z0-9_-]` (what Claude Code and Codex accept) replaced by `_`; `shakedown` if it is empty.
pub fn server_name(benchmark: &str) -> String {
    if benchmark.is_empty() {
        return "shakedown".into();
    }
    benchmark
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect()
}

/// The secrets a CLI child doesn't get (except its own vendor's key): database credentials,
/// every model provider's key, and the local model servers.
const SECRET_ENV: [&str; 7] = [
    "SHAKEDOWN_PG_URL",
    "DATABASE_URL",
    "PGPASSWORD",
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    LOCAL_MODELS_ENV,
];

/// True for the name of an env var a CLI child must not see: the secrets above, and any database
/// URL, local model key or provider key under another prefix (a stale variable from an older
/// setup is removed too).
fn is_secret_env(name: &str) -> bool {
    name.starts_with("PG")
        || name.starts_with(LOCAL_KEY_PREFIX)
        || name.contains("LOCAL_KEY_")
        || name.ends_with("_PG_URL")
        || name.ends_with("_API_KEY")
        || SECRET_ENV.contains(&name)
}

/// The env vars to remove from a CLI child, of those named in `present`: every secret but
/// `keep`, the key of the CLI's own vendor (Claude Code signs in with `ANTHROPIC_API_KEY`,
/// Codex with `OPENAI_API_KEY`).
fn scrubbed(present: impl Iterator<Item = String>, keep: &str) -> Vec<String> {
    let mut out: Vec<String> = present.filter(|k| is_secret_env(k)).collect();
    out.extend(SECRET_ENV.iter().map(|k| k.to_string()));
    out.retain(|k| k != keep);
    out.sort();
    out.dedup();
    out
}

/// Removes database and API secrets from a CLI child's environment, all but `keep`.
pub(crate) fn scrub_env(c: &mut Command, keep: &str) {
    let present = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in scrubbed(present, keep) {
        c.env_remove(k);
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::{scrubbed, server_name, Mcp, ModelRoute};

    #[test]
    fn secrets_under_any_prefix_are_removed() {
        let present = [
            "OLD_LOCAL_KEY_OLLAMA",
            "OLD_PG_URL",
            "MISTRAL_API_KEY",
            "OLD_PORT",
        ]
        .map(String::from);
        let gone = scrubbed(present.into_iter(), "ANTHROPIC_API_KEY");
        for k in [
            "OLD_LOCAL_KEY_OLLAMA",
            "OLD_PG_URL",
            "MISTRAL_API_KEY",
            "SHAKEDOWN_PG_URL",
        ] {
            assert!(gone.iter().any(|g| g == k), "{k} kept");
        }
        assert!(!gone.iter().any(|g| g == "OLD_PORT"));
        assert!(
            !gone.iter().any(|g| g == "ANTHROPIC_API_KEY"),
            "the CLI's own key stays"
        );
    }

    #[test]
    fn each_cli_keeps_only_its_own_vendor_key() {
        let present = [
            "PATH",
            "HOME",
            "SHAKEDOWN_PORT",
            "SHAKEDOWN_LOCAL_KEY_OLLAMA",
            "PGUSER",
        ];
        for (keep, other) in [
            ("ANTHROPIC_API_KEY", "OPENAI_API_KEY"),
            ("OPENAI_API_KEY", "ANTHROPIC_API_KEY"),
        ] {
            let gone = scrubbed(present.iter().map(|k| k.to_string()), keep);
            for k in [
                other,
                "OPENROUTER_API_KEY",
                "SHAKEDOWN_LOCAL_KEY_OLLAMA",
                "SHAKEDOWN_LOCAL_MODELS",
                "PGUSER",
                "PGPASSWORD",
                "SHAKEDOWN_PG_URL",
                "DATABASE_URL",
            ] {
                assert!(gone.iter().any(|g| g == k), "{k} kept with {keep}");
            }
            for k in [keep, "PATH", "HOME", "SHAKEDOWN_PORT"] {
                assert!(!gone.iter().any(|g| g == k), "{k} removed with {keep}");
            }
        }
    }

    /// The env changes `c` makes: a var set (`Some`) or removed (`None`), the last change winning.
    fn env_changes(c: &Command) -> std::collections::BTreeMap<String, Option<String>> {
        let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
        c.get_envs().map(|(k, v)| (text(k), v.map(text))).collect()
    }

    /// `c` removes every secret (the fixed ones and those this process has) and then sets exactly
    /// `set`: no key from the environment survives into a routed CLI agent.
    fn only_set_after_the_scrub(c: &Command, set: &[(&str, &str)]) {
        let env = env_changes(c);
        let present = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
        let secrets = present.filter(|k| super::is_secret_env(k));
        for k in secrets.chain(super::SECRET_ENV.map(String::from)) {
            if !set.iter().any(|(s, _)| *s == k) {
                assert_eq!(env.get(&k), Some(&None), "{k} survives");
            }
        }
        let kept: Vec<(&str, Option<&str>)> = (env.iter())
            .filter(|(_, v)| v.is_some())
            .map(|(k, v)| (k.as_str(), v.as_deref()))
            .collect();
        let mut want: Vec<(&str, Option<&str>)> = set.iter().map(|(k, v)| (*k, Some(*v))).collect();
        want.sort();
        assert_eq!(kept, want);
    }

    /// Claude Code's and Codex's commands as a worker builds them: MCP token `sda_mcp`, routed to
    /// `base` with token `sda_route`.
    fn routed_commands(base: &str) -> (Command, Command) {
        use crate::agents::{claude, codex};
        use crate::model::{AgentKind, AgentSpec};
        let dir = super::fresh_workdir().unwrap();
        let spec = AgentSpec::new("a", AgentKind::ClaudeCode);
        let mcp = Mcp {
            server: "s".into(),
            url: "http://127.0.0.1:9/mcp/b/a".into(),
            token: Some("sda_mcp".into()),
        };
        let route = ModelRoute {
            base: base.into(),
            token: "sda_route".into(),
        };
        let mut c = claude::command_in("claude", &spec, &mcp, "go", &dir).unwrap();
        claude::route_models(&mut c, &route);
        let mut x = codex::command_with("codex", &spec, &mcp, &dir, "go");
        codex::route_models(&mut x, &route);
        std::fs::remove_dir_all(&dir).unwrap();
        (c, x)
    }

    /// In a worker, Claude Code and Codex keep no secret of the parent's environment and get only
    /// the gateway route, their agent token as every vendor key. The order is kept by
    /// construction (`route_models` runs after `command_*`, which scrubs); the vendor key the
    /// route sets is the scrub's `keep`.
    #[test]
    fn a_routed_cli_keeps_no_parent_secret_and_gets_only_the_route() {
        let base = "http://127.0.0.1:7/gateway/b/a";
        let (c, x) = routed_commands(base);
        let claude_set = [
            ("ANTHROPIC_BASE_URL", base),
            ("ANTHROPIC_API_KEY", "sda_route"),
        ];
        only_set_after_the_scrub(&c, &claude_set);
        let v1 = format!("{base}/v1");
        let codex_set = [
            ("OPENAI_BASE_URL", v1.as_str()),
            ("OPENAI_API_KEY", "sda_route"),
            ("CODEX_API_KEY", "sda_route"),
            (crate::agents::codex::AGENT_TOKEN_ENV, "sda_mcp"),
        ];
        only_set_after_the_scrub(&x, &codex_set);
    }

    fn text_args(c: &Command) -> Vec<String> {
        (c.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// The variables `docker` passes into the container: each `-e NAME`.
    fn passed_in(c: &Command) -> Vec<String> {
        let args = text_args(c);
        let named = args.windows(2).filter(|w| w[0] == "-e");
        let mut names: Vec<String> = named.map(|w| w[1].clone()).collect();
        names.sort();
        names
    }

    /// `run` is `inner` (`program`) in a container of image `t` as uid 1000 on network `net`, in
    /// `/w`, passed by name exactly the variables `inner` sets, with no token in argv, and with
    /// `inner`'s own environment (the scrub and the route) for `docker`.
    fn contained(run: &Command, inner: &Command, program: &str) {
        assert_eq!(run.get_program(), "docker");
        let args = text_args(run);
        let head = "run --rm -i --name c --user 1000:1000 --network net --cap-drop ALL \
                    --security-opt no-new-privileges --memory 1g --pids-limit 256 --read-only \
                    --tmpfs /tmp --tmpfs /home/agent:uid=1000,gid=1000,mode=0700 \
                    --mount type=bind,src=/w,dst=/w -w /w";
        let head: Vec<&str> = head.split_whitespace().collect();
        assert_eq!(args[..head.len()], head[..]);
        let at = args.iter().position(|a| a == "t").expect("the image");
        assert_eq!(args[at + 1], program);
        assert_eq!(args[at + 2..], text_args(inner)[..]);
        assert!(!args.iter().any(|a| a.contains("sda_")), "a token in argv");
        let set = env_changes(inner).into_iter().filter(|(_, v)| v.is_some());
        assert_eq!(passed_in(run), set.map(|(k, _)| k).collect::<Vec<_>>());
        assert_eq!(env_changes(run), env_changes(inner));
    }

    /// In a sandbox's worker, Claude Code and Codex run as uid 1000 in a throwaway container on
    /// the agent network, in their work folder. The container gets by name exactly the variables
    /// the command sets, and `docker` keeps no parent secret (see
    /// `a_routed_cli_keeps_no_parent_secret_and_gets_only_the_route`): the only key in its
    /// environment is the agent token.
    #[test]
    fn in_a_worker_cli_agents_run_unprivileged_on_the_agent_network() {
        let jail = super::super::jail::Jail {
            network: "net".into(),
            image: "t".into(),
            listen: "10.250.0.1:4610".parse().unwrap(),
        };
        let (c, x) = routed_commands(&format!("{}/gateway/b/a", jail.base()));
        let dir = std::path::Path::new("/w");
        contained(&jail.docker_run(&c, dir, "c"), &c, "claude");
        contained(&jail.docker_run(&x, dir, "c"), &x, "codex");
    }

    #[test]
    fn the_server_is_named_after_the_benchmark() {
        assert_eq!(server_name("retail"), "retail");
        assert_eq!(server_name("toy-bench_2"), "toy-bench_2");
        assert_eq!(server_name("My bench.v2/é"), "My_bench_v2__");
        assert_eq!(server_name(""), "shakedown");
    }
}
