//! Built worlds: shaked asks the world builder for templates and previews, builds a world into
//! `SHAKEDOWN_WORLDS` (a benchmark package it installs while it runs), prepares its base in the
//! background and makes it the current benchmark. Runs, suites and experiments keep the benchmark
//! they were created with. The builder here is `tests/fixtures/builder.py` (one template `toy`);
//! the live test at the end uses the real one (`benchmarks/world`) and its small manufacturer.
mod support;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use parts::{Hub, Registry};
use serde_json::{json, Value};
use shaked::providers::ProviderConfig;
use support::{
    app,
    packages::{bin_dir, counter, package_root, toy, Package, TestHub, ToyRoot},
    run, AppFx,
};

/// A test's packages and its worlds folder (removed on drop).
struct Roots {
    toys: ToyRoot,
    worlds: ToyRoot,
}

impl Roots {
    fn all(&self) -> Vec<PathBuf> {
        vec![self.toys.0.clone(), self.worlds.0.clone()]
    }

    /// The folders in the worlds root, temporary ones too.
    fn written(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&self.worlds.0) else {
            return vec![];
        };
        let names = rd.filter_map(|e| e.ok()?.file_name().into_string().ok());
        names.filter(|n| !n.starts_with(".current")).collect()
    }
}

/// The toy builder over the toy benchmark run with the integration `part`.
fn builder(part: &str) -> Package {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let script = |f: &str| fixtures.join(f).to_string_lossy().into_owned();
    let args = ["--toy", &script("toy.py"), "--integration", part];
    let mut command = vec!["python3".to_string(), script("builder.py")];
    command.extend(args.iter().map(|a| a.to_string()));
    Package {
        name: "toy-builder".into(),
        kind: "builder",
        command,
    }
}

/// The part `part`, the toy benchmark `toybench` over it, and the toy builder; an empty worlds
/// folder.
fn roots(tag: &str, part: Package) -> Roots {
    let bench = toy(
        "toybench",
        "benchmark",
        &["bench", "--integration", &part.name],
    );
    let b = builder(&part.name);
    let toys = package_root(tag, &[part, bench, b]);
    let worlds = ToyRoot(toys.0.with_extension("worlds"));
    let _ = std::fs::remove_dir_all(&worlds.0);
    std::fs::create_dir_all(&worlds.0).unwrap();
    Roots { toys, worlds }
}

/// A test app over `roots` (its open agents call `upstream`), on `toybench` until a world is made
/// current.
struct Fx {
    app: AppFx,
    roots: Roots,
}

impl Fx {
    async fn on(tag: &str, roots: Roots, upstream: &str) -> Fx {
        let worlds = roots.worlds.0.clone();
        let providers = ProviderConfig::with_upstream(upstream);
        let app = AppFx::idle("worlds", tag)
            .hub(TestHub::new(&roots.all()))
            .bench("toybench")
            .providers(providers)
            .config(move |c| c.worlds_dir = Some(worlds))
            .start()
            .await;
        Fx { app, roots }
    }

    /// An app whose worlds run on the kit's counter (saves on disk, kept across a restart; it
    /// reads them back only under the name `counter`).
    async fn counter(tag: &str) -> Fx {
        let part = counter("counter", &[]);
        Fx::on(tag, roots(tag, part), app::NOWHERE).await
    }

    async fn build(&self, company: &str) -> (u16, Value) {
        let body = json!({"template": "toy", "choices": {"company": {"name": company}}});
        self.app.post("/api/worlds", body).await
    }

    /// Builds `company`'s world and waits until it is ready: its row.
    async fn built(&self, company: &str) -> Value {
        let (status, v) = self.build(company).await;
        assert_eq!(status, 202, "{v}");
        wait_world(&self.app, v["name"].as_str().unwrap(), "ready").await
    }
}

