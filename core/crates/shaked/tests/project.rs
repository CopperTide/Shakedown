//! shaked opens the project folder `SHAKEDOWN_PROJECT` names: edits made outside the app are
//! seen, and a broken file is reported without stopping the rest from loading.

mod support;

use serde_json::json;
use shaked::project_state::NoProject;
use shakedown_project::format::Kind;
use support::project::{project_app, project_copy};
use support::{packages::toy_world, run, AppFx};

#[test]
fn no_project_configured() {
    run(async {
        let fx = AppFx::idle("project", "none").start().await;
        let err = fx.state.project().await.unwrap_err();
        assert!(matches!(err, NoProject::NotConfigured), "{err}");
        assert!(err.to_string().contains("no project is open"));
    });
}

#[test]
fn opens_the_configured_folder() {
    run(async {
        let fx = project_app("project", "open").await;
        let project = fx.state.project().await.unwrap();
        assert_eq!(project.config.name, "Larkspur Ops");
        assert_eq!(project.list(Kind::Setup).len(), 3);
    });
}

#[test]
fn broken_file_reported_others_load() {
    run(async {
        let dir = project_copy();
        std::fs::write(dir.path().join("scenarios/broken.yaml"), ":::\n").unwrap();
        let fx = AppFx::idle("project", "broken")
            .project(dir.path())
            .start()
            .await;
        let project = fx.state.project().await.unwrap();
        assert_eq!(project.list(Kind::World).len(), 1);
        assert_eq!(project.problems.len(), 1, "{:?}", project.problems);
    });
}

#[test]
fn outside_edits_are_seen() {
    run(async {
        let fx = project_app("project", "edits").await;
        assert_eq!(
            fx.state.project().await.unwrap().list(Kind::Scenario).len(),
            1
        );
        let second = fx.dir.path().join("scenarios/second.yaml");
        let text =
            std::fs::read_to_string(fx.dir.path().join("scenarios/supplier-slip.yaml")).unwrap();
        std::fs::write(second, text).unwrap();
        assert_eq!(
            fx.state.project().await.unwrap().list(Kind::Scenario).len(),
            2
        );
    });
}

#[test]
fn not_a_project_is_reported() {
    run(async {
        let dir = tempfile::tempdir().unwrap();
        let fx = AppFx::idle("project", "empty")
            .project(dir.path())
            .start()
            .await;
        let err = fx.state.project().await.unwrap_err();
        assert!(err.to_string().contains("not a Shakedown project"), "{err}");
    });
}

#[test]
fn writes_go_through_the_project() {
    run(async {
        let fx = project_app("project", "write").await;
        let id = shakedown_project::Id::parse("supplier-slip").unwrap();
        fx.state
            .with_project_mut(|p| p.remove(Kind::Scenario, &id))
            .await
            .unwrap()
            .unwrap();
        assert!(!fx.dir.path().join("scenarios/supplier-slip.yaml").exists());
        assert!(fx
            .state
            .project()
            .await
            .unwrap()
            .list(Kind::Scenario)
            .is_empty());
    });
}

/// A caller's project as a test layer files it: `<root>/person` for the person, `<root>/<token
/// id>` for a token.
fn caller_project(root: &std::path::Path, who: &str) -> std::path::PathBuf {
    let dir = root.join(who);
    support::project::copy_dir(&support::project::project_fixture(), &dir);
    dir
}

#[test]
fn layer_picks_the_callers_folder() {
    run(async {
        let root = tempfile::tempdir().unwrap();
        let token = uuid::Uuid::new_v4();
        let mine = caller_project(root.path(), "person");
        caller_project(root.path(), &token.to_string());
        std::fs::remove_dir_all(mine.join("worlds/larkspur-plant")).unwrap();
        let spy = shaked::harness::SpyLayer::new().projects_under(root.path().to_path_buf());
        let fx = AppFx::idle("project", "per-caller")
            .layer(std::sync::Arc::new(spy))
            .start()
            .await;
        let person = fx.state.project_for(&shaked::auth::Caller::person());
        let theirs = shaked::auth::Caller::api_token(token, vec![]);
        let other = fx.state.project_for(&theirs);
        assert_eq!(other.current().await.unwrap().list(Kind::World).len(), 1);
        assert_eq!(person.current().await.unwrap().list(Kind::World).len(), 0);
    });
}

#[test]
fn same_folder_shares_state() {
    run(async {
        let fx = project_app("project", "shared").await;
        let a = fx.state.project_for(&shaked::auth::Caller::person());
        let b = fx.state.project_for(&shaked::auth::Caller::api_token(
            uuid::Uuid::new_v4(),
            vec![],
        ));
        let id = shakedown_project::Id::parse("supplier-slip").unwrap();
        let removed = a.change(|p| p.remove(Kind::Scenario, &id)).await;
        removed.unwrap().unwrap();
        let seen = b.current().await.unwrap();
        assert!(
            seen.list(Kind::Scenario).is_empty(),
            "the other caller sees it"
        );
    });
}

/// Larkspur with its plant world run by the toy benchmark, and a world `bare` with no package.
fn larkspur_with_packages() -> tempfile::TempDir {
    let dir = project_copy();
    let worlds = dir.path().join("worlds");
    let plant = worlds.join("larkspur-plant/world.yaml");
    let text = std::fs::read_to_string(&plant).unwrap();
    std::fs::write(&plant, format!("{text}package: toybench\n")).unwrap();
    std::fs::create_dir_all(worlds.join("bare")).unwrap();
    let bare = "format: shakedown-world/3\nlabel: Bare\n";
    std::fs::write(worlds.join("bare/world.yaml"), bare).unwrap();
    dir
}

/// A world's tasks are its package's, asked for by the package's name or by the project world
/// that names it, each with its group.
#[test]
fn a_worlds_tasks_are_listed_by_its_package() {
    run(async {
        let dir = larkspur_with_packages();
        let bench = ["--scored", "--two-tasks", "--group", "test"];
        let fx = AppFx::idle("project", "tasks")
            .root(toy_world("project-tasks", &[], &bench))
            .bench("toybench")
            .project(dir.path())
            .start()
            .await;
        let (status, by_world) = fx.get("/api/benchmark?world=larkspur-plant").await;
        assert_eq!(status, 200, "{by_world}");
        let tasks = json!([{"id": "t1", "reason": "poke it", "group": "test"},
                           {"id": "t2", "reason": "poke it twice"}]);
        assert_eq!(
            (&by_world["name"], &by_world["tasks"]),
            (&json!("toybench"), &tasks)
        );
        let (_, by_name) = fx.get("/api/benchmark?name=toybench").await;
        assert_eq!(by_name["tasks"], tasks);
        let (_, listed) = fx.get("/api/tasks").await;
        assert_eq!(listed[0]["group"], "test");
        assert_unknown_benchmarks_are_404s(&fx).await;
    });
}

/// An unknown package or world, and a world without a package, are 404s that say why.
async fn assert_unknown_benchmarks_are_404s(fx: &AppFx) {
    let no_package =
        "Bare has no runtime yet: set package: to an installed benchmark or built world";
    for (path, why) in [
        ("name=nope", "no benchmark named nope is installed"),
        ("world=nope", "no world named nope"),
        ("world=bare", no_package),
    ] {
        let (status, v) = fx.get(&format!("/api/benchmark?{path}")).await;
        assert_eq!((status, v["error"].as_str()), (404, Some(why)), "{path}");
    }
}
