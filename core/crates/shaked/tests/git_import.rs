//! Import from git: a repo made in the test (a tiny project with one Python part, run through
//! uv, and one world) is fetched at a commit, installed out of its checkout, registered and
//! deployed; a new commit replaces its packages and keeps the project's own edits. The live test
//! imports the airline repo.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde_json::{json, Value};
use shaked::{
    ops::{find, mcp, OpCtx},
    project_state::ProjectState,
    providers::ProviderConfig,
};
use support::{app, run, AppFx};

/// uv runs the Python part; without it the tests are skipped, saying why.
fn uv_found() -> bool {
    let found = Command::new("uv").arg("--version").output().is_ok();
    if !found {
        eprintln!("skipped: uv is not on PATH");
    }
    found
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "git {args:?}: {err}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The project files of the fixture repo, for packages named `<p>-...`.
fn project_files(p: &str) -> Vec<(&'static str, String)> {
    let setup = "format: shakedown-setup/1\nlabel: Solo\nteam:\n  kind: builtin\n  agents:\n    - {id: planner, role: You poke boxes., model: claude-sonnet, tools: [\"*\"]}\n";
    vec![
        ("shakedown.yaml", "format: shakedown-project/1\nname: Toy ops\ndefaults:\n  world: box\n".into()),
        ("worlds/box/world.yaml", format!("format: shakedown-world/3\nlabel: The box\npackage: {p}-bench\n")),
        ("worlds/box/data/seed.json", "{\"n\": 0}\n".into()),
        ("scenarios/poke.yaml", "format: shakedown-scenario/1\nlabel: Poke it\nworld: box\ntask: Poke the box once.\npreset: t1\n".into()),
        ("setups/solo.yaml", setup.into()),
        ("experiments/try.yaml", "format: shakedown-experiment/1\nlabel: Try it\nworld: box\nscenario: poke\nsetups: [solo]\nrepeats: {per_setup: 1}\n".into()),
        ("ops/compose.yaml", "services:\n  cache:\n    image: redis:6.2-alpine\n    volumes: [cache-data:/data]\nvolumes:\n  cache-data: {}\n".into()),
    ]
}

/// The fixture's packages: the Python part `<p>-part` (the toy program, run with uv) and the
/// benchmark `<p>-bench` over it.
fn package_files(p: &str) -> Vec<(&'static str, String)> {
    let manifest = |kind: &str, name: &str, extra: &str, argv: &str| {
        format!("kind = \"{kind}\"\nname = \"{name}\"\nversion = \"0.1.0\"\nprotocol = 1\n{extra}run = {{ command = [{argv}] }}\n")
    };
    let part = r#""uv", "run", "--project", ".", "python", "toy.py", "part""#;
    let bench = format!(
        r#""uv", "run", "--project", "../parts/toy", "python", "../parts/toy/toy.py", "bench", "--name", "{p}-bench", "--integration", "{p}-part", "--scored""#
    );
    vec![
        ("parts/toy/pyproject.toml", "[project]\nname = \"toy-part\"\nversion = \"0.1.0\"\nrequires-python = \">=3.8\"\ndependencies = []\n\n[tool.uv]\npackage = false\n".into()),
        ("parts/toy/shakedown-package.toml", manifest("integration", &format!("{p}-part"), "", part)),
        ("bench/shakedown-package.toml", manifest("benchmark", &format!("{p}-bench"), "opt_in = true\n", &bench)),
    ]
}

/// A repo holding the fixture project and its packages, and its first commit.
fn fixture_repo(p: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    for (file, text) in project_files(p).into_iter().chain(package_files(p)) {
        write(r, file, &text);
    }
    let toy = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toy.py");
    std::fs::copy(toy, r.join("parts/toy/toy.py")).unwrap();
    git(r, &["init", "-q"]);
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "first"]);
    let commit = git(r, &["rev-parse", "HEAD"]);
    (dir, commit)
}