/// The row of world `name` once its state is `want` (a failure, when ready is wanted, panics with
/// its error).
async fn wait_world(app: &AppFx, name: &str, want: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        let (status, v) = app.get(&format!("/api/worlds/{name}")).await;
        assert_eq!(status, 200, "{v}");
        if v["state"] == want {
            return v;
        }
        assert!(v["state"] != "failed" || want != "ready", "{v}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("world {name} did not get {want}");
}

#[test]
fn templates_are_listed_shown_and_previewed() {
    run(async {
        let fx = Fx::counter("wtpl").await;
        let (status, list) = fx.app.get("/api/templates").await;
        assert_eq!((status, &list[0]["id"]), (200, &json!("toy")), "{list}");
        let (status, form) = fx.app.get("/api/templates/toy").await;
        assert_eq!((status, &form["company"]["name"]), (200, &json!("Toy Co")));
        let (status, v) = fx.app.get("/api/templates/nope").await;
        assert_eq!((status, &v["error"]), (404, &json!("no template nope")));
        let choices = json!({"company": {"name": "Acme Tools"}});
        let (status, p) = fx.app.post("/api/templates/toy/preview", choices).await;
        assert_eq!((status, &p["name"]), (200, &json!("acme-tools")), "{p}");
    });
}

#[test]
fn without_a_builder_templates_are_a_404() {
    run(async {
        let part = counter("counter", &[]);
        let bench = toy(
            "toybench",
            "benchmark",
            &["bench", "--integration", &part.name],
        );
        let toys = package_root("wnob", &[part, bench]);
        let registry = Registry::scan(std::slice::from_ref(&toys.0)).unwrap();
        let saves = toys.0.with_extension("saves");
        let hub = Arc::new(Hub::new(registry, bin_dir(), saves.clone()));
        let (store, _clean) = app::meta_store("worlds", "wnob").await;
        let (state, base) = app::idle_app(store, hub, Some("toybench"), |_| {}).await;
        let (status, v) =
            app::send(reqwest::Client::new().get(format!("{base}/api/templates"))).await;
        assert_eq!(
            (status, &v["error"]),
            (404, &json!("no world builder is installed"))
        );
        drop(state);
        let _ = std::fs::remove_dir_all(saves);
    });
}

#[test]
fn building_a_world_installs_it_prepares_it_and_makes_it_current() {
    run(async {
        let fx = Fx::counter("wbuild").await;
        let (status, v) = fx.build("Acme Tools").await;
        assert_eq!(
            (status, &v),
            (202, &json!({"name": "acme-tools", "state": "preparing"}))
        );
        let ready = wait_world(&fx.app, "acme-tools", "ready").await;
        assert!(ready["elapsed_ms"].as_u64().unwrap() < 20_000, "{ready}");
        let want = json!({"id": "toy", "name": "Toy"});
        let row = (
            &ready["title"],
            &ready["template"],
            &ready["built"],
            &ready["current"],
        );
        assert_eq!(
            row,
            (&json!("Acme Tools"), &want, &json!(true), &json!(true))
        );
        let health = fx.app.get("/api/health").await.1;
        assert_eq!(health["benchmark"]["name"], "acme-tools");
        assert_eq!(fx.roots.written(), ["acme-tools"]);
    });
}

#[test]
fn every_world_is_listed_with_the_current_one_marked() {
    run(async {
        let fx = Fx::counter("wlist").await;
        let (_, before) = fx.app.get("/api/worlds").await;
        let toy = before
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["name"] == "toybench");
        let toy = toy.unwrap_or_else(|| panic!("toybench is listed: {before}"));
        let row = (&toy["built"], &toy["current"], &toy["template"]);
        assert_eq!(row, (&json!(false), &json!(true), &Value::Null));
        fx.built("Acme Tools").await;
        let (_, after) = fx.app.get("/api/worlds").await;
        let current: Vec<&Value> = (after.as_array().unwrap().iter())
            .filter(|w| w["current"] == true)
            .map(|w| &w["name"])
            .collect();
        assert_eq!(current, [&json!("acme-tools")]);
    });
}

#[test]
fn a_taken_name_is_refused_with_a_suggestion() {
    run(async {
        let fx = Fx::counter("wtaken").await;
        fx.built("Acme Tools").await;
        let (status, v) = fx.build("Acme Tools").await;
        assert_eq!(
            (status, &v["suggested"]),
            (409, &json!("acme-tools-2")),
            "{v}"
        );
        let said = "a world or package named acme-tools exists: give the company another name";
        assert_eq!(v["error"], said);
        let (status, v) = fx.build("Toybench").await;
        assert_eq!(
            (status, &v["suggested"]),
            (409, &json!("toybench-2")),
            "{v}"
        );
        let long = "x".repeat(40);
        fx.built(&long).await;
        let (status, v) = fx.build(&long).await;
        let want = format!("{}-2", "x".repeat(38));
        assert_eq!((status, &v["suggested"]), (409, &json!(want)), "{v}");
    });
}

