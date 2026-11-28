//! Equal experiment slugs in different projects must not share deployments.
mod support;
use shaked::store::{ItemKind, ItemWrite, Scoped};
use shakedown_project::format::ProjectFile;
use support::{pg, records, run};
use uuid::Uuid;

async fn design(s: Scoped<'_>, name: &str) -> Uuid {
    let p = s.create_project(&ProjectFile::named(name)).await.unwrap();
    s.put_project_item(
        p.id,
        ItemWrite {
            kind: ItemKind::Experiment,
            slug: "same",
            body: &serde_json::json!({}),
            base: None,
        },
    )
    .await
    .unwrap()
    .id
}

#[test]
fn deployments_follow_row_identity() {
    run(async {
        let (_, store, _clean) = pg::fresh_store("projlinks").await;
        let s = records(&store);
        let a = design(s, "A").await;
        let b = design(s, "B").await;
        let tx = store.tx(&shaked::auth::Caller::person()).await.unwrap();
        for (id, row) in [("a", a), ("b", b)] {
            tx.execute("INSERT INTO experiments (id,name,task_id,benchmark,runs,project_experiment,project_experiment_id) VALUES ($1,'Test','t','b',1,'same',$2)", &[&id,&row]).await.unwrap();
        }
        tx.commit().await.unwrap();
        assert_eq!(s.deployments_of_row(Some(a)).await.unwrap(), ["a"]);
        s.unlink_deployments_of_row(a).await.unwrap();
        assert_eq!(s.deployments_of_row(Some(b)).await.unwrap(), ["b"]);
        assert!(s.link_deployment_to_row("a", "same", a).await.unwrap());
        assert!(!s.link_deployment_to_row("b", "same", a).await.unwrap());
        assert_eq!(s.deployments_of_row(Some(a)).await.unwrap(), ["a"]);
    });
}
