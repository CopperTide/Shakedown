//! Branches run in sandboxes. A test control box with `Hosting` over the fake backend (each
//! sandbox a local `shaked worker`) runs the toy world with a scripted agent. `live_…` run on
//! real E2B with `E2B_API_KEY` set and are `#[ignore]`d:
//! `cargo test -p shaked --test hosting -- --ignored live_ --nocapture`. Each kills every
//! sandbox it made, also when it fails, and its sandboxes time out on their own anyway.
mod support;
use support::link::ws_url;
use support::scripted::{nothing, scripted_team, Between};
use support::{live, packages, poll_until, records, run, AppFx, STEP, WAIT};

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use serde_json::{json, Value};
use shaked::{
    hosting::{
        e2b::{E2bBackend, API},
        fake::FakeBackend,
        partial_of, Hosting, ProviderRef, ProviderState, SandboxBackend, SandboxSpec,
        SANDBOX_STOPPED, TICKET_PATH,
    },
    model::*,
};
use tokio::sync::Semaphore;

/// E2B's base template (no `shaked` in it: the live check stops at the ticket); the fake
/// ignores it.
const TEMPLATE: &str = "base";

/// A control box on the scored toy world (task `t1`, solved by one poke), its branches not in
/// sandboxes yet: the app and the toy packages' folder.
async fn control(tag: &str) -> (AppFx, PathBuf) {
    let root = packages::toy_world(&format!("host-{tag}"), &[], &["--scored"]);
    let toys = root.0.clone();
    let fx = AppFx::idle("hosting", tag).root(root).bench("toybench");
    (fx.start().await, toys)
}

/// Runs `fx`'s branches in sandboxes from `backend` linking to `link`, its `Hosting` `tweak`ed.
fn host(fx: &AppFx, backend: Arc<dyn SandboxBackend>, link: &str, tweak: Tweak) {
    let hosting = Hosting::new(backend, link, TEMPLATE).unwrap();
    let hosting = tweak(hosting.with_poll(Duration::from_millis(200)));
    assert!(fx.state.hosting.set(Arc::new(hosting)).is_ok());
}

type Tweak = fn(Hosting) -> Hosting;

/// A control box whose branches run in fake sandboxes, its `Hosting` `tweak`ed: the app and
/// the backend.
async fn hosted(tag: &str, tweak: Tweak) -> (AppFx, Arc<FakeBackend>) {
    let (fx, toys) = control(tag).await;
    let link = ws_url(&fx.base);
    let toys = toys.to_str().unwrap();
    let env = [("SHAKEDOWN_TEST_GUARD", "1"), ("SHAKEDOWN_PACKAGES", toys)];
    let fake = Arc::new(FakeBackend::new(env!("CARGO_BIN_EXE_shaked"), &link, &env));
    host(&fx, fake.clone(), &link, tweak);
    (fx, fake)
}

/// `POST /api/runs` of `t1` for `team`: the status and the answer.
async fn post_run(fx: &AppFx, team: &Team) -> (u16, Value) {
    let body = json!({"task_id": "t1", "team": team, "runner": "chain"});
    fx.post("/api/runs", body).await
}

/// A run of `t1` for `team` started: its branch.
async fn start(fx: &AppFx, team: &Team) -> String {
    let (status, v) = post_run(fx, team).await;
    assert_eq!(status, 200, "{v}");
    v["branch_id"].as_str().unwrap().to_string()
}

/// Waits until `fake` was asked for `n` kills, and no more.
async fn killed(fake: &FakeBackend, n: usize) {
    let done = poll_until(WAIT, STEP, async || (fake.killed() >= n).then_some(())).await;
    assert!(done.is_some(), "{} kills, not {n}", fake.killed());
    assert_eq!(fake.killed(), n);
}

#[test]
fn a_run_with_hosting_runs_in_a_fake_sandbox() {
    run(async {
        let (fx, fake) = hosted("runs", |h| h).await;
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let b = live::ended(&fx.state, &id).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        let evs = records(&fx.state.store).events(&id).await.unwrap();
        assert_eq!(evs.first().map(|e| e.kind), Some(EventKind::RunStart));
        killed(&fake, 1).await;
        assert_eq!(fake.created(), 1);
    });
}

#[test]
fn an_external_run_is_refused_on_hosting() {
    run(async {
        let (fx, fake) = hosted("external", |h| h).await;
        let team = scripted_team(nothing()).await;
        let body = json!({"task_id": "t1", "team": team, "runner": "external"});
        let (status, v) = fx.post("/api/runs", body).await;
        assert_eq!(status, 422, "{v}");
        let why = v.to_string();
        assert!(why.contains(shaked::launch::NOT_IN_A_SANDBOX), "{why}");
        assert_eq!(fake.created(), 0);
    });
}

