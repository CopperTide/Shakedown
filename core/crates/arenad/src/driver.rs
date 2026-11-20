//! A Code connection's driver, launched by arenad for one run: the team's own program, which
//! joins the run through the external-runner routes (`arena.driver` in the Python SDK does it in
//! a few lines). arenad starts it with the run's ids in its environment and watches it: if it
//! exits while its run is still live, the run is scored and ends `failed` with the exit code and
//! the tail of its stderr; if it is still running a grace period after its run ended, it is
//! killed. Its exit is recorded as a `driver` world event.
//!
//! arenad binds 127.0.0.1 only (`main.rs`), and the routes that launch a driver refuse requests a
//! web page could make (`guard_launch`), so only programs on this machine can start one. Any
//! local program can, though: a Code connection runs as arenad's user, so this assumes a machine
//! whose accounts trust each other (`docs/agents.md`).

use std::{
    collections::{BTreeMap, VecDeque},
    os::unix::process::ExitStatusExt,
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde_json::json;
use tokio::{io::AsyncReadExt, process::Child};

use crate::{
    app::AppState,
    model::*,
    proxy::env::env_for,
    store::Connection,
    suite::{
        self,
        slots::{self, SlotOutcome},
        Slot, Suite,
    },
};

/// How much of the driver's stdout and stderr is kept, from the end.
pub const TAIL_BYTES: usize = 8 * 1024;
/// How much of its stderr the run's error quotes, from the end.
const ERROR_TAIL_CHARS: usize = 2_000;
/// How often the watcher looks whether the run is still live.
const POLL: Duration = Duration::from_millis(250);

/// The environment a driver inherits from arenad; everything else (provider keys included) is
/// cleared. The connection's own `env` and the run's `ARENA_*` ids come on top.
const PASSED_ENV: [&str; 9] = [
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "UV_CACHE_DIR",
    "XDG_CACHE_HOME",
    "RUST_LOG",
];

/// The last [`TAIL_BYTES`] a stream wrote.
#[derive(Default)]
struct Tail(VecDeque<u8>);

impl Tail {
    fn push(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
        let over = self.0.len().saturating_sub(TAIL_BYTES);
        self.0.drain(..over);
    }

    fn text(&self) -> String {
        let (a, b) = self.0.as_slices();
        String::from_utf8_lossy(&[a, b].concat()).into_owned()
    }
}

/// A driver arenad started: what it has written so far.
#[derive(Clone)]
pub struct Launched {
    stdout: Arc<Mutex<Tail>>,
    stderr: Arc<Mutex<Tail>>,
    /// Set once the driver exited: how (`exit N`, `killed by signal N`).
    exit: Arc<Mutex<Option<String>>>,
}

impl Launched {
    pub fn stdout_tail(&self) -> String {
        self.stdout.lock().unwrap().text()
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr.lock().unwrap().text()
    }

    /// How the driver exited, once it has.
    pub fn exit(&self) -> Option<String> {
        self.exit.lock().unwrap().clone()
    }
}

/// The run a driver is launched for.
pub struct RunIds<'a> {
    pub run_id: &'a str,
    pub branch_id: &'a str,
    pub task_id: &'a str,
    /// The branch this one was forked from, on a fork.
    pub fork_of: Option<&'a str>,
}