fn commit_change(repo: &Path, file: &str, text: &str) -> String {
    write(repo, file, text);
    git(repo, &["commit", "-q", "-am", "change"]);
    git(repo, &["rev-parse", "HEAD"])
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

/// `fields` of each entry of the list `key` in `v`, joined by spaces.
fn rows(v: &Value, key: &str, fields: &[&str]) -> Vec<String> {
    let row = |e: &Value| -> String {
        let words: Vec<&str> = fields.iter().map(|f| e[*f].as_str().unwrap()).collect();
        words.join(" ")
    };
    v[key].as_array().unwrap().iter().map(row).collect()
}

/// The plan: fetched and found, nothing installed, registered or written.
fn check_plan(fx: &AppFx, plan: &Value, commit: &str, project: &Path) {
    assert_eq!(plan["proposed"], true);
    assert_eq!(plan["commit"], commit);
    assert_eq!(
        rows(plan, "packages", &["name", "python"]),
        ["gia-bench parts/toy", "gia-part parts/toy"]
    );
    let want = [
        "write world box",
        "write scenario poke",
        "write setup solo",
        "write experiment try",
    ];
    assert_eq!(rows(plan, "items", &["action", "kind", "id"]), want);
    assert_eq!(plan["compose"][0]["file"], "ops/compose.yaml");
    assert_eq!(
        plan["compose"][0]["part"]["config"]["volumes"],
        json!(["cache-data"])
    );
    assert_eq!(plan["problems"], json!([]), "{plan}");
    assert!(!project.join("shakedown.yaml").exists());
    assert!(fx.state.hub.registry().get("gia-part").is_none());
}

/// Confirmed: installed out of the checkout, registered, written.
async fn check_imported(fx: &AppFx, done: &Value, commit: &str, project: &Path) {
    assert_eq!(done["registered"], true);
    assert_eq!(done["written"].as_array().unwrap().len(), 4);
    let checkout = PathBuf::from(done["checkout"].as_str().unwrap());
    assert!(checkout.join("parts/toy/toy.py").is_file());
    assert!(
        !checkout.join("parts/toy/.venv").exists(),
        "the environment is kept out of the checkout"
    );
    let env = PathBuf::from(done["python"][0]["env"].as_str().unwrap());
    assert!(env.join("pyvenv.cfg").is_file(), "{}", env.display());
    let part = fx.state.hub.registry().get("gia-part").cloned().unwrap();
    let uv_env = (
        "UV_PROJECT_ENVIRONMENT".to_string(),
        env.display().to_string(),
    );
    assert!(part.env.contains(&uv_env));
    let (_, status) = fx.get("/api/project").await;
    assert_eq!(
        (status["name"].as_str(), status["counts"]["worlds"].as_u64()),
        (Some("Toy ops"), Some(1))
    );
    assert_eq!(status["problems"], json!([]), "{status}");
    assert_eq!(status["imports"][0]["commit"], commit);
    assert!(project.join("worlds/box/data/seed.json").is_file());
}

/// The imported programs run from their environment, and a run deploys on them.
async fn check_deploys(fx: &AppFx) {
    let bench = fx.state.hub.bench_describe("gia-bench").await.unwrap();
    assert!(bench.tasks.iter().any(|t| t.id == "t1"));
    let (status, detail) = fx
        .post("/api/project/experiments/try/deploy", json!({}))
        .await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(detail["name"], "Try it");
    let (_, runs) = fx.get("/api/project/experiments/try/runs").await;
    assert_eq!(runs["deployments"].as_array().unwrap().len(), 1);
}

#[test]
fn a_repo_is_imported_registered_and_deployed() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, commit) = fixture_repo("gia");
        let project = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("gitimport", "deploy")
            .providers(ProviderConfig::with_upstream(&silent_provider().await))
            .project(project.path())
            .start()
            .await;
        let url = repo.path().display().to_string();
        let (status, plan) = fx
            .post("/api/project/import-git", json!({"url": url}))
            .await;
        assert_eq!(status, 200, "{plan}");
        check_plan(&fx, &plan, &commit, project.path());
        let confirmed = json!({"url": url, "confirm": true});
        let (status, done) = fx.post("/api/project/import-git", confirmed).await;
        assert_eq!(status, 200, "{done}");
        check_imported(&fx, &done, &commit, project.path()).await;
        check_deploys(&fx).await;
    });
}

