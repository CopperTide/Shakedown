//! A project in a database: read once per revision, changed in memory, saved in one go.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures_util::FutureExt;
use shakedown_project::format::Kind;
use shakedown_project::Id;

use super::*;
use crate::layer::BoxFuture;

/// A database of one project: its files, revision, and how often it was loaded and saved.
#[derive(Default)]
struct FakeDb {
    files: Mutex<(i64, Files)>,
    loads: AtomicUsize,
    saves: Mutex<Vec<Vec<Change>>>,
}

impl ProjectDb for FakeDb {
    fn key(&self) -> String {
        "one".into()
    }

    fn revision(&self) -> BoxFuture<'_, Result<i64, NoProject>> {
        async { Ok(self.files.lock().unwrap().0) }.boxed()
    }

    fn load(&self) -> BoxFuture<'_, Result<(i64, Files), NoProject>> {
        self.loads.fetch_add(1, Ordering::Relaxed);
        async { Ok(self.files.lock().unwrap().clone()) }.boxed()
    }

    fn save(&self, base: i64, changes: Vec<Change>) -> BoxFuture<'_, Result<i64, NoProject>> {
        async move {
            let mut files = self.files.lock().unwrap();
            if files.0 != base {
                let message = "the project changed meanwhile".into();
                let status = StatusCode::CONFLICT;
                return Err(NoProject::Refused { status, message });
            }
            for (path, bytes) in &changes {
                match bytes {
                    Some(b) => files.1.insert(path.clone(), b.clone()),
                    None => files.1.remove(path),
                };
            }
            files.0 += 1;
            self.saves.lock().unwrap().push(changes);
            Ok(files.0)
        }
        .boxed()
    }
}

fn state(db: &Arc<FakeDb>, projects: &Projects) -> Arc<ProjectState> {
    projects.get(Some(ProjectHome::Db(db.clone())))
}

const WORLD: &str = "format: shakedown-world/3\nlabel: Mill\n";

fn world() -> shakedown_project::ItemBody {
    shakedown_project::ItemBody::parse(Kind::World, "w", WORLD).unwrap()
}

#[tokio::test]
async fn init_put_and_read_save_once_each_and_load_once_per_revision() {
    let (db, projects) = (Arc::new(FakeDb::default()), Projects::new(None));
    let s = state(&db, &projects);
    assert!(matches!(s.current().await, Err(NoProject::Unreadable(_))));
    s.init("Plant", false).await.unwrap().unwrap();
    let mill = Id::parse("mill").unwrap();
    s.change(|p| p.put(Kind::World, &mill, world()))
        .await
        .unwrap()
        .unwrap();
    let saves = db.saves.lock().unwrap().clone();
    assert_eq!(saves.len(), 2);
    assert_eq!(saves[1].len(), 1, "{:?}", saves[1]);
    assert_eq!(saves[1][0].0, "worlds/mill/world.yaml");
    let loads = db.loads.load(Ordering::Relaxed);
    let again = state(&db, &projects);
    assert_eq!(again.current().await.unwrap().list(Kind::World).len(), 1);
    assert_eq!(s.current().await.unwrap().config.name, "Plant");
    assert_eq!(
        db.loads.load(Ordering::Relaxed),
        loads + 1,
        "one load, then the cache"
    );
}

#[tokio::test]
async fn a_failed_change_saves_nothing_and_a_stale_one_is_refused() {
    let (db, projects) = (Arc::new(FakeDb::default()), Projects::new(None));
    let s = state(&db, &projects);
    s.init("Plant", false).await.unwrap().unwrap();
    let missing = Id::parse("missing").unwrap();
    let failed = s.change(|p| p.remove(Kind::World, &missing)).await.unwrap();
    assert!(failed.is_err());
    assert_eq!(db.saves.lock().unwrap().len(), 1);
    let mill = Id::parse("mill").unwrap();
    let stale = s
        .change(|p| {
            db.files.lock().unwrap().0 += 1;
            p.put(Kind::World, &mill, world())
        })
        .await;
    let Err(NoProject::Refused { status, .. }) = stale else {
        panic!("{stale:?}");
    };
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_read_copy_refuses_writes() {
    let (db, projects) = (Arc::new(FakeDb::default()), Projects::new(None));
    let s = state(&db, &projects);
    s.init("Plant", false).await.unwrap().unwrap();
    let copy = s.current().await.unwrap();
    assert!(copy.write_trash_note("1", "runs.json", "[]").is_err());
    assert!(copy.root().is_none() && s.root().is_none());
}

#[tokio::test]
async fn git_on_a_database_is_400_and_saves_nothing() {
    let (db, projects) = (Arc::new(FakeDb::default()), Projects::new(None));
    let Err(e) = state(&db, &projects).init("Plant", true).await.unwrap() else {
        panic!("git on a project kept in a database");
    };
    assert_eq!(crate::ops::OpError::from(e).status, StatusCode::BAD_REQUEST);
    assert!(db.saves.lock().unwrap().is_empty());
}

#[tokio::test]
async fn no_project_in_a_database_says_it_is_no_folder() {
    let (db, projects) = (Arc::new(FakeDb::default()), Projects::new(None));
    let ctx = crate::ops::OpCtx {
        project: state(&db, &projects),
        app: None,
        caller: Arc::new(crate::auth::Caller::person()),
    };
    let op = crate::ops::find("project.status").unwrap();
    let err = (op.run)(&ctx, op, serde_json::json!({})).await.unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(err.details.unwrap()["folder"], false);
}
