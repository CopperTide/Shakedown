//! Mixed file/table homes and import planning without a database dependency.
use shakedown_project::{
    format::{Kind, ProjectFile},
    store::{MemStore, Store},
    Id, ItemBody, Project, Snapshot, TableKinds,
};
use std::sync::Arc;

fn mixed() -> (Project, Arc<MemStore>) {
    let store = Arc::new(MemStore::default());
    let p = Project::open_snapshot(Snapshot {
        revision: 1,
        tables: TableKinds::LOCAL,
        settings: Some(ProjectFile::named("Local")),
        rows: Vec::new(),
        store: store.clone(),
    })
    .unwrap();
    (p, store)
}

#[test]
fn local_scenarios_stay_files_while_experiments_are_rows() {
    let (mut p, files) = mixed();
    let scenario = ItemBody::parse(
        Kind::Scenario,
        "s",
        "format: shakedown-scenario/1\nlabel: S\nworld: w\ntask: T",
    )
    .unwrap();
    p.put(Kind::Scenario, &Id::parse("s").unwrap(), scenario)
        .unwrap();
    let experiment = ItemBody::parse(
        Kind::Experiment,
        "e",
        "format: shakedown-experiment/1\nlabel: E\nworld: w\nscenario: s\nsetups: [a]\nrepeats: {per_setup: 1}",
    )
    .unwrap();
    p.put(Kind::Experiment, &Id::parse("e").unwrap(), experiment)
        .unwrap();
    assert!(files.exists("scenarios/s.yaml"));
    assert!(!files.exists("experiments/e.yaml"));
    assert!(!files.exists("shakedown.yaml"));
    let changes = p.take_changes().unwrap();
    assert_eq!(changes.items.len(), 1);
    assert_eq!(changes.files.len(), 1);
}

#[test]
fn export_excludes_internal_files_and_import_is_idempotent() {
    let (mut p, files) = mixed();
    files.write("worlds/w/data.bin", &[0, 255, 4]).unwrap();
    files.write(".git/config", b"private remote").unwrap();
    files.write(".shakedown/trash/a", b"old").unwrap();
    let exported = p.export().unwrap();
    assert_eq!(&*exported["worlds/w/data.bin"], &[0, 255, 4]);
    assert!(!exported.keys().any(|p| p.starts_with('.')));
    let plan = p.plan_import(&exported);
    assert!(plan.items.is_empty() && plan.files.is_empty() && plan.settings.is_none());
    p.import_files(&plan).unwrap();
    assert!(p.take_changes().is_none());
}
