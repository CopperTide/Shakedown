//! The same operations against an ordinary folder and typed Pg home.
mod support;
#[path = "support/table_project.rs"]
mod table_project;

use serde_json::{json, Value};
use shaked::{
    layer::ProjectHome,
    ops::{self, OpCtx},
    project_state::{ProjectState, Projects},
};
use shakedown_project::format::ProjectFile;
use std::sync::Arc;
use support::{pg, records, run};

async fn call(state: Arc<ProjectState>, name: &str, args: Value) -> Value {
    let op = ops::find(name).unwrap();
    (op.run)(&OpCtx::project_only(state), op, args)
        .await
        .unwrap()
}

async fn exercise(state: Arc<ProjectState>) -> Value {
    seed(state.clone()).await;
    seed_design(state.clone()).await;
    setup_file(state.clone()).await;
    let exported = call(state.clone(), "project.export", json!({})).await;
    import_same(state.clone(), &exported).await;
    let removed = call(
        state.clone(),
        "scenarios.delete",
        json!({"id":"rush", "confirm":true, "cascade":true}),
    )
    .await;
    call(
        state.clone(),
        "project.restore",
        json!({"trash":removed["trash"], "confirm":true}),
    )
    .await;
    assert_eq!(
        call(state.clone(), "project.export", json!({})).await,
        exported
    );
    call(state, "scenarios.list", json!({})).await
}

#[test]
fn file_and_table_operations_agree() {
    run(async {
        let (_, store, _clean) = pg::fresh_store("tableops").await;
        let s = records(&store);
        let tx = store.tx(&shaked::auth::Caller::person()).await.unwrap();
        tx.execute("CREATE TABLE test_project_files (project_id uuid, path text, body bytea, PRIMARY KEY(project_id, path))", &[]).await.unwrap();
        tx.commit().await.unwrap();
        let id = s.create_project(&ProjectFile::named("A")).await.unwrap().id;
        let home = table_project::Home {
            store: Arc::new(store),
            id,
        };
        let table = Projects::new(None).get(Some(ProjectHome::Db(Arc::new(home))));
        let dir = tempfile::tempdir().unwrap();
        shakedown_project::Project::init(dir.path(), "A", false).unwrap();
        let folder = Arc::new(ProjectState::new(Some(dir.path().into())));
        assert_eq!(exercise(table).await, exercise(folder).await);
    });
}

async fn seed(state: Arc<ProjectState>) {
    let world = json!({"format":"shakedown-world/3", "label":"Mill"});
    call(
        state.clone(),
        "worlds.put",
        json!({"id":"mill", "body":world, "confirm":true}),
    )
    .await;
    let body =
        json!({"format":"shakedown-scenario/1", "label":"Rush", "world":"mill", "task":"Ship"});
    call(
        state.clone(),
        "scenarios.put",
        json!({"id":"rush", "body":body, "confirm":true}),
    )
    .await;
}

async fn seed_design(state: Arc<ProjectState>) {
    let setup = json!({"format":"shakedown-setup/1", "label":"Agent", "team":{"kind":"http", "url":"https://example.invalid/agent"}});
    call(
        state.clone(),
        "setups.put",
        json!({"id":"agent", "body":setup, "confirm":true}),
    )
    .await;
    let experiment = json!({"format":"shakedown-experiment/1", "label":"Compare", "world":"mill", "scenario":"rush", "setups":["agent"], "repeats":{"per_setup":1}});
    call(
        state,
        "experiments.put",
        json!({"id":"compare", "body":experiment, "confirm":true}),
    )
    .await;
}

async fn import_same(state: Arc<ProjectState>, exported: &Value) {
    let preview = call(
        state.clone(),
        "project.import_files",
        json!({"files": exported["files"]}),
    )
    .await;
    assert_eq!(preview["proposed"], true);
    assert_eq!(preview["items"], json!([]));
    let imported = call(
        state,
        "project.import_files",
        json!({"files": exported["files"], "confirm": true}),
    )
    .await;
    assert_eq!(imported["proposed"], false);
    assert_eq!(imported["problems"], json!([]));
}

async fn setup_file(state: Arc<ProjectState>) {
    let id = shakedown_project::Id::parse("agent").unwrap();
    let before = state
        .current()
        .await
        .unwrap()
        .revision(shakedown_project::format::Kind::Setup, &id);
    call(
        state.clone(),
        "project.import_files",
        json!({"files":{"setups/agent/notes.txt":"dGVzdA=="}, "confirm":true}),
    )
    .await;
    let after = state
        .current()
        .await
        .unwrap()
        .revision(shakedown_project::format::Kind::Setup, &id);
    assert_eq!(after, before.map(|r| r + 1));
}