/// The command that starts `conn`'s driver for the run `ids`: in its own process group, with
/// only [`PASSED_ENV`] of arenad's environment, then `world_env` (the proxy and hosts of a world
/// that serves HTTP), the connection's env and the run's ids.
fn command_for(
    base: &str,
    conn: &Connection,
    ids: &RunIds<'_>,
    world_env: &BTreeMap<String, String>,
) -> Result<tokio::process::Command> {
    let (program, args) = (conn.command.split_first()).context("the connection has no command")?;
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).env_clear().stdin(Stdio::null());
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.process_group(0).kill_on_drop(true);
    for k in PASSED_ENV {
        if let Ok(v) = std::env::var(k) {
            cmd.env(k, v);
        }
    }
    cmd.envs(world_env)
        .envs(&conn.env)
        .env("ARENA_URL", base)
        .env("ARENA_RUN", ids.run_id)
        .env("ARENA_BRANCH", ids.branch_id)
        .env("ARENA_TASK", ids.task_id)
        .env("ARENA_CONNECTION", &conn.id);
    if let Some(parent) = ids.fork_of {
        cmd.env("ARENA_FORK_OF", parent);
    }
    if let Some(dir) = &conn.workdir {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

/// `ARENA_WORLD_EPOCH_MS` (the run's world clock 0, unix ms) and `ARENA_CLOCK_SPEED` (world ms
/// per wall ms) for a driver of `branch`.
async fn time_env(
    state: &AppState,
    branch: &str,
    ctx: Option<&crate::exec::BranchCtx>,
) -> BTreeMap<String, String> {
    let events = state.store.events(branch).await.unwrap_or_default();
    let epoch = crate::recorded::world_epoch_ms(&events);
    let speed = ctx.map_or_else(crate::clock::speed_from_env, |c| c.clock.speed());
    let pairs = [
        ("ARENA_WORLD_EPOCH_MS", epoch.to_string()),
        ("ARENA_CLOCK_SPEED", speed.to_string()),
    ];
    let mut env: BTreeMap<String, String> = pairs.map(|(k, v)| (k.to_string(), v)).into();
    if let Some(m) = ctx.and_then(|c| c.rules.world_minutes) {
        env.insert("ARENA_WORLD_MINUTES".into(), m.to_string());
    }
    env
}

/// Starts `conn`'s command for the run `ids` and watches it (see the module's notes).
pub async fn launch(state: &Arc<AppState>, conn: &Connection, ids: RunIds<'_>) -> Result<Launched> {
    let ctx = state.live.get(ids.branch_id);
    let mut world_env = ctx
        .as_ref()
        .map(|c| env_for(state, c, None))
        .unwrap_or_default();
    world_env.extend(time_env(state, ids.branch_id, ctx.as_deref()).await);
    let mut cmd = command_for(&state.config.self_base, conn, &ids, &world_env)?;
    let what = || format!("starting the driver `{}`", conn.command.join(" "));
    let mut child = cmd.spawn().with_context(what)?;
    let launched = Launched {
        stdout: Arc::default(),
        stderr: Arc::default(),
        exit: Arc::default(),
    };
    let readers = [
        read_into(child.stdout.take(), launched.stdout.clone()),
        read_into(child.stderr.take(), launched.stderr.clone()),
    ];
    let (state, branch, watched) = (state.clone(), ids.branch_id.to_string(), launched.clone());
    tokio::spawn(async move {
        let status = watch(&state, &branch, &mut child).await;
        for r in readers {
            let _ = tokio::time::timeout(Duration::from_secs(2), r).await;
        }
        exited(&state, &branch, status, &watched).await;
    });
    Ok(launched)
}

/// Copies a stream into `tail` until it closes.
fn read_into<R>(stream: Option<R>, tail: Arc<Mutex<Tail>>) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(mut s) = stream else { return };
        let mut buf = [0u8; 4096];
        while let Ok(n) = s.read(&mut buf).await {
            if n == 0 {
                break;
            }
            tail.lock().unwrap().push(&buf[..n]);
        }
    })
}

/// Ends the run `failed` (scored) when its driver has driven it for longer than the limit: the
/// configured one, or its world time window's wall time and the grace when that is longer. A run
/// whose finish is held waits for its window instead.
async fn cut_off(state: &AppState, branch: &str, since: Instant) {
    let Some(ctx) = state.live.get(branch) else {
        return;
    };
    let (configured, grace) = (state.config.code_run_limit, state.config.driver_grace);
    let limit = crate::window::driver_limit(&ctx, configured, grace);
    if since.elapsed() < limit || crate::window::is_held(&ctx) {
        return;
    }
    let mins = (limit.as_secs_f64() / 60.0).max(0.1);
    let why =
        format!("the driver was still running after {mins:.1} min, so Shakedown ended the run");
    state.live.fail_if_live(state, branch, why).await;
}

/// Waits for the driver to exit; ends its run when the driver drove it past the limit
/// (`cut_off`), and kills the driver (and whatever it started) once its run has been over for
/// the grace period. `None` when its status couldn't be read.
async fn watch(state: &AppState, branch: &str, child: &mut Child) -> Option<ExitStatus> {
    let (grace, started) = (state.config.driver_grace, Instant::now());
    let mut over_since: Option<Instant> = None;
    loop {
        tokio::select! {
            status = child.wait() => return status.ok(),
            _ = tokio::time::sleep(POLL) => {
                if state.live.contains(branch) {
                    cut_off(state, branch, started).await;
                    continue;
                }
                if over_since.get_or_insert_with(Instant::now).elapsed() >= grace {
                    kill_group(child.id());
                    let _ = child.kill().await;
                    return child.wait().await.ok();
                }
            }
        }
    }
}

/// Kills the driver's process group: the driver leads its own, so this takes what it started.
fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid.filter(|p| *p > 0) {
        unsafe { libc::killpg(pid as i32, libc::SIGKILL) };
    }
}

/// How a driver exited, in a few words.
fn exit_words(status: Option<ExitStatus>) -> String {
    match status {
        Some(s) => match (s.code(), s.signal()) {
            (Some(c), _) => format!("exit {c}"),
            (None, Some(sig)) => format!("killed by signal {sig}"),
            (None, None) => "exit".into(),
        },
        None => "exit (status unknown)".into(),
    }
}

