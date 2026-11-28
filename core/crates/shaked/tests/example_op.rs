//! `project.example` against a folder and a typed Pg home (hosted's: scenarios, setups and
//! experiments in tables, worlds as file rows): the same four items, then 409 on a second call.
mod support;
#[path = "support/table_project.rs"]
mod table_project;

use axum::http::StatusCode;
use serde_json::{json, Value};
use shaked::{
    layer::ProjectHome,
    ops::{self, OpCtx},
    project_state::{ProjectState, Projects},
};
use shakedown_project::format::ProjectFile;
use std::sync::Arc;
use support::{pg, records, run};

async fn call(state: &Arc<ProjectState>, name: &str, args: Value) -> Result<Value, ops::OpError> {
    let op = ops::find(name).unwrap();
    (op.run)(&OpCtx::project_only(state.clone()), op, args).await
}

/// Adds the example twice: what the first wrote and is listed, and the second's status.
async fn twice(state: Arc<ProjectState>) -> (Value, Vec<Value>, StatusCode) {
    let args = json!({"key": "my-key", "model": "some/model", "confirm": true});
    let written = call(&state, "project.example", args.clone()).await.unwrap();
    let mut listed = vec![];
    for kind in ["worlds", "scenarios", "setups", "experiments"] {
        let items = call(&state, &format!("{kind}.list"), json!({}))
            .await
            .unwrap();
        listed.push(items["items"][0]["id"].clone());
        assert_eq!(items["items"][0]["problems"], json!([]), "{items}");
    }
    let again = call(&state, "project.example", args).await.unwrap_err();
    (written["written"].clone(), listed, again.status)
}

#[test]
fn the_example_is_the_same_in_a_folder_and_in_tables_and_never_overwrites() {
    run(async {
        let (_, store, _clean) = pg::fresh_store("exampleop").await;
        let tx = store.tx(&shaked::auth::Caller::person()).await.unwrap();
        tx.execute("CREATE TABLE test_project_files (project_id uuid, path text, body bytea, PRIMARY KEY(project_id, path))", &[]).await.unwrap();
        tx.commit().await.unwrap();
        let id = records(&store)
            .create_project(&ProjectFile::named("A"))
            .await
            .unwrap()
            .id;
        let home = table_project::Home {
            store: Arc::new(store),
            id,
        };
        let table = Projects::new(None).get(Some(ProjectHome::Db(Arc::new(home))));
        let dir = tempfile::tempdir().unwrap();
        shakedown_project::Project::init(dir.path(), "A", false).unwrap();
        let folder = Arc::new(ProjectState::new(Some(dir.path().into())));
        let (in_table, in_folder) = (twice(table).await, twice(folder).await);
        assert_eq!(in_table, in_folder);
        assert_eq!(in_table.1.len(), 4);
        assert_eq!(in_table.2, StatusCode::CONFLICT);
    });
}
