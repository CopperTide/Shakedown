//! What a CLI agent (Claude Code, Codex) runs with: the world's MCP endpoint, the gateway route of
//! its model calls in a `shaked worker`, only a small allowlisted part of shaked's environment,
//! and the process run with a time limit in a fresh work folder.

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
use crate::{exec::BranchCtx, flow::FlowControl};

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

/// All of shaked's environment a CLI child gets besides its own vendor's key: what a CLI needs to
/// run (paths, the user, locale, terminal, time zone, temp and XDG folders, extra CA
/// certificates). An entry ending in `*` names every variable with that prefix. Nothing else gets
/// through: no key, token, password or database URL, and no proxy setting (one can carry
/// credentials).
const CLI_ENV: [&str; 14] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_*",
    "TERM",
    "TZ",
    "TMPDIR",
    "XDG_*",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
];

/// True when a CLI child gets shaked's variable `name`: [`CLI_ENV`] names it, or it is `keep`,
/// the key of the CLI's own vendor (Claude Code signs in with `ANTHROPIC_API_KEY`, Codex with
/// `OPENAI_API_KEY`).
fn passed(name: &str, keep: &str) -> bool {
    let named = |entry: &&str| match entry.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *entry,
    };
    name == keep || CLI_ENV.iter().any(named)
}