/// `project.import_git` with `input` on `project` alone (as `shaked import` runs it).
async fn import_with(project: &Path, input: Value) -> Value {
    let state = Arc::new(ProjectState::new(Some(project.to_path_buf())));
    let ctx = OpCtx::project_only(state);
    let op = find("project.import_git").unwrap();
    let out = (op.run)(&ctx, op, input).await;
    out.unwrap_or_else(|e| panic!("{}", e.message))
}

/// The fixture repo at `url` imported as `toy` into `project` alone, confirmed.
async fn import_alone(project: &Path, url: &str) -> Value {
    import_with(project, json!({"url": url, "id": "toy", "confirm": true})).await
}

fn empty_hub(saves: &Path) -> parts::Hub {
    let saves = saves.to_path_buf();
    parts::Hub::new(parts::Registry::default(), saves.clone(), saves)
}

/// Registers the project's imports with `hub`, which then holds them at `commit`.
async fn registers_at(hub: &parts::Hub, project: &Path, commit: &str) {
    let errors = shaked::git_import::register(hub, project).await;
    assert_eq!(errors, Vec::<String>::new());
    assert_eq!(hub.source_version("toy").as_deref(), Some(commit));
}

/// The person edits the scenario in the project; the repo changes it and the world. Answers the
/// person's text and the new commit.
fn edit_both_sides(repo: &Path, project: &Path) -> (String, String) {
    let scenario = project.join("scenarios/poke.yaml");
    let mine = std::fs::read_to_string(&scenario)
        .unwrap()
        .replace("Poke it", "Poke it (ours)");
    std::fs::write(&scenario, &mine).unwrap();
    let theirs = "format: shakedown-scenario/1\nlabel: Poke it twice\nworld: box\ntask: Poke the box.\npreset: t1\n";
    commit_change(repo, "scenarios/poke.yaml", theirs);
    let world = "format: shakedown-world/3\nlabel: The new box\npackage: gib-bench\n";
    (mine, commit_change(repo, "worlds/box/world.yaml", world))
}

/// The second import replaced the world it wrote, kept the person's scenario and left the
/// unchanged setup.
fn check_second(done: &Value, project: &Path, mine: &str, commits: (&str, &str)) {
    assert_eq!(
        (done["commit"].as_str(), done["previous_commit"].as_str()),
        (Some(commits.1), Some(commits.0))
    );
    let actions = rows(done, "items", &["action", "id"]);
    for want in ["replace box", "keep poke", "same solo"] {
        assert!(
            actions.iter().any(|a| a == want),
            "{want} not in {actions:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.join("scenarios/poke.yaml")).unwrap(),
        mine
    );
    let world = std::fs::read_to_string(project.join("worlds/box/world.yaml")).unwrap();
    assert!(world.contains("The new box"));
}

#[test]
fn a_new_commit_replaces_the_packages_and_keeps_the_projects_own_edits() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, first) = fixture_repo("gib");
        let (project, saves) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (url, hub) = (repo.path().display().to_string(), empty_hub(saves.path()));
        assert_eq!(
            import_alone(project.path(), &url).await["registered"],
            false
        );
        registers_at(&hub, project.path(), &first).await;
        let (mine, second) = edit_both_sides(repo.path(), project.path());
        let done = import_alone(project.path(), &url).await;
        check_second(&done, project.path(), &mine, (&first, &second));
        // Registered again from the new checkout; the one before is kept, two back removed.
        registers_at(&hub, project.path(), &second).await;
        let imports = project.path().join(".shakedown/imports/toy");
        let part = hub.registry().get("gib-part").cloned().unwrap();
        assert!(part
            .dir
            .starts_with(imports.join(&second[..12]).canonicalize().unwrap()));
        assert!(imports.join(&first[..12]).is_dir());
        let third = commit_change(repo.path(), "worlds/box/data/seed.json", "{\"n\": 1}\n");
        import_alone(project.path(), &url).await;
        assert!(
            !imports.join(&first[..12]).exists(),
            "two commits back is removed"
        );
        assert!(imports.join(&second[..12]).is_dir() && imports.join(&third[..12]).is_dir());
    });
}

