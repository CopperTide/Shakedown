//! Branches run in sandboxes. A test control box with `Hosting` over the fake backend (each
//! sandbox a local `shaked worker`) runs the toy world with a scripted agent. `live_…` run on
//! real E2B with `E2B_API_KEY` set and are `#[ignore]`d:
//! `cargo test -p shaked --test hosting -- --ignored live_ --nocapture`. Each kills every
//! sandbox it made, also when it fails, and its sandboxes time out on their own anyway.
mod support;
use support::link::ws_url;
use support::scripted::{nothing, scripted_team};
use support::{live, packages, poll_until, records, run, AppFx, STEP, WAIT};

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use serde_json::{json, Value};
use shaked::{
    hosting::{
        e2b::{E2bBackend, API},
        fake::FakeBackend,
        Hosting, ProviderRef, ProviderState, SandboxBackend, SandboxSpec, TICKET_PATH,
    },
    model::*,
};

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