/// Removes from a CLI child's environment every variable of shaked's that [`passed`] doesn't let
/// through, by name: the child inherits only those, and a jail's container (which gets only what
/// the command sets) none of them. What the caller sets afterwards (the gateway route, the
/// agent's token) comes on top.
pub(crate) fn scrub_env(c: &mut Command, keep: &str) {
    for (k, _) in std::env::vars_os() {
        if !k.to_str().is_some_and(|k| passed(k, keep)) {
            c.env_remove(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, process::Command};

    use super::{passed, server_name, Mcp, ModelRoute};

    /// Every name the base lets through (two of each prefix), as a parent shaked may have them.
    const PARENT_BASE: [&str; 16] = [
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TERM",
        "TZ",
        "TMPDIR",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_EXTRA_CA_CERTS",
    ];

    /// Names a parent shaked may hold that no CLI child may see: the vault's key, endpoint
    /// credentials, tokens, secrets and passwords of any service, cloud and database credentials,
    /// both vendors' keys, proxies (one can carry credentials), and `FOO_BAR_SECRET` for any name
    /// no list thought of.
    const PARENT_SECRETS: [&str; 27] = [
        "SHAKEDOWN_VAULT_KEY",
        "SHAKEDOWN_VAULT_KEY_FILE",
        "SHAKEDOWN_AGENT_TOKEN",
        "SHAKEDOWN_AGENT_AUTH",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "HF_TOKEN",
        "NPM_TOKEN",
        "CLOUDFLARE_API_TOKEN",
        "MY_SERVICE_TOKEN",
        "MY_SERVICE_SECRET",
        "MY_SERVICE_PASSWORD",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_BEARER_TOKEN_BEDROCK",
        "SHAKEDOWN_OBJECT_STORE_R2_SECRET_ACCESS_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "SHAKEDOWN_PG_URL",
        "DATABASE_URL",
        "PGPASSWORD",
        "HTTPS_PROXY",
        "https_proxy",
        "FOO_BAR_SECRET",
    ];

    #[test]
    fn only_the_base_and_the_clis_own_vendor_key_pass() {
        for (keep, other) in [
            ("ANTHROPIC_API_KEY", "OPENAI_API_KEY"),
            ("OPENAI_API_KEY", "ANTHROPIC_API_KEY"),
        ] {
            assert!(passed(keep, keep), "{keep}");
            assert!(!passed(other, keep), "{other} with {keep}");
            for k in PARENT_BASE {
                assert!(passed(k, keep), "{k} with {keep}");
            }
            for k in PARENT_SECRETS.iter().filter(|k| **k != keep) {
                assert!(!passed(k, keep), "{k} with {keep}");
            }
        }
    }

    /// Claude Code's and Codex's commands as a worker builds them, on the programs `claude_bin`
    /// and `codex_bin`: MCP token `sda_mcp`, routed to `base` with token `sda_route`.
    fn routed_commands((claude_bin, codex_bin): (&str, &str), base: &str) -> (Command, Command) {
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
        let mut c = claude::command_in(claude_bin, &spec, &mcp, "go", &dir).unwrap();
        claude::route_models(&mut c, &route);
        let mut x = codex::command_with(codex_bin, &spec, &mcp, &dir, "go");
        codex::route_models(&mut x, &route);
        std::fs::remove_dir_all(&dir).unwrap();
        (c, x)
    }

    /// The environment `c`'s process started with: it runs `c` (a stand-in that says it is ready,
    /// then waits on its stdin, so it stays the process shaked started), reads its
    /// `/proc/<pid>/environ` once it is ready (before, the new program's environment may not be
    /// set up yet) and kills it.
    fn started_env(mut c: Command) -> BTreeMap<String, String> {
        use std::{io::BufRead, process::Stdio};
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = c.spawn().unwrap();
        let out = std::io::BufReader::new(child.stdout.take().unwrap());
        out.lines().next().unwrap().unwrap();
        let raw = std::fs::read(format!("/proc/{}/environ", child.id())).unwrap();
        let _ = child.kill();
        let _ = child.wait();
        let pairs = raw.split(|b| *b == 0).filter(|p| !p.is_empty());
        let text = |p: &[u8]| String::from_utf8_lossy(p).into_owned();
        let split = |p: &[u8]| text(p).split_once('=').map(|(k, v)| (k.into(), v.into()));
        pairs.filter_map(split).collect()
    }

    /// A program that says `ready`, waits for a line on its stdin and does nothing else, in a fresh
    /// folder.
    fn waiting_program() -> (tempfile::TempDir, String) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli");
        std::fs::write(&path, "#!/bin/sh\necho ready\nread line\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = path.to_string_lossy().into_owned();
        (dir, path)
    }

    /// Set only in the child test process [`a_cli_child_gets_only_the_base_and_its_route`] starts.
    const PROBE: &str = "SHAKEDOWN_TEST_SCRUB_PROBE";

    /// In a test process whose environment is exactly [`PARENT_BASE`] and [`PARENT_SECRETS`],
    /// Claude Code's and Codex's routed children start with exactly the base and their route (run
    /// by the test below; without it, nothing to check).
    #[test]
    #[ignore = "run by a_cli_child_gets_only_the_base_and_its_route, in a set environment"]
    fn probe_a_cli_childs_environment() {
        if std::env::var_os(PROBE).is_none() {
            return;
        }
        let (_dir, program) = waiting_program();
        let base = "http://127.0.0.1:7/gateway/b/a";
        let v1 = format!("{base}/v1");
        let (c, x) = routed_commands((&program, &program), base);
        let claude = [
            ("ANTHROPIC_BASE_URL", base),
            ("ANTHROPIC_API_KEY", "sda_route"),
        ];
        let codex = [
            ("OPENAI_BASE_URL", v1.as_str()),
            ("OPENAI_API_KEY", "sda_route"),
            ("CODEX_API_KEY", "sda_route"),
            (crate::agents::codex::AGENT_TOKEN_ENV, "sda_mcp"),
        ];
        for (cmd, route) in [(c, &claude[..]), (x, &codex[..])] {
            let env = started_env(cmd);
            let names = route.iter().map(|r| r.0);
            let mut want: Vec<&str> = PARENT_BASE.into_iter().chain(names).collect();
            want.sort();
            assert_eq!(env.keys().map(String::as_str).collect::<Vec<_>>(), want);
            for (k, v) in route {
                assert_eq!(env[*k], *v, "{k}");
            }
        }
    }

    /// A CLI child gets exactly the base of the parent's environment and its gateway route,
    /// whatever else the parent holds. Checked in a child test process whose environment is set
    /// whole, so this one's (which other tests read) stays as it is.
    #[test]
    fn a_cli_child_gets_only_the_base_and_its_route() {
        let name = "agents::cli::tests::probe_a_cli_childs_environment";
        let mut probe = Command::new(std::env::current_exe().unwrap());
        probe.args(["--exact", name, "--ignored", "--test-threads=1"]);
        probe.env_clear();
        for k in PARENT_BASE {
            let value = std::env::var(k).unwrap_or_else(|_| "from-the-parent".into());
            probe.env(k, value);
        }
        probe.env("TMPDIR", std::env::temp_dir());
        for k in PARENT_SECRETS.into_iter().chain([PROBE]) {
            probe.env(k, "from-the-parent");
        }
        let out = probe.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && text.contains("1 passed"), "{text}");
    }

    /// The env changes `c` makes: a var set (`Some`) or removed (`None`), the last change winning.
    fn env_changes(c: &Command) -> BTreeMap<String, Option<String>> {
        let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
        c.get_envs().map(|(k, v)| (text(k), v.map(text))).collect()
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
    /// the command sets (its route and agent token, none of shaked's own), and `docker` the same
    /// environment as the command (see `a_cli_child_gets_only_the_base_and_its_route`).
    #[test]
    fn in_a_worker_cli_agents_run_unprivileged_on_the_agent_network() {
        let jail = super::super::jail::Jail {
            network: "net".into(),
            image: "t".into(),
            listen: "10.250.0.1:4610".parse().unwrap(),
        };
        let base = format!("{}/gateway/b/a", jail.base());
        let (c, x) = routed_commands(("claude", "codex"), &base);
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