/// Records the driver's exit, and ends its run `failed` (scored) when it exited before finishing.
async fn exited(state: &Arc<AppState>, branch: &str, status: Option<ExitStatus>, l: &Launched) {
    let words = exit_words(status);
    *l.exit.lock().unwrap() = Some(words.clone());
    let stderr = record_exit(state, branch, &words, l).await;
    let how = match status.and_then(|s| s.code()) {
        Some(c) => format!("exited with code {c}"),
        None => words.replace("exit", "exited"),
    };
    let held = state
        .live
        .get(branch)
        .is_some_and(|c| crate::window::is_held(&c));
    if held {
        return; // it finished before it exited: the window ends the run
    }
    let tail = crate::text::last_chars(stderr.trim(), ERROR_TAIL_CHARS);
    let why = match tail.is_empty() {
        true => format!("the driver {how} before finishing"),
        false => format!("the driver {how} before finishing: {tail}"),
    };
    state.live.fail_if_live(state, branch, why).await;
}

/// Records the driver's exit (`driver/exit`, with the tails of its output): its stderr tail.
async fn record_exit(state: &AppState, branch: &str, words: &str, l: &Launched) -> String {
    let stderr = l.stderr_tail();
    let ev = json!({
        "part": "driver",
        "kind": "exit",
        "text": words,
        "data": {"stderr_tail": stderr, "stdout_tail": l.stdout_tail()},
    });
    let kind = EventKind::WorldEvent;
    let recorded = match state.live.get(branch) {
        Some(ctx) => ctx.append_now(&state.store, None, kind, ev).await,
        None => state.store.append(branch, None, kind, ev).await,
    };
    if let Err(e) = recorded {
        eprintln!("branch {branch}: recording the driver's exit: {e:#}");
    }
    stderr
}

/// The rules of a run a Code connection drives.
pub fn rules_for(conn_id: &str, max_cost_usd: Option<f64>, world_minutes: Option<f64>) -> Rules {
    Rules::new(Runner::External)
        .with_max_cost(max_cost_usd)
        .with_world_minutes(world_minutes)
        .with_connection(Some(conn_id.to_string()))
}

/// Starts the external run of a Code setup's `slot` (waiting out a cleanup, as built-in runs
/// do), recorded on the slot: `None` when it couldn't start. Saves over the limit stop the
/// experiment, as they stop a built-in suite.
async fn start_run(
    state: &Arc<AppState>,
    suite: &Suite,
    slot: &Slot,
    conn_id: &str,
) -> Result<Option<crate::launch::Launched>> {
    let _permit = slots::permit(state, suite, slot).await?;
    let rules = rules_for(conn_id, suite.max_cost_usd, None);
    let start = crate::launch::launch(state, suite::slot_spec(suite, slot, rules));
    match slots::run_slot(state, suite, slot, start).await? {
        SlotOutcome::Filled(l) => Ok(Some(l)),
        SlotOutcome::Failed(_) => Ok(None),
    }
}

/// Starts the run of a suite's claimed `slot` whose setup is a Code connection, and launches
/// the driver for it once the slot names the run: the branch id. `None` (the slot marked failed)
/// when the connection is gone or the run couldn't start.
pub async fn start_slot(
    state: &Arc<AppState>,
    suite: &Suite,
    slot: &Slot,
) -> Result<Option<String>> {
    let id = suite
        .connection
        .as_deref()
        .context("the suite has no connection")?;
    let Some(conn) = state.store.connection(id).await? else {
        let why = format!("the Code connection {id} was deleted");
        state.store.slot_failed(&suite.id, slot.ord, &why).await?;
        return Ok(None);
    };
    let Some(started) = start_run(state, suite, slot, id).await? else {
        return Ok(None);
    };
    let ids = RunIds {
        run_id: &started.run_id,
        branch_id: &started.branch_id,
        task_id: &slot.task_id,
        fork_of: None,
    };
    launch_or_fail(state, &conn, ids).await;
    Ok(Some(started.branch_id))
}

/// Launches the driver; when it can't start, its run ends `failed` (scored) saying why. The
/// driver, or `None` when it couldn't start.
pub async fn launch_or_fail(
    state: &Arc<AppState>,
    conn: &Connection,
    ids: RunIds<'_>,
) -> Option<Launched> {
    let branch = ids.branch_id.to_string();
    match launch(state, conn, ids).await {
        Ok(l) => Some(l),
        Err(e) => {
            let why = format!("the driver could not start: {e:#}");
            state.live.fail_if_live(state, &branch, why).await;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tail_keeps_the_last_bytes() {
        let mut t = Tail::default();
        t.push(&vec![b'a'; TAIL_BYTES]);
        t.push(b"end");
        let text = t.text();
        assert_eq!(text.len(), TAIL_BYTES);
        assert!(text.ends_with("aend"));
    }
}
