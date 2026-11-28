//! Model edits applied through the same Pg transaction as their history.

use shaked::store::{ItemKind, Scoped, Stale};
use shakedown_project::{
    format::{Kind, ProjectFile},
    store::MemStore,
    Id, ItemBody, Project, Row, Snapshot, TableKinds,
};
use std::sync::Arc;
use support::{pg, records, run};
use uuid::Uuid;

mod support;

async fn read(s: Scoped<'_>, id: Uuid) -> Project {
    let data = s.project_items(id).await.unwrap().unwrap();
    let rows = data
        .items
        .into_iter()
        .map(|i| Row {
            kind: match i.kind {
                ItemKind::Scenario => Kind::Scenario,
                ItemKind::Setup => Kind::Setup,
                ItemKind::Experiment => Kind::Experiment,
            },
            id: i.id,
            slug: i.slug,
            body: i.body,
            revision: i.revision,
        })
        .collect();
    Project::open_snapshot(Snapshot {
        revision: data.project.revision,
        tables: TableKinds::HOSTED,
        settings: Some(data.project.settings),
        rows,
        store: Arc::new(MemStore::default()),
    })
    .unwrap()
}

fn put(p: &mut Project, name: &str, label: &str) {
    let yaml = format!("format: shakedown-scenario/1\nlabel: {label}\nworld: mill\ntask: t\n");
    let body = ItemBody::parse(Kind::Scenario, "scenario", &yaml).unwrap();
    p.put(Kind::Scenario, &Id::parse(name).unwrap(), body)
        .unwrap();
}

async fn save(s: Scoped<'_>, id: Uuid, p: &mut Project) {
    s.apply_project_change(id, &p.take_changes().unwrap())
        .await
        .unwrap();
}

#[test]
fn independent_items_commit_and_stale_batch_rolls_back() {
    run(async {
        let (_, store, _clean) = pg::fresh_store("projchanges").await;
        let s = records(&store);
        let id = s.create_project(&ProjectFile::named("A")).await.unwrap().id;
        let mut first = read(s, id).await;
        put(&mut first, "a", "A");
        put(&mut first, "b", "B");
        save(s, id, &mut first).await;
        concurrent_edits(s, id).await;
        assert_eq!(s.project_changes(id, 100).await.unwrap().len(), 3);
    });
}

async fn concurrent_edits(s: Scoped<'_>, id: Uuid) {
    let mut a = read(s, id).await;
    let mut b = read(s, id).await;
    let mut stale = read(s, id).await;
    put(&mut a, "a", "A2");
    put(&mut b, "b", "B2");
    save(s, id, &mut a).await;
    save(s, id, &mut b).await;
    put(&mut stale, "new", "New");
    put(&mut stale, "a", "Stale");
    let error = s
        .apply_project_change(id, &stale.take_changes().unwrap())
        .await
        .unwrap_err();
    assert!(error.downcast_ref::<Stale>().is_some(), "{error:#}");
    assert_no_partial(s, id).await;
}

#[test]
fn table_export_import_and_delete_undo_round_trip() {
    run(async {
        let (_, store, _clean) = pg::fresh_store("projroundtrip").await;
        let s = records(&store);
        let id = s.create_project(&ProjectFile::named("A")).await.unwrap().id;
        let mut p = read(s, id).await;
        put(&mut p, "a", "A");
        save(s, id, &mut p).await;
        let mut p = read(s, id).await;
        let original_id = p.row_id(Kind::Scenario, &Id::parse("a").unwrap());
        let files = p.export().unwrap();
        let file_project = Project::open_in(Arc::new(MemStore::new(files.clone()))).unwrap();
        assert_eq!(p.list(Kind::Scenario), file_project.list(Kind::Scenario));
        p.delete(Kind::Scenario, &Id::parse("a").unwrap(), false, None)
            .unwrap();
        save(s, id, &mut p).await;
        undo_latest(s, id).await;
        let restored = read(s, id).await;
        assert_eq!(
            restored.row_id(Kind::Scenario, &Id::parse("a").unwrap()),
            original_id
        );
        assert_eq!(restored.export().unwrap(), files);
    });
}

async fn undo_latest(s: Scoped<'_>, id: Uuid) {
    let changes = s.project_changes(id, 1).await.unwrap();
    let c = &changes[0];
    let mut p = read(s, id).await;
    p.undo(shakedown_project::history::Recorded {
        id: c.id,
        label: &c.label,
        items: &c.items,
    })
    .unwrap();
    save(s, id, &mut p).await;
}

async fn assert_no_partial(s: Scoped<'_>, id: Uuid) {
    let current = read(s, id).await;
    assert!(current
        .get(Kind::Scenario, &Id::parse("new").unwrap())
        .is_none());
    assert_eq!(
        current
            .get(Kind::Scenario, &Id::parse("a").unwrap())
            .unwrap()
            .label,
        "A2"
    );
}
