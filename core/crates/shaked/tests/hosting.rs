//! Branches run in sandboxes. A test control box with `Hosting` over the fake backend (each
//! sandbox a local `shaked worker`) runs the toy world with a scripted agent. `live_…` run on
//! real E2B with `E2B_API_KEY` set and are `#[ignore]`d:
//! `cargo test -p shaked --test hosting -- --ignored live_ --nocapture`. Each kills every
//! sandbox it made, also when it fails, and its sandboxes time out on their own anyway.
mod support;
use support::fake_apis::{fake_apis, test_vault, FakeApis, Seen};
use support::link::ws_url;
use support::packages::{Package, TestHub, ToyRoot};
use support::scripted::{nothing, scripted_team, Between};
use support::{live, packages, poll_until, records, run, AppFx, STEP, WAIT};

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use serde_json::{json, Value};
use shaked::{
    hosting::{
        e2b::{E2bBackend, API},
        fake::FakeBackend,
        partial_of, Hosting, ProviderRef, ProviderState, SandboxBackend, SandboxSpec, BUSY,
        LONGEST_REACHED, SANDBOX_STOPPED, SANDBOX_WORLDS, TICKET_PATH,
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

/// [`control`], its toy world built here: its worlds folder is the toy packages' folder.
async fn control_built(tag: &str) -> (AppFx, PathBuf) {
    let root = packages::toy_world(&format!("host-{tag}"), &[], &["--scored"]);
    let toys = root.0.clone();
    let worlds = toys.clone();
    let fx = AppFx::idle("hosting", tag).root(root).bench("toybench");
    let fx = fx
        .config(move |c| c.worlds_dir = Some(worlds))
        .start()
        .await;
    (fx, toys)
}

/// Runs `fx`'s branches in sandboxes of `template` from `backend` linking to `link`, its
/// `Hosting` `tweak`ed.
fn host(
    fx: &AppFx,
    backend: Arc<dyn SandboxBackend>,
    (link, template): (&str, &str),
    tweak: Tweak,
) {
    let hosting = Hosting::new(backend, link, template).unwrap();
    let hosting = tweak(hosting.with_poll(Duration::from_millis(200)));
    assert!(fx.state.hosting.set(Arc::new(hosting)).is_ok());
}

type Tweak = fn(Hosting) -> Hosting;

/// Runs `fx`'s branches in fake sandboxes, its `Hosting` `tweak`ed, whose workers find their
/// packages in `packages`: the backend.
fn fake_host(fx: &AppFx, packages: &std::path::Path, tweak: Tweak) -> Arc<FakeBackend> {
    let link = ws_url(&fx.base);
    let packages = packages.to_str().unwrap();
    let env = [
        ("SHAKEDOWN_TEST_GUARD", "1"),
        ("SHAKEDOWN_PACKAGES", packages),
    ];
    let fake = Arc::new(FakeBackend::new(env!("CARGO_BIN_EXE_shaked"), &link, &env));
    host(fx, fake.clone(), (&link, TEMPLATE), tweak);
    fake
}

/// A control box whose branches run in fake sandboxes, its `Hosting` `tweak`ed: the app and
/// the backend.
async fn hosted(tag: &str, tweak: Tweak) -> (AppFx, Arc<FakeBackend>) {
    let (fx, toys) = control(tag).await;
    let fake = fake_host(&fx, &toys, tweak);
    (fx, fake)
}

#[test]
fn a_sandbox_only_control_box_runs_a_branch_without_a_package_program_here() {
    run(async {
        let spy = shaked::harness::SpyLayer::new();
        let root = packages::toy_world("host-sandonly", &[], &["--scored"]);
        let toys = root.0.clone();
        let fx = AppFx::idle("hosting", "sandonly").layer(Arc::new(spy.clone()));
        let fx = fx.root(root).bench("toybench").start().await;
        spy.sandbox_packages();
        let fake = fake_host(&fx, &toys, |h| h);
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let b = live::ended(&fx.state, &id).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        killed(&fake, 1).await;
    });
}

/// A package `name` (a benchmark) whose program, if it ever started, would leave `marker`.
fn must_not_run(name: &str, marker: &std::path::Path) -> Package {
    let touch = format!("touch '{}'; exit 1", marker.display());
    let command = vec!["sh".into(), "-c".into(), touch];
    Package {
        name: name.into(),
        kind: "benchmark",
        command,
    }
}

/// `bench`'s description file in the repository.
fn describe_file(bench: &str) -> PathBuf {
    packages::repo()
        .join("benchmarks")
        .join(bench)
        .join(parts::DESCRIBE_FILE)
}

/// A control box whose packages run only in sandboxes over the packages of `root` (its hub
/// starts no program), new runs on `bench`.
async fn sandbox_only(tag: &str, root: &ToyRoot, bench: &str) -> AppFx {
    let spy = shaked::harness::SpyLayer::new();
    spy.sandbox_packages();
    let hub = TestHub::sandbox_only(std::slice::from_ref(&root.0));
    let fx = AppFx::idle("hosting", tag).layer(Arc::new(spy)).hub(hub);
    fx.bench(bench).start().await
}

/// A control box whose packages run only in sandboxes, over a stand-in for `bench` (its
/// description file, a program that must never start), answers `bench`'s `tasks` tasks and
/// starts a run of the first in a sandbox, starting no program here.
async fn runs_from_its_description(bench: &str, tasks: usize) {
    let tag = format!("sd{}", bench.replace('-', ""));
    let marker = packages::toy_dir(&format!("{tag}-ran"));
    let _ = std::fs::remove_file(&marker);
    let root = packages::package_root(&tag, &[must_not_run(bench, &marker)]);
    let copied = root.0.join(bench).join(parts::DESCRIBE_FILE);
    std::fs::copy(describe_file(bench), copied).unwrap();
    let fx = sandbox_only(&tag, &root, bench).await;
    let (status, health) = fx.get("/api/health").await;
    assert_eq!(status, 200, "{health}");
    assert_eq!(health["benchmark"]["tasks"], tasks, "{health}");
    let (status, list) = fx.get("/api/tasks").await;
    assert_eq!(status, 200, "{list}");
    // The sandbox's worker finds no packages: its branch ends there, nothing runs here.
    let fake = fake_host(&fx, &packages::toy_dir(&format!("{tag}-none")), |h| h);
    let team = scripted_team(nothing()).await;
    let body = json!({"task_id": list[0]["id"], "team": team, "runner": "chain"});
    let (status, v) = fx.post("/api/runs", body).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(fake.created(), 1);
    live::ended(&fx.state, v["branch_id"].as_str().unwrap()).await;
    assert!(
        !marker.exists(),
        "a package program started on the control box"
    );
}

#[test]
fn a_sandbox_only_control_box_reads_retails_description_and_starts_no_program() {
    run(runs_from_its_description("retail", 10));
}

#[test]
fn a_sandbox_only_control_box_runs_the_prebuilt_software_factory_from_its_description() {
    run(runs_from_its_description("software-factory", 2));
}

#[test]
fn hosted_world_building_is_409_where_packages_run_in_sandboxes() {
    run(async {
        let marker = packages::toy_dir("sd-build-ran");
        let _ = std::fs::remove_file(&marker);
        let builder = Package {
            kind: "builder",
            ..must_not_run("world-builder", &marker)
        };
        let bench = must_not_run("toybench", &marker);
        let root = packages::package_root("sd-build", &[builder, bench]);
        let fx = sandbox_only("sdbuild", &root, "toybench").await;
        let preview = json!({"company": {"name": "Acme"}});
        let build = json!({"template": "software-factory", "choices": {}});
        for (status, v) in [
            fx.get("/api/templates").await,
            fx.get("/api/templates/software-factory").await,
            fx.post("/api/templates/software-factory/preview", preview)
                .await,
            fx.post("/api/worlds", build).await,
        ] {
            assert_eq!(status, 409, "{v}");
            assert!(v["error"].to_string().contains("prebuilt worlds"), "{v}");
        }
        assert!(
            !marker.exists(),
            "the world builder started on the control box"
        );
    });
}

#[test]
fn a_benchmark_without_a_description_file_is_409_where_packages_run_in_sandboxes() {
    run(async {
        let marker = packages::toy_dir("sd-nodesc-ran");
        let _ = std::fs::remove_file(&marker);
        let root = packages::package_root("sd-nodesc", &[must_not_run("toybench", &marker)]);
        let fx = sandbox_only("sdnodesc", &root, "toybench").await;
        let (status, health) = fx.get("/api/health").await;
        assert_eq!(status, 200, "{health}");
        let (status, v) = fx.get("/api/tasks").await;
        assert_eq!(status, 409, "{v}");
        assert!(
            v["error"].to_string().contains("no description file"),
            "{v}"
        );
        assert!(
            !marker.exists(),
            "a package program started on the control box"
        );
    });
}

/// Writes `benchmarks/<bench>/describe.json` from `bench`'s live `describe` when
/// `SHAKEDOWN_WRITE_DESCRIBE` is set; else checks the file is it. The package's own folder is
/// `${WORLD_DIR}` in the file (a built world names its files under it), so the file holds
/// wherever the package is.
async fn description_file_is_live_describe(bench: &str) {
    let hub = TestHub::new(&[]);
    let m = hub.hub.registry().get(bench).cloned().unwrap();
    let live = hub.hub.bench_describe(bench).await.unwrap();
    let dir = m.dir.canonicalize().unwrap();
    let text = serde_json::to_string_pretty(&live).unwrap() + "\n";
    let text = text.replace(dir.to_str().unwrap(), "${WORLD_DIR}");
    if parts::settings::var_os("SHAKEDOWN_WRITE_DESCRIBE").is_some() {
        std::fs::write(describe_file(bench), &text).unwrap();
    }
    let live: parts::proto::BenchDescribe = serde_json::from_str(&text).unwrap();
    let file = parts::described::read(&m).unwrap();
    let stale = format!(
        "benchmarks/{bench}/describe.json is stale: run this test with SHAKEDOWN_WRITE_DESCRIBE=1"
    );
    assert_eq!(file, live, "{stale}");
}

#[test]
fn retails_description_file_is_its_live_describe() {
    run(description_file_is_live_describe("retail"));
}

#[test]
fn the_prebuilt_software_factorys_description_file_is_its_live_describe() {
    run(description_file_is_live_describe("software-factory"));
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

/// The run owner's own OpenRouter key, as the Settings page saves it.
const OWN_ROUTER_KEY: &str = "sk-or-own-key-4444";

/// A hosted control box whose providers are `up` (no key of the environment's), its caller's
/// own OpenRouter key saved under the name `openrouter`, on `up`: the app and the backend.
async fn keyed_hosted(tag: &str, up: &FakeApis) -> (AppFx, Arc<FakeBackend>) {
    let root = packages::toy_world(&format!("host-{tag}"), &[], &["--scored"]);
    let toys = root.0.clone();
    let fx = AppFx::idle("hosting", tag).root(root).bench("toybench");
    let fx = fx.providers(up.providers()).start().await;
    let _ = fx.state.vault.set(test_vault());
    let key = json!({"name": "openrouter", "kind": "openrouter", "secret": OWN_ROUTER_KEY,
                     "base_url": format!("{}/v1", up.base)});
    let (status, v) = fx.post("/api/models/keys", key).await;
    assert_eq!(status, 200, "{v}");
    let fake = fake_host(&fx, &toys, |h| h);
    (fx, fake)
}

/// A built-in API-mode agent `id` on `provider` (None: the default).
fn api_agent(id: &str, provider: Option<&str>) -> Value {
    json!({"id": id, "name": id, "kind": "open", "model": "m", "provider": provider})
}

#[test]
fn a_hosted_api_agent_calls_on_the_owners_key_by_name_or_by_kind() {
    run(async {
        let up = fake_apis().await;
        let (fx, fake) = keyed_hosted("keys", &up).await;
        let agents = [
            api_agent("named", Some("key:openrouter")),
            api_agent("plain", None),
        ];
        let team = json!({"id": "t", "name": "T", "agents": agents});
        let id = start(&fx, &serde_json::from_value(team).unwrap()).await;
        let b = live::ended(&fx.state, &id).await;
        assert_ne!(b.status, BranchStatus::InfraError, "{:?}", b.error);
        let seen = up.seen();
        let calls = seen.iter().filter(|s| s.path == "/v1/chat/completions");
        let auth = |s: &Seen| s.headers["authorization"].to_str().unwrap().to_string();
        let auths: Vec<String> = calls.map(auth).collect();
        let want = format!("Bearer {OWN_ROUTER_KEY}");
        assert_eq!(
            auths,
            [want.clone(), want],
            "one call per agent, on the owner's key"
        );
        killed(&fake, 1).await;
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

/// The status `branch` has now.
async fn status_of(fx: &AppFx, branch: &str) -> BranchStatus {
    records(&fx.state.store)
        .branch(branch)
        .await
        .unwrap()
        .status
}

/// `n` runs of `team` posted at once: their branches, the first running, the rest queued.
async fn posted(fx: &AppFx, team: &Team, n: usize) -> Vec<String> {
    let mut ids = vec![];
    for _ in 0..n {
        ids.push(start(fx, team).await);
    }
    ids
}

#[test]
fn runs_beyond_the_slot_wait_their_turn_and_all_finish() {
    run(async {
        let (fx, fake) = hosted("queue", |h| h).await;
        let gate = Arc::new(Semaphore::new(0));
        let team = scripted_team(gated(&gate)).await;
        let ids = posted(&fx, &team, 4).await;
        linked(&fx, &*fake, &ids[0]).await;
        assert_eq!(fake.created(), 1, "one sandbox at a time");
        for id in &ids[1..] {
            assert_eq!(status_of(&fx, id).await, BranchStatus::Queued);
        }
        let (_, v) = fx.get(&format!("/api/branches/{}", ids[2])).await;
        assert_eq!(v["queue_position"], 2, "{v}");
        gate.add_permits(4);
        for id in &ids {
            let b = live::ended(&fx.state, id).await;
            assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        }
        killed(&fake, 4).await;
        assert_eq!(fake.created(), 4);
    });
}

#[test]
fn a_queued_run_stops_at_once_and_never_gets_a_sandbox() {
    run(async {
        let (fx, fake) = hosted("unqueue", |h| h).await;
        let gate = Arc::new(Semaphore::new(0));
        let team = scripted_team(gated(&gate)).await;
        let ids = posted(&fx, &team, 2).await;
        let (status, v) = fx
            .post(&format!("/api/branches/{}/stop", ids[1]), json!({}))
            .await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(status_of(&fx, &ids[1]).await, BranchStatus::Stopped);
        gate.add_permits(2);
        let b = live::ended(&fx.state, &ids[0]).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        let next = start(&fx, &team).await;
        assert_eq!(
            live::ended(&fx.state, &next).await.status,
            BranchStatus::Passed
        );
        killed(&fake, 2).await;
        assert_eq!(fake.created(), 2, "none for the stopped run");
    });
}

#[test]
fn a_full_queue_refuses_and_a_restart_ends_the_queued_runs() {
    run(async {
        let (fx, fake) = hosted("full", |h| h.with_queue_bound(1)).await;
        let gate = Arc::new(Semaphore::new(0));
        let team = scripted_team(gated(&gate)).await;
        let ids = posted(&fx, &team, 2).await;
        let (status, v) = post_run(&fx, &team).await;
        assert_eq!(status, 429, "{v}");
        assert!(v.to_string().contains(BUSY), "{v}");
        linked(&fx, &*fake, &ids[0]).await;
        // What a restart does first: the queue was in memory, so its runs end saying why.
        shaked::jobs::interrupt_at_start(&fx.state.store)
            .await
            .unwrap();
        let b = records(&fx.state.store).branch(&ids[1]).await.unwrap();
        let ended = (b.status, b.error.as_deref());
        assert_eq!(
            ended,
            (BranchStatus::Interrupted, Some(shaked::jobs::QUEUED_LOST))
        );
        assert_eq!(
            status_of(&fx, &ids[0]).await,
            BranchStatus::Running,
            "its worker goes on"
        );
        gate.add_permits(1);
        let b = live::ended(&fx.state, &ids[0]).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        killed(&fake, 1).await;
        assert_eq!(fake.created(), 1, "none for the ended queued run");
    });
}

#[test]
fn a_run_past_its_longest_is_stopped_and_its_sandbox_killed() {
    run(async {
        let (fx, fake) = hosted("longest", |h| h.with_longest_run(Duration::from_secs(1))).await;
        let gate = Arc::new(Semaphore::new(0));
        let id = start(&fx, &scripted_team(gated(&gate)).await).await;
        let b = live::ended(&fx.state, &id).await;
        let ended = (b.status, b.error.as_deref());
        assert_eq!(ended, (BranchStatus::Stopped, Some(LONGEST_REACHED)));
        killed(&fake, 1).await;
        gate.add_permits(1);
    });
}

/// A control box whose toy world is built here (its worlds folder is the toy packages' folder),
/// its branches run in fake sandboxes that have the toy integration only: the app and the
/// backend.
async fn built_world_hosted(tag: &str) -> (AppFx, Arc<FakeBackend>, tempfile::TempDir) {
    let (fx, toys) = control_built(tag).await;
    let parts = tempfile::tempdir().unwrap();
    let part = format!("toy-host-{tag}");
    std::os::unix::fs::symlink(toys.join(&part), parts.path().join(&part)).unwrap();
    let fake = fake_host(&fx, parts.path(), |h| h);
    (fx, fake, parts)
}

#[test]
fn a_world_built_here_is_copied_into_its_sandbox() {
    run(async {
        let (fx, fake, _parts) = built_world_hosted("built").await;
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let b = live::ended(&fx.state, &id).await;
        assert_eq!(b.status, BranchStatus::Passed, "{:?}", b.error);
        killed(&fake, 1).await;
    });
}

#[test]
fn a_world_too_big_for_a_sandbox_ends_its_branch_saying_so() {
    run(async {
        let (fx, toys) = control_built("big").await;
        let big = std::fs::File::create(toys.join("toybench/rows.json")).unwrap();
        big.set_len(65 << 20).unwrap();
        let fake = fake_host(&fx, &toys, |h| h);
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let b = live::ended(&fx.state, &id).await;
        assert_eq!(b.status, BranchStatus::InfraError);
        let why = b.error.unwrap_or_default();
        assert!(why.contains("over the 64 MiB a sandbox takes"), "{why}");
        assert_eq!(fake.created(), 0, "no sandbox for a world it can't take");
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

/// A link host the sandbox may reach and no worker answers on: the check stops at the ticket.
const LIVE_LINK: &str = "wss://example.com/worker";

/// The template the live check boots: `SHAKEDOWN_E2B_TEMPLATE` (a `shakedown-worker-…` one,
/// whose worker takes the ticket), else E2B's `base` (no worker: the ticket stays), and which.
fn live_template() -> (String, bool) {
    match std::env::var("SHAKEDOWN_E2B_TEMPLATE") {
        Ok(t) if !t.is_empty() => (t, true),
        _ => (TEMPLATE.to_string(), false),
    }
}

/// Whether `sb` got the built toy world and its ticket whole: with a worker, the worker took
/// the ticket (it and its partial file gone); without, the ticket is there, newline-ended.
async fn handed_over(e2b: &E2bBackend, sb: &ProviderRef, worker: bool) -> anyhow::Result<()> {
    let world = format!("{SANDBOX_WORLDS}/toybench/shakedown-package.toml");
    let manifest = String::from_utf8(e2b.read_file(sb, &world).await?)?;
    anyhow::ensure!(
        manifest.contains("toybench"),
        "the world's manifest: {manifest}"
    );
    let partial = partial_of(TICKET_PATH);
    if worker {
        let taken = async || {
            let gone = e2b.read_file(sb, TICKET_PATH).await.is_err();
            (gone && e2b.read_file(sb, &partial).await.is_err()).then_some(())
        };
        return poll_until(WAIT, STEP, taken)
            .await
            .context("the worker took no ticket");
    }
    let ticket = String::from_utf8(e2b.read_file(sb, TICKET_PATH).await?)?;
    anyhow::ensure!(
        ticket.starts_with("sdt_") && ticket.ends_with('\n'),
        "not a whole ticket"
    );
    anyhow::ensure!(
        e2b.read_file(sb, &partial).await.is_err(),
        "the partial file stayed"
    );
    Ok(())
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

#[test]
#[ignore = "live: creates a real E2B sandbox through Hosting (needs E2B_API_KEY)"]
fn live_hosting_hands_an_e2b_sandbox_its_world_and_ticket_and_kills_it() {
    let Ok(key) = std::env::var("E2B_API_KEY") else {
        println!("skipped: E2B_API_KEY is not set");
        return;
    };
    run(async {
        let (fx, _) = control_built("live").await;
        let e2b = Arc::new(E2bBackend::new(key, API.parse().unwrap()));
        let (template, worker) = live_template();
        let longest = |h: Hosting| h.with_longest_run(Duration::from_secs(60));
        host(&fx, e2b.clone(), (LIVE_LINK, &template), longest);
        let id = start(&fx, &scripted_team(nothing()).await).await;
        let sb = linked_live(&e2b, &id).await;
        println!("created E2B sandbox {} of {template} for branch {id}", sb.0);
        let handed = handed_over(&e2b, &sb, worker).await;
        let killed = stopped_and_killed(&fx, &e2b, &id, &sb).await;
        println!("killed E2B sandbox {} (by the watcher: {killed})", sb.0);
        handed.unwrap();
        assert!(killed, "killed by its watcher");
        let left = e2b
            .list(&labels(&[("shaked", "example.com")]))
            .await
            .unwrap();
        assert!(left.is_empty(), "{left:?}");
        println!("world and ticket handed over whole (worker took it: {worker}); none left");
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