/// The repo drops its experiment, setup and scenario; the person made an experiment of their own
/// on the setup and edited the scenario.
fn drop_from_repo(repo: &Path, project: &Path) {
    let ours = "format: shakedown-experiment/1\nlabel: Ours\nworld: box\nscenario: poke\nsetups: [solo]\nrepeats: {per_setup: 1}\n";
    write(project, "experiments/ours.yaml", ours);
    let scenario = project.join("scenarios/poke.yaml");
    let mine = std::fs::read_to_string(&scenario)
        .unwrap()
        .replace("Poke it", "Poke it (ours)");
    std::fs::write(&scenario, mine).unwrap();
    let gone = [
        "experiments/try.yaml",
        "setups/solo.yaml",
        "scenarios/poke.yaml",
    ];
    git(repo, &[&["rm", "-q"][..], &gone[..]].concat());
    git(repo, &["commit", "-q", "-m", "drop"]);
}

/// The re-import after [`drop_from_repo`] removed the unchanged experiment, to the trash, and kept
/// the setup the person's experiment uses and the scenario they changed.
fn check_dropped(done: &Value, project: &Path) {
    let actions = rows(done, "items", &["action", "id"]);
    for want in ["same box", "remove try", "keep solo", "keep poke"] {
        assert!(
            actions.contains(&want.to_string()),
            "{want} not in {actions:?}"
        );
    }
    assert_eq!(rows(done, "removed", &["kind", "id"]), ["experiment try"]);
    assert!(!project.join("experiments/try.yaml").exists());
    assert!(
        project.join(".shakedown/trash").is_dir(),
        "a removal can be undone"
    );
    for kept in [
        "setups/solo.yaml",
        "scenarios/poke.yaml",
        "experiments/ours.yaml",
    ] {
        assert!(project.join(kept).is_file(), "{kept}");
    }
}

/// A re-import removes what the last import wrote and the repo no longer has, when it is unchanged
/// and nothing left uses it (to the trash); it keeps what the person changed or still uses, and
/// removes that once nothing does. The person's own items are never touched.
#[test]
fn a_reimport_removes_what_the_repo_dropped_unless_the_project_changed_or_uses_it() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("drop");
        let project = tempfile::tempdir().unwrap();
        let (url, p) = (repo.path().display().to_string(), project.path());
        import_alone(p, &url).await;
        drop_from_repo(repo.path(), p);
        check_dropped(&import_alone(p, &url).await, p);
        // Once the person's experiment is gone, nothing uses the setup: it goes too.
        std::fs::remove_file(p.join("experiments/ours.yaml")).unwrap();
        let done = import_alone(p, &url).await;
        assert_eq!(rows(&done, "removed", &["kind", "id"]), ["setup solo"]);
        assert!(
            p.join("scenarios/poke.yaml").is_file(),
            "the person's edit stays"
        );
    });
}

/// An item two imports of the project hold stays when one repo drops it: the other still has it.
#[test]
fn a_reimport_keeps_what_another_import_holds() {
    if !uv_found() {
        return;
    }
    run(async {
        let (repo, _) = fixture_repo("two");
        let project = tempfile::tempdir().unwrap();
        let (url, p) = (repo.path().display().to_string(), project.path());
        for id in ["one", "two"] {
            import_with(p, json!({"url": url, "id": id, "confirm": true})).await;
        }
        git(repo.path(), &["rm", "-q", "experiments/try.yaml"]);
        git(repo.path(), &["commit", "-q", "-m", "drop"]);
        let done = import_with(p, json!({"url": url, "id": "one", "confirm": true})).await;
        let kept = done["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "try");
        let why = kept.unwrap()["reason"].as_str().unwrap().to_string();
        assert!(why.contains("the import two has it too"), "{why}");
        assert!(p.join("experiments/try.yaml").is_file());
    });
}

/// The text of a `tools/call` of `project_import_git` with `arguments`.
async fn tool_text(ctx: &OpCtx<'_>, arguments: Value) -> String {
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                      "params": {"name": "project_import_git", "arguments": arguments}});
    let reply = mcp::handle(ctx, &mcp::Asker::Local, &call).await.unwrap();
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn the_import_is_an_mcp_tool_and_refuses_other_transports() {
    run(async {
        let project = tempfile::tempdir().unwrap();
        let state = ProjectState::new(Some(project.path().to_path_buf()));
        let ctx = OpCtx::project_only(Arc::new(state));
        let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        let tools = mcp::handle(&ctx, &mcp::Asker::Local, &list).await.unwrap();
        let names = rows(&tools["result"], "tools", &["name"]);
        assert!(names.iter().any(|n| n == "project_import_git"), "{names:?}");
        let text = tool_text(&ctx, json!({"url": "ext::sh -c touch% /tmp/x"})).await;
        assert!(text.contains("is not a git URL"), "{text}");
        let text = tool_text(&ctx, json!({"url": "/srv/x", "ref": "--upload-pack=sh"})).await;
        assert!(text.contains("is not a ref"), "{text}");
    });
}