/// The scripted agent waiting, between its calls, for a permit of `gate`.
fn gated(gate: &Arc<Semaphore>) -> Between {
    let gate = gate.clone();
    Arc::new(move || {
        let gate = gate.clone();
        Box::pin(async move { gate.acquire().await.unwrap().forget() })
    })
}

/// The labels `pairs`.
fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    let pairs = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string()));
    pairs.collect()
}

/// The sandbox `backend` runs `branch` in, once its worker has recorded something.
async fn linked(fx: &AppFx, backend: &dyn SandboxBackend, branch: &str) -> ProviderRef {
    let store = &fx.state.store;
    let recorded = async || {
        let evs = records(store).events(branch).await.unwrap();
        (!evs.is_empty()).then_some(())
    };
    let recorded = poll_until(WAIT, STEP, recorded).await;
    assert!(recorded.is_some(), "the worker recorded nothing");
    let found = backend.list(&labels(&[("branch", branch)])).await.unwrap();
    found.into_iter().next().expect("its sandbox").sb
}

#[test]
fn a_vanished_sandbox_ends_its_branch_infra_error() {
    run(async {
        let (fx, fake) = hosted("vanish", |h| h).await;
        let gate = Arc::new(Semaphore::new(0));
        let id = start(&fx, &scripted_team(gated(&gate)).await).await;
        fake.vanish(&linked(&fx, &*fake, &id).await);
        let b = live::ended(&fx.state, &id).await;
        let ended = (b.status, b.error.as_deref());
        assert_eq!(ended, (BranchStatus::InfraError, Some(SANDBOX_STOPPED)));
        let evs = records(&fx.state.store).events(&id).await.unwrap();
        assert_eq!(evs.first().map(|e| e.kind), Some(EventKind::RunStart));
        killed(&fake, 1).await;
        gate.add_permits(1);
    });
}

/// A sandbox from `fake` labelled `pairs`.
async fn labelled(fake: &FakeBackend, pairs: &[(&str, &str)]) -> ProviderRef {
    let spec = SandboxSpec {
        template: TEMPLATE.into(),
        allow_out: vec![],
        timeout: Duration::from_secs(60),
        metadata: labels(pairs),
    };
    fake.create(&spec).await.unwrap()
}

#[test]
fn the_sweep_kills_sandboxes_whose_branch_ended_or_that_outlived_their_timeout() {
    run(async {
        let (fx, fake) = hosted("sweep", |h| h).await;
        let gate = Arc::new(Semaphore::new(0));
        let id = start(&fx, &scripted_team(gated(&gate)).await).await;
        linked(&fx, &*fake, &id).await;
        let ours = ("shaked", "127.0.0.1");
        let orphan = labelled(&fake, &[ours, ("branch", "no-such-branch")]).await;
        let old = labelled(&fake, &[ours, ("branch", &id), ("started", "0")]).await;
        let theirs = labelled(&fake, &[("shaked", "elsewhere"), ("branch", "nope")]).await;
        shaked::hosting::sweep(&fx.state).await.unwrap();
        for (sb, was) in [(&orphan, false), (&old, false), (&theirs, true)] {
            let running = fake.state(sb).await.unwrap() == ProviderState::Running;
            assert_eq!(running, was, "{}", sb.0);
        }
        let b = live::ended(&fx.state, &id).await;
        assert_eq!(b.error.as_deref(), Some(SANDBOX_STOPPED));
        killed(&fake, 3).await;
        fake.kill(&theirs).await.unwrap();
        gate.add_permits(1);
    });
}

/// What the live check writes and reads back.
const TICKET: &[u8] = b"sdt_live_check\n";

fn live_spec(test: &str) -> SandboxSpec {
    SandboxSpec {
        template: "base".into(),
        allow_out: vec!["example.com".into()],
        timeout: Duration::from_secs(120),
        metadata: BTreeMap::from([("test".into(), test.into())]),
    }
}

