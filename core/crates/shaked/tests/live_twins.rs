//! Real clients against a real twin through shaked's recording proxy (live: needs `gh` and `git` on
//! PATH and the github-twin package's environment, `uv sync` in `integrations/github-twin`). The
//! world is one GitHub twin part seeded with `tests/fixtures/clients/app`; the run is one an
//! external system drives, and each agent's clients get the environment its registration gives:
//! the proxy with its ticket, the run's CA and the twin's variables. HTTPS goes to the twin's host
//! on 443 by `CONNECT`, as on GitHub. No model is called.
mod support;
use support::records;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};
use shaked::model::*;
use support::{packages, run, AppFx};

const HOST: &str = "github.acme.test";
const REPO: &str = "acme/app";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clients")
}

/// The GitHub twin's config: one organisation `acme` with the repository `app`.
fn twin_config() -> Value {
    json!({
        "hosts": [{"host": HOST}], "org": "acme", "token": "acme-token", "default_user": "acme-admin",
        "repos": [{"name": "app", "source": fixtures().join("app"), "default_branch": "main"}],
        "actions": {"enabled": false}
    })
}

/// shaked over a toy benchmark whose world is the twin, and a live external run of it.
struct TwinFx {
    fx: AppFx,
    branch: String,
    home: Home,
}

/// The agents' homes and clones, removed on drop.
struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One agent's clients: its own home, gh config and the environment shaked gave it.
struct Agent {
    cwd: PathBuf,
    env: Vec<(String, String)>,
}

impl TwinFx {
    async fn new(tag: &str) -> TwinFx {
        let config = format!("github={}", twin_config());
        let bench = ["bench", "--part", "github=github-twin", "--config", &config];
        let root = packages::package_root(tag, &[packages::toy("toybench", "benchmark", &bench)]);
        let fx = AppFx::idle("twins", tag)
            .root(root)
            .bench("toybench")
            .start()
            .await;
        let team = json!({"id": "own", "name": "Own design", "agents": []});
        let body = json!({"task_id": "t1", "team": team, "runner": "external"});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 200, "{v}");
        let home = std::env::temp_dir().join(format!("shaked-twins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let branch = v["branch_id"].as_str().unwrap().to_string();
        let home = Home(home);
        TwinFx { fx, branch, home }
    }

    /// Registers `id` (read-only when asked) and makes its home.
    async fn agent(&self, id: &str, read_only: bool) -> Agent {
        let path = format!("/api/branches/{}/agents", self.branch);
        let body = json!({"id": id, "name": id, "read_only": read_only});
        let (status, a) = self.fx.post(&path, body).await;
        assert_eq!(status, 200, "{a}");
        let cwd = self.home.0.join(id);
        std::fs::create_dir_all(&cwd).unwrap();
        let home = cwd.to_string_lossy().into_owned();
        let mut env = vec![
            ("PATH".into(), std::env::var("PATH").unwrap()),
            ("HOME".into(), home.clone()),
            ("GH_CONFIG_DIR".into(), format!("{home}/gh")),
            ("XDG_CACHE_HOME".into(), format!("{home}/cache")),
        ];
        for k in ["GIT_AUTHOR", "GIT_COMMITTER"] {
            env.push((format!("{k}_NAME"), id.into()));
            env.push((format!("{k}_EMAIL"), format!("{id}@example.test")));
        }
        let given = a["env"].as_object().expect("an env: the world serves HTTP");
        env.extend(
            given
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())),
        );
        Agent { cwd, env }
    }

    async fn steps(&self) -> Vec<Event> {
        let evs = records(&self.fx.state.store).events(&self.branch).await;
        let calls = evs
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EventKind::ToolCall);
        calls.filter(|e| e.payload["part"] == "github").collect()
    }
}

impl Agent {
    /// `program args` in `dir` (the agent's home when None), off the runtime's threads.
    async fn run(&self, dir: Option<&str>, program: &str, args: &[&str]) -> Output {
        let cwd = dir.map_or(self.cwd.clone(), |d| self.cwd.join(d));
        let mut c = Command::new(program);
        c.args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(self.env.clone());
        tokio::task::spawn_blocking(move || c.output().unwrap())
            .await
            .unwrap()
    }

    /// `run`, failing the test unless it exits 0: its stdout.
    async fn ok(&self, dir: Option<&str>, program: &str, args: &[&str]) -> String {
        let out = self.run(dir, program, args).await;
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        assert!(
            out.status.success(),
            "{program} {args:?}: {}{}",
            text(&out.stdout),
            text(&out.stderr)
        );
        text(&out.stdout)
    }

    /// `gh <args> -R <host>/acme/app`.
    async fn gh(&self, args: &[&str]) -> Output {
        let repo = format!("{HOST}/{REPO}");
        let all: Vec<&str> = args.iter().copied().chain(["-R", repo.as_str()]).collect();
        self.run(None, "gh", &all).await
    }

    async fn gh_ok(&self, args: &[&str]) -> String {
        let repo = format!("{HOST}/{REPO}");
        let all: Vec<&str> = args.iter().copied().chain(["-R", repo.as_str()]).collect();
        self.ok(None, "gh", &all).await
    }