#[test]
fn problems_are_a_422_and_nothing_is_written() {
    run(async {
        let fx = Fx::counter("wprob").await;
        let (status, v) = fx.build("  ").await;
        assert_eq!(status, 422, "{v}");
        assert_eq!(v["problems"][0]["message"], "Give the company a name");
        assert!(fx.roots.written().is_empty(), "{:?}", fx.roots.written());
    });
}

#[test]
fn a_double_build_is_refused_while_preparing() {
    run(async {
        let fx = Fx::counter("wtwice").await;
        let (first, second) = tokio::join!(fx.build("Acme Tools"), fx.build("Acme Tools"));
        let mut statuses = [first.0, second.0];
        statuses.sort();
        assert_eq!(statuses, [202, 409], "{first:?} {second:?}");
        let (status, v) = fx
            .app
            .post("/api/worlds/acme-tools/prepare", json!({}))
            .await;
        assert!(status == 409 || status == 202, "{status} {v}");
        wait_world(&fx.app, "acme-tools", "ready").await;
        assert_eq!(fx.roots.written(), ["acme-tools"]);
    });
}

#[test]
fn a_failed_prepare_can_be_prepared_again() {
    run(async {
        let refuse = std::env::temp_dir().join(format!("shaked-wfail-{}", std::process::id()));
        std::fs::write(&refuse, "").unwrap();
        let flag = refuse.to_string_lossy().into_owned();
        let part = toy(
            "toy-wfail",
            "integration",
            &["part", "--refuse-start-if", &flag],
        );
        let fx = Fx::on("wfail", roots("wfail", part), app::NOWHERE).await;
        let (status, v) = fx.build("Acme Tools").await;
        assert_eq!(status, 202, "{v}");
        let failed = wait_world(&fx.app, "acme-tools", "failed").await;
        assert!(
            failed["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{failed}"
        );
        assert_eq!(failed["current"], false);
        std::fs::remove_file(&refuse).unwrap();
        let (status, _) = fx
            .app
            .post("/api/worlds/acme-tools/prepare", json!({}))
            .await;
        assert_eq!(status, 202);
        let ready = wait_world(&fx.app, "acme-tools", "ready").await;
        assert_eq!(
            (&ready["current"], &ready["error"]),
            (&json!(true), &Value::Null)
        );
    });
}

#[test]
fn choosing_a_world_makes_it_current_and_an_unknown_one_is_a_404() {
    run(async {
        let fx = Fx::counter("wuse").await;
        fx.built("Acme Tools").await;
        let (status, row) = fx.app.post("/api/worlds/toybench/use", json!({})).await;
        assert_eq!((status, &row["current"]), (200, &json!(true)), "{row}");
        assert_eq!(
            fx.app.get("/api/health").await.1["benchmark"]["name"],
            "toybench"
        );
        let (status, v) = fx.app.post("/api/worlds/nope/use", json!({})).await;
        assert_eq!(
            (status, &v["error"]),
            (404, &json!("no benchmark named nope"))
        );
        let (status, _) = fx.app.get("/api/worlds/nope").await;
        assert_eq!(status, 404);
    });
}

/// A built world is installed from `SHAKEDOWN_WORLDS` when shaked starts again, ready since its base
/// saves exist, and still current: shaked remembers the choice (owner ruling, 2026-11-19).
#[test]
fn a_built_world_survives_a_restart() {
    run(async {
        let fx = Fx::counter("wrestart").await;
        fx.built("Acme Tools").await;
        let hub = fx.app.hub.sibling(&fx.roots.all());
        let (store, _clean) = app::meta_store("worlds", "wrestart2").await;
        let worlds = fx.roots.worlds.0.clone();
        let (_state, base) = app::idle_app(store, hub, None, |c| c.worlds_dir = Some(worlds)).await;
        let url = format!("{base}/api/worlds/acme-tools");
        let (status, w) = app::send(reqwest::Client::new().get(url)).await;
        assert_eq!(status, 200, "{w}");
        let row = (&w["built"], &w["state"], &w["current"]);
        assert_eq!(row, (&json!(true), &json!("ready"), &json!(true)), "{w}");
    });
}

/// The packages shaked starts over, alone: `toybench` (the only one not opt-in), the opt-in toy
/// benchmark `acme` and the opt-in `broken` (its program exits at once, so it can't describe
/// itself); `.current` in its worlds folder names `remembered`.
struct Start {
    toys: ToyRoot,
    saves: ToyRoot,
}

impl Start {
    fn new(tag: &str, remembered: &str) -> Start {
        let part = toy(&format!("toy-{tag}"), "integration", &["part"]);
        let bench = |name: &str| {
            toy(
                name,
                "benchmark",
                &["bench", "--name", name, "--integration", &part.name],
            )
        };
        let (toybench, acme) = (bench("toybench"), bench("acme"));
        let broken = Package {
            name: "broken".into(),
            kind: "benchmark",
            command: vec!["false".into()],
        };
        let toys = package_root(tag, &[toybench, acme, broken, part]);
        for opt_in in ["acme", "broken"] {
            let manifest = toys.0.join(opt_in).join("arena-package.toml");
            let text = std::fs::read_to_string(&manifest).unwrap();
            std::fs::write(&manifest, format!("{text}opt_in = true\n")).unwrap();
        }
        std::fs::write(toys.0.join(".current"), format!("{remembered}\n")).unwrap();
        let saves = ToyRoot(toys.0.with_extension("saves"));
        Start { toys, saves }
    }

    /// shaked's state as it starts over these packages, `SHAKEDOWN_BENCHMARK` being `env`.
    async fn state(&self, env: Option<&str>) -> (support::pg::Cleanup, shaked::app::AppState) {
        let registry = Registry::scan(std::slice::from_ref(&self.toys.0)).unwrap();
        let hub = Arc::new(Hub::new(registry, bin_dir(), self.saves.0.clone()));
        let (store, clean) = app::meta_store("worlds", "wstart").await;
        let mut config = shaked::app::Config::new();
        config.worlds_dir = Some(self.toys.0.clone());
        config.start_benchmark = env.map(str::to_string);
        (clean, shaked::app::AppState::new(store, hub, config))
    }
}

#[test]
fn arena_benchmark_wins_over_the_remembered_world() {
    run(async {
        let start = Start::new("wenv", "acme");
        let (_clean, state) = start.state(Some("toybench")).await;
        assert_eq!(state.benchmark(), "toybench");
        assert_eq!(state.startup_bench().await.unwrap().name, "toybench");
    });
}

#[test]
fn the_remembered_world_wins_over_the_only_benchmark_installed() {
    run(async {
        let start = Start::new("wrem", "acme");
        let (_clean, state) = start.state(None).await;
        assert_eq!(state.benchmark(), "acme");
        assert_eq!(state.startup_bench().await.unwrap().name, "acme");
    });
}

#[test]
fn a_remembered_world_that_cant_describe_itself_is_left_for_the_only_one_installed() {
    run(async {
        let start = Start::new("wbroken", "broken");
        let (_clean, state) = start.state(None).await;
        let bench = state.startup_bench().await;
        assert_eq!(bench.map(|b| b.name).unwrap(), "toybench");
        assert_eq!(state.benchmark(), "toybench");
    });
}

/// A provider that accepts connections and never answers: a run on it stays running.
async fn silent_provider() -> String {
    let (listener, base) = app::bind().await;
    tokio::spawn(async move {
        let mut held = vec![];
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    base
}

impl Fx {
    /// An experiment of two runs of `t1` by one open agent: its id.
    async fn experiment(&self) -> String {
        let agent = json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m",
                           "prompt": "Poke the box."});
        let team = json!({"id": "a", "name": "Team a", "agents": [agent]});
        let body = json!({"task": "t1", "runs": 2, "setups": [{"team": team}]});
        let (status, v) = self.app.post("/api/experiments", body).await;
        assert_eq!(status, 200, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    /// The branch of the experiment's `n`-th run once it is running.
    async fn running(&self, exp: &str, n: usize) -> String {
        let mut v = Value::Null;
        for _ in 0..300 {
            v = self.app.get(&format!("/api/experiments/{exp}")).await.1;
            let r = &v["setups"][0]["runs"][n];
            if r["status"] == "running" {
                return r["branch_id"].as_str().unwrap().to_string();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("run {n} of {exp} did not start: {v}");
    }

    /// The benchmark the branch's `RunStart` names.
    async fn benchmark_of(&self, branch: &str) -> Value {
        let events = self.app.state.store.events(branch).await.unwrap();
        events[0].payload["benchmark"].clone()
    }
}

#[test]
fn an_experiment_keeps_its_world_when_another_is_chosen() {
    run(async {
        let part = counter("counter", &[]);
        let fx = Fx::on("wexp", roots("wexp", part), &silent_provider().await).await;
        fx.built("Acme Tools").await;
        let exp = fx.experiment().await;
        let first = fx.running(&exp, 0).await;
        let (status, _) = fx.app.post("/api/worlds/toybench/use", json!({})).await;
        assert_eq!(status, 200);
        let stop = format!("/api/branches/{first}/stop");
        assert_eq!(fx.app.post(&stop, json!({})).await.0, 200);
        let second = fx.running(&exp, 1).await;
        fx.app
            .post(&format!("/api/experiments/{exp}/stop"), json!({}))
            .await;
        assert_eq!(fx.benchmark_of(&first).await, "acme-tools");
        assert_eq!(fx.benchmark_of(&second).await, "acme-tools");
    });
}

/// The current `toybench` and the opt-in `acme` over one toy part `box`; only `acme` has task `t2`
/// and offers a disruption (`jam`, after `{after}` calls). Open agents call a provider that never answers.
async fn asked_for(tag: &str) -> AppFx {
    let part = toy(&format!("toy-{tag}"), "integration", &["part"]);
    let after =
        json!({"name": "after", "label": "After", "kind": "int", "min": 0, "max": 9, "default": 1});
    let jam = json!({"id": "jam", "label": "Jam", "description": "It jams after {after} calls",
        "part": "box", "params": [after],
        "template": {"id": "jam-{after}", "when": {"after_calls": "{after}"}, "do": {"kind": "jam"},
                     "text": "The box jams"}});
    let options = json!({"base": ["t1"], "disruptions": [jam],
                         "setup": {"box": {"tool": "poke", "args": {}}}})
    .to_string();
    let bench = |name: &str, extra: &[&str]| {
        let args = [
            &["bench", "--name", name, "--integration", &part.name],
            extra,
        ]
        .concat();
        toy(name, "benchmark", &args)
    };
    let toybench = bench("toybench", &[]);
    let acme = bench("acme", &["--two-tasks", "--scenario-options", &options]);
    let root = package_root(tag, &[toybench, acme, part]);
    AppFx::idle("worlds", tag)
        .root(root)
        .bench("toybench")
        .providers(ProviderConfig::with_upstream(&silent_provider().await))
        .start()
        .await
}

fn jam() -> Value {
    json!({"disruptions": [{"id": "jam", "params": {"after": 2}}]})
}

/// A team of one open agent.
fn planner() -> Value {
    let agent = json!({"id": "planner", "name": "Planner", "kind": "open", "model": "m",
                       "prompt": "Poke the box."});
    json!({"id": "a", "name": "Team a", "agents": [agent]})
}

/// Starts a run of `task` with no agent, which an external system drives, on `bench` in
/// `scenario`, and finishes it: the status, and `{branch_id, start}` (its `RunStart`) or the error
/// answered.
async fn external_run(
    fx: &AppFx,
    task: &str,
    bench: Option<&str>,
    scenario: Value,
) -> (u16, Value) {
    let team = json!({"id": "own", "name": "Own", "agents": []});
    let body = json!({"task_id": task, "team": team, "runner": "external", "scenario": scenario,
                      "benchmark": bench});
    let (status, v) = fx.post("/api/runs", body).await;
    let Some(branch) = v["branch_id"].as_str() else {
        return (status, v);
    };
    fx.post(&format!("/api/branches/{branch}/finish"), json!({}))
        .await;
    let events = fx.state.store.events(branch).await.unwrap();
    (
        status,
        json!({"branch_id": branch, "start": events[0].payload}),
    )
}

#[test]
fn a_run_runs_on_the_benchmark_it_asks_for_in_a_scenario_of_that_benchmark() {
    run(async {
        let fx = asked_for("wrun").await;
        let (status, v) = external_run(&fx, "t1", Some("acme"), jam()).await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["start"]["benchmark"], "acme");
        let jammed = &v["start"]["scenario"]["disruptions"][0];
        assert_eq!(
            (&jammed["id"], &jammed["params"]["after"]),
            (&json!("jam"), &json!(2))
        );
        let (status, v) = external_run(&fx, "t1", Some("nope"), json!(null)).await;
        assert_eq!(
            (status, &v["error"]),
            (409, &json!("no benchmark named nope is installed"))
        );
        let (status, v) = external_run(&fx, "t1", None, json!(null)).await;
        assert_eq!(
            (status, &v["start"]["benchmark"]),
            (200, &json!("toybench"))
        );
    });
}

#[test]
fn an_experiment_runs_on_the_benchmark_it_asks_for_in_a_scenario_of_that_benchmark() {
    run(async {
        let fx = asked_for("wexpask").await;
        let body = |bench: &str| {
            json!({"task": "t1", "runs": 1, "setups": [{"team": planner()}], "scenario": jam(),
                   "benchmark": bench})
        };
        let (status, v) = fx.post("/api/experiments", body("acme")).await;
        assert_eq!(status, 200, "{v}");
        let id = v["id"].as_str().unwrap();
        fx.post(&format!("/api/experiments/{id}/stop"), json!({}))
            .await;
        let stored = fx.state.store.experiment(id).await.unwrap().unwrap();
        assert_eq!(stored.benchmark, "acme");
        let (status, v) = fx.post("/api/experiments", body("nope")).await;
        assert_eq!(
            (status, &v["error"]),
            (409, &json!("no benchmark named nope is installed"))
        );
    });
}

#[test]
fn a_suite_runs_on_the_benchmark_it_asks_for_in_scenarios_of_that_benchmark() {
    run(async {
        let fx = asked_for("wsuiteask").await;
        let team = json!({"id": "own", "name": "Own", "agents": []});
        let body = |bench: &str| {
            json!({"team": team, "runner": "external", "tasks": ["t1"], "scenarios": [jam()],
                   "benchmark": bench})
        };
        let (status, v) = fx.post("/api/suites", body("acme")).await;
        assert_eq!(status, 200, "{v}");
        let stored = fx.state.store.suite(v["id"].as_str().unwrap()).await;
        assert_eq!(stored.unwrap().unwrap().benchmark, "acme");
        let (status, v) = fx.post("/api/suites", body("nope")).await;
        assert_eq!(
            (status, &v["error"]),
            (409, &json!("no benchmark named nope is installed"))
        );
    });
}

/// A run is reviewed with the ticket of its own benchmark's task, whatever is current since.
#[test]
fn a_run_is_reviewed_with_the_ticket_of_its_own_benchmark() {
    run(async {
        let fx = asked_for("wreview").await;
        let (status, v) = external_run(&fx, "t2", Some("acme"), json!(null)).await;
        assert_eq!(status, 200, "{v}");
        let events = fx
            .state
            .store
            .events(v["branch_id"].as_str().unwrap())
            .await;
        let ticket = shaked::review::ticket(&fx.state, &events.unwrap()).await;
        assert_eq!(ticket.as_deref(), Some("# Ticket\n\nPoke the box twice.\n"));
    });
}

/// A world asked for while one benchmark (`first-co`) is current, and prepared after another
/// (`toybench`) was chosen, is ready but not made current: the choice made since stands.
#[test]
fn a_world_prepared_after_another_was_chosen_leaves_that_one_current() {
    run(async {
        let part = toy(
            "toy-wslow",
            "integration",
            &["part", "--save-sleep", "1500"],
        );
        let fx = Fx::on("wslow", roots("wslow", part), app::NOWHERE).await;
        fx.built("First Co").await;
        let (status, v) = fx.build("Acme Tools").await;
        assert_eq!(status, 202, "{v}");
        let (status, v) = fx.app.post("/api/worlds/toybench/use", json!({})).await;
        assert_eq!(status, 200, "{v}");
        let ready = wait_world(&fx.app, "acme-tools", "ready").await;
        assert_eq!(ready["current"], json!(false), "{ready}");
        let health = fx.app.get("/api/health").await.1;
        assert_eq!(health["benchmark"]["name"], "toybench");
    });
}

/// The real builder and its small manufacturer template: built, prepared and current in under a
/// minute (stage 3, W8). Needs `uv`, the template's integrations and Docker images.
#[test]
#[ignore = "live: the real builder, its template's integrations and their images"]
fn live_a_small_manufacturer_world_is_ready_in_under_a_minute() {
    run(async {
        let worlds = ToyRoot(
            std::env::temp_dir().join(format!("shaked-live-worlds-{}", std::process::id())),
        );
        std::fs::create_dir_all(&worlds.0).unwrap();
        let dir = worlds.0.clone();
        let fx = AppFx::idle("worlds", "wlive")
            .hub(TestHub::new(std::slice::from_ref(&worlds.0)))
            .bench("retail")
            .config(move |c| c.worlds_dir = Some(dir))
            .start()
            .await;
        let body = json!({"template": "small-manufacturer", "choices": {"company": {"name": "Live Parts"}}});
        let (status, v) = fx.post("/api/worlds", body).await;
        assert_eq!(status, 202, "{v}");
        let ready = wait_world(&fx, "live-parts", "ready").await;
        eprintln!("live: ready in {} ms", ready["elapsed_ms"]);
        assert!(ready["elapsed_ms"].as_u64().unwrap() < 60_000, "{ready}");
    });
}