/// A shaked that serves others (hosted) refuses the import before fetching anything: the repo's
/// code would run on the server, and hosted runs it only in a sandbox.
#[test]
fn a_hosted_shaked_refuses_the_import() {
    run(async {
        let project = tempfile::tempdir().unwrap();
        let hosted = shaked::layer::Policy {
            serves_others: true,
            runs_packages_here: false,
            ..shaked::layer::Policy::LOCAL
        };
        let spy = shaked::harness::SpyLayer::new().with_policy(hosted);
        let fx = AppFx::idle("gitimport", "hosted")
            .layer(Arc::new(spy))
            .project(project.path())
            .start()
            .await;
        let asked = json!({"url": "/srv/repo", "confirm": true});
        let (status, v) = fx.post("/api/project/import-git", asked).await;
        assert_eq!(status, 403, "{v}");
        assert!(v["error"].as_str().unwrap().contains("sandbox"), "{v}");
        assert!(!project.path().join(".shakedown").exists());
    });
}

/// `AIRLINE_WORLD_REPO`, else the airline repo beside the Shakedown checkouts.
fn airline_repo() -> PathBuf {
    std::env::var_os("AIRLINE_WORLD_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/alexsky2/MLSys/CopperTide/airline-world"))
}

/// The airline import found its benchmarks, parts and views.
fn check_airline(done: &Value) {
    assert_eq!(done["problems"], json!([]));
    assert_eq!(done["project"]["problems"], json!([]));
    let names = rows(done, "packages", &["kind", "name"]);
    let want = [
        "benchmark airline-hub-day",
        "benchmark airline-network-day",
        "integration airline-driver",
        "integration movement-control",
        "integration crew-tracking",
        "integration maintenance-control",
        "integration ops-approvals",
    ];
    for w in want {
        assert!(names.iter().any(|n| n == w), "{w} not in {names:?}");
    }
    let views = rows(done, "views", &["world", "file"]);
    assert_eq!(
        views,
        ["hub-day views/occ.yaml", "network-day views/occ.yaml"]
    );
}

/// The project the airline import made loads with no problems and holds its items.
fn check_airline_project(project: &Path) {
    let here = shakedown_project::Project::open(project).unwrap();
    assert_eq!(here.problems, vec![], "the imported project has problems");
    for kind in shakedown_project::format::Kind::ALL {
        assert!(here.list(kind).len() >= 2, "{kind:?}");
    }
}

/// Imports the airline repo into a fresh project: it loads with no problems, and its benchmark
/// packages, parts and views are found; its benchmark describes itself from the import's
/// environment. `cargo test -p shaked --test git_import -- --ignored live_ --nocapture`.
#[test]
#[ignore = "imports the airline repo (installs its Python environment with uv)"]
fn live_airline_world_imports() {
    run(async {
        let repo = airline_repo();
        assert!(
            repo.join("shakedown.yaml").is_file(),
            "no airline repo at {}",
            repo.display()
        );
        let project = tempfile::tempdir().unwrap();
        let input = json!({"url": repo.display().to_string(), "confirm": true});
        let done = import_with(project.path(), input).await;
        println!("{}", serde_json::to_string_pretty(&done).unwrap());
        check_airline(&done);
        check_airline_project(project.path());
        let saves = tempfile::tempdir().unwrap();
        let hub = empty_hub(saves.path());
        let errors = shaked::git_import::register(&hub, project.path()).await;
        assert_eq!(errors, Vec::<String>::new());
        let bench = hub.bench_describe("airline-hub-day").await.unwrap();
        let tasks: Vec<&str> = bench.tasks.iter().map(|t| t.id.as_str()).collect();
        println!("airline-hub-day tasks: {tasks:?}");
        assert!(!tasks.is_empty());
    });
}