/// The sandbox runs, takes a ticket at [`TICKET_PATH`] and gives it back.
async fn write_and_read(e2b: &E2bBackend, sb: &ProviderRef) -> anyhow::Result<()> {
    anyhow::ensure!(
        e2b.state(sb).await? == ProviderState::Running,
        "not running"
    );
    e2b.write_file(sb, TICKET_PATH, TICKET.to_vec()).await?;
    let back = e2b.read_file(sb, TICKET_PATH).await?;
    anyhow::ensure!(
        back == TICKET,
        "read back {:?}",
        String::from_utf8_lossy(&back)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "live: creates a real E2B sandbox (needs E2B_API_KEY)"]
async fn live_e2b_create_write_kill() {
    let Ok(key) = std::env::var("E2B_API_KEY") else {
        println!("skipped: E2B_API_KEY is not set");
        return;
    };
    let e2b = E2bBackend::new(key, API.parse().unwrap());
    let sb = e2b
        .create(&live_spec("live_e2b_create_write_kill"))
        .await
        .unwrap();
    println!("created E2B sandbox {}", sb.0);
    let used = write_and_read(&e2b, &sb).await;
    e2b.kill(&sb).await.unwrap();
    println!("killed E2B sandbox {}", sb.0);
    used.unwrap();
    let ticket = String::from_utf8_lossy(TICKET);
    println!("wrote and read back {ticket:?} at {TICKET_PATH}");
    assert_eq!(e2b.state(&sb).await.unwrap(), ProviderState::Gone);
}

/// A link host no worker answers on: E2B's base template has no `shaked`, so the live check
/// through `Hosting` stops at the ticket.
const LIVE_LINK: &str = "wss://example.com/worker";

/// The ticket in `sb`, and whether its partial file is gone (renamed onto it).
async fn handed_ticket(e2b: &E2bBackend, sb: &ProviderRef) -> (anyhow::Result<Vec<u8>>, bool) {
    let ticket = e2b.read_file(sb, TICKET_PATH).await;
    let partial = e2b.read_file(sb, &partial_of(TICKET_PATH)).await;
    (ticket, partial.is_err())
}

/// Stops `branch` on `fx` and waits for its watcher to kill `sb` (killed here if it doesn't):
/// whether the watcher did.
async fn stopped_and_killed(fx: &AppFx, e2b: &E2bBackend, branch: &str, sb: &ProviderRef) -> bool {
    let (status, v) = fx
        .post(&format!("/api/branches/{branch}/stop"), json!({}))
        .await;
    println!("stop: {status} {v}");
    let gone = async || (e2b.state(sb).await.ok()? == ProviderState::Gone).then_some(());
    let killed = poll_until(WAIT, STEP, gone).await.is_some();
    if !killed {
        e2b.kill(sb).await.unwrap();
    }
    killed
}

/// The ticket was handed over whole (newline-ended, its partial file renamed), and the
/// watcher killed the sandbox.
fn assert_handed_and_killed(ticket: anyhow::Result<Vec<u8>>, renamed: bool, killed: bool) {
    let ticket = String::from_utf8(ticket.unwrap()).unwrap();
    assert!(ticket.starts_with("sdt_") && ticket.ends_with('\n'));
    assert!(renamed, "written beside its path, then renamed");
    assert!(killed, "killed by its watcher");
}

#[test]
#[ignore = "live: creates a real E2B sandbox through Hosting (needs E2B_API_KEY)"]
fn live_hosting_hands_an_e2b_sandbox_its_ticket_and_kills_it() {
    let Ok(key) = std::env::var("E2B_API_KEY") else {
        println!("skipped: E2B_API_KEY is not set");
        return;
    };
    run(async {
        let (fx, _) = control("live").await;
        let e2b = Arc::new(E2bBackend::new(key, API.parse().unwrap()));
        host(&fx, e2b.clone(), LIVE_LINK, |h| {
            h.with_longest_run(Duration::from_secs(60))
        });
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let sb = linked_live(&e2b, &id).await;
        println!("created E2B sandbox {} for branch {id}", sb.0);
        let (ticket, renamed) = handed_ticket(&e2b, &sb).await;
        let killed = stopped_and_killed(&fx, &e2b, &id, &sb).await;
        println!("killed E2B sandbox {} (by the watcher: {killed})", sb.0);
        assert_handed_and_killed(ticket, renamed, killed);
        let ours = labels(&[("shaked", "example.com")]);
        let left = e2b.list(&ours).await.unwrap();
        assert!(left.is_empty(), "{left:?}");
        println!("ticket handed over whole at {TICKET_PATH}; no sandbox of ours left");
    });
}

/// The E2B sandbox `Hosting` made for `branch` (listed by its label).
async fn linked_live(e2b: &E2bBackend, branch: &str) -> ProviderRef {
    let labels = labels(&[("branch", branch)]);
    let listed = async || e2b.list(&labels).await.unwrap().into_iter().next();
    poll_until(WAIT, STEP, listed)
        .await
        .expect("its sandbox")
        .sb
}