    /// Clones the repository, commits `file` on `branch` and pushes it.
    async fn push_branch(&self, branch: &str, file: &str) {
        let url = format!("https://{HOST}/{REPO}.git");
        self.ok(None, "git", &["clone", "-q", &url, "app"]).await;
        std::fs::write(self.cwd.join("app").join(file), "a line\n").unwrap();
        for args in [
            vec!["switch", "-q", "-c", branch],
            vec!["add", file],
            vec!["commit", "-q", "-m", "Add a line"],
            vec!["push", "-q", "-u", "origin", branch],
        ] {
            self.ok(Some("app"), "git", &args).await;
        }
    }
}

/// The coder's pull request #1 of branch `f`, pushed over git and opened with gh.
async fn open_pr(coder: &Agent) {
    coder.push_branch("f", "LINE.md").await;
    let create = ["pr", "create", "-t", "T", "-b", "B", "--head", "f"];
    coder.gh_ok(&create).await;
}

/// The steps `agent` took: the URL and the answer's status of each.
fn steps_of(steps: &[Event], agent: &str) -> Vec<(String, i64)> {
    let mine = steps.iter().filter(|s| s.agent.as_deref() == Some(agent));
    let url = |s: &Event| s.payload["args"]["url"].as_str().unwrap_or("").to_string();
    let status = |s: &Event| s.payload["result"]["status"].as_i64().unwrap_or(0);
    mine.map(|s| (url(s), status(s))).collect()
}

#[test]
#[ignore = "live: needs gh, git and the github-twin package's environment"]
fn live_gh_drives_a_pull_request_through_the_proxy() {
    run(async {
        let fx = TwinFx::new("ghflow").await;
        let (coder, reviewer) = (
            fx.agent("coder", false).await,
            fx.agent("reviewer", false).await,
        );
        let version = coder.ok(None, "gh", &["--version"]).await;
        println!("{}", version.lines().next().unwrap_or(""));
        coder
            .ok(None, "gh", &["auth", "status", "--hostname", HOST])
            .await;
        open_pr(&coder).await;
        assert!(coder.gh_ok(&["pr", "list"]).await.contains("\tT\tf\tOPEN"));
        reviewer
            .gh_ok(&["pr", "review", "1", "--approve", "-b", "Good"])
            .await;
        coder
            .gh_ok(&["pr", "merge", "1", "--squash", "--delete-branch"])
            .await;
        let view = coder.gh_ok(&["pr", "view", "1", "--json", "state"]).await;
        assert!(view.contains("\"MERGED\""), "{view}");
        let steps = fx.steps().await;
        assert!(steps.len() > 5 && steps.iter().all(|s| s.agent.is_some()));
        let on_443 = format!("https://{HOST}/");
        assert!(steps_of(&steps, "coder")
            .iter()
            .all(|(u, _)| u.starts_with(&on_443)));
        assert!(!steps_of(&steps, "reviewer").is_empty());
    })
}

#[test]
#[ignore = "live: needs gh, git and the github-twin package's environment"]
fn live_a_read_only_agents_gh_reads_but_its_merge_is_refused() {
    run(async {
        let fx = TwinFx::new("ghro").await;
        let (coder, auditor) = (
            fx.agent("coder", false).await,
            fx.agent("auditor", true).await,
        );
        open_pr(&coder).await;
        assert!(auditor
            .gh_ok(&["pr", "list"])
            .await
            .contains("\tT\tf\tOPEN"));
        let refused = auditor.gh(&["pr", "merge", "1", "--merge"]).await;
        assert!(!refused.status.success());
        let view = coder.gh_ok(&["pr", "view", "1", "--json", "state"]).await;
        assert!(view.contains("\"OPEN\""), "{view}");
        let audited = steps_of(&fx.steps().await, "auditor");
        assert!(
            audited.iter().any(|(_, status)| *status == 200),
            "{audited:?}"
        );
        let branch = fx.fx.get(&format!("/api/branches/{}", fx.branch)).await.1;
        assert_ne!(
            branch["status"], "failed",
            "a refused write is no violation: {}",
            branch["error"]
        );
    })
}

#[test]
#[ignore = "live: needs the github-twin package's environment (PyGithub is its dev dependency)"]
fn live_pygithub_works_unchanged_through_the_proxy() {
    run(async {
        let fx = TwinFx::new("pygithub").await;
        let coder = fx.agent("coder", false).await;
        let reviewer = fx.agent("reviewer", false).await;
        let python = packages::repo().join("integrations/github-twin/.venv/bin/python");
        let script = fixtures().join("pygithub_flow.py");
        let (python, script) = (python.to_str().unwrap(), script.to_str().unwrap());
        let opened = coder.ok(None, python, &[script, "open", REPO]).await;
        assert!(opened.contains("opened 1"), "{opened}");
        reviewer
            .ok(None, python, &[script, "approve", REPO, "1"])
            .await;
        let merged = coder.ok(None, python, &[script, "merge", REPO, "1"]).await;
        assert!(merged.contains("merged True"), "{merged}");
        let steps = fx.steps().await;
        assert!(steps.iter().any(|s| s.agent.as_deref() == Some("reviewer")));
        assert!(steps.iter().all(|s| s.agent.is_some()));
    })
}
