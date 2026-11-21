//! shaked opens the project folder `SHAKEDOWN_PROJECT` names: edits made outside the app are
//! seen, and a broken file is reported without stopping the rest from loading.

mod support;

use shaked::project_state::NoProject;
use shakedown_project::format::Kind;
use support::{run, AppFx};

fn larkspur() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    support::copy_dir(&support::project_fixture(), dir.path());
    dir
}

#[test]
fn no_project_configured() {
    run(async {
        let fx = AppFx::idle("project", "none").start().await;
        let err = fx.state.project().unwrap_err();
        assert!(matches!(err, NoProject::NotConfigured), "{err}");
        assert!(err.to_string().contains("no project is open"));
    });
}

#[test]
fn opens_the_configured_folder() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("project", "open")
            .project(dir.path())
            .start()
            .await;
        let project = fx.state.project().unwrap();
        assert_eq!(project.config.name, "Larkspur Ops");
        assert_eq!(project.list(Kind::Setup).len(), 3);
    });
}

#[test]
fn broken_file_reported_others_load() {
    run(async {
        let dir = larkspur();
        std::fs::write(dir.path().join("scenarios/broken.yaml"), ":::\n").unwrap();
        let fx = AppFx::idle("project", "broken")
            .project(dir.path())
            .start()
            .await;
        let project = fx.state.project().unwrap();
        assert_eq!(project.list(Kind::World).len(), 1);
        assert_eq!(project.problems.len(), 1, "{:?}", project.problems);
    });
}

#[test]
fn outside_edits_are_seen() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("project", "edits")
            .project(dir.path())
            .start()
            .await;
        assert_eq!(fx.state.project().unwrap().list(Kind::Scenario).len(), 1);
        let second = dir.path().join("scenarios/second.yaml");
        let text =
            std::fs::read_to_string(dir.path().join("scenarios/supplier-slip.yaml")).unwrap();
        std::fs::write(second, text).unwrap();
        assert_eq!(fx.state.project().unwrap().list(Kind::Scenario).len(), 2);
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
        let err = fx.state.project().unwrap_err();
        assert!(err.to_string().contains("not a Shakedown project"), "{err}");
    });
}

#[test]
fn writes_go_through_the_project() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("project", "write")
            .project(dir.path())
            .start()
            .await;
        let id = shakedown_project::Id::parse("supplier-slip").unwrap();
        fx.state
            .with_project_mut(|p| p.remove(Kind::Scenario, &id))
            .unwrap()
            .unwrap();
        assert!(!dir.path().join("scenarios/supplier-slip.yaml").exists());
        assert!(fx.state.project().unwrap().list(Kind::Scenario).is_empty());
    });
}

/// A caller's project as a test layer files it: `<root>/person` for the person, `<root>/<token
/// id>` for a token.
fn caller_project(root: &std::path::Path, who: &str) -> std::path::PathBuf {
    let dir = root.join(who);
    support::copy_dir(&support::project_fixture(), &dir);
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
        assert_eq!(other.current().unwrap().list(Kind::World).len(), 1);
        assert_eq!(person.current().unwrap().list(Kind::World).len(), 0);
    });
}

#[test]
fn same_folder_shares_state() {
    run(async {
        let dir = larkspur();
        let fx = AppFx::idle("project", "shared")
            .project(dir.path())
            .start()
            .await;
        let a = fx.state.project_for(&shaked::auth::Caller::person());
        let b = fx.state.project_for(&shaked::auth::Caller::api_token(
            uuid::Uuid::new_v4(),
            vec![],
        ));
        assert!(std::sync::Arc::ptr_eq(&a, &b));
    });
}
