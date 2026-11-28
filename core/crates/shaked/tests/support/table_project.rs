//! A typed Pg home with transactional world files for operation parity tests.
use shaked::{
    auth::Caller,
    layer::{BoxFuture, NoProject, ProjectDb},
    store::{ItemKind, Store},
};
use shakedown_project::{
    format::Kind,
    store::{Change, Files, MemStore},
    ChangeSet, Row, Snapshot, TableKinds,
};
use std::sync::Arc;
use uuid::Uuid;

pub struct Home {
    pub store: Arc<Store>,
    pub id: Uuid,
}

fn failed(error: anyhow::Error) -> NoProject {
    if error.is::<shaked::store::Stale>() || error.is::<shaked::store::SlugTaken>() {
        return NoProject::Refused {
            status: axum::http::StatusCode::CONFLICT,
            message: error.to_string(),
        };
    }
    NoProject::Failed(error)
}

impl ProjectDb for Home {
    fn key(&self) -> String {
        self.id.to_string()
    }
    fn table_kinds(&self) -> TableKinds {
        TableKinds::HOSTED
    }
    fn revision(&self) -> BoxFuture<'_, Result<i64, NoProject>> {
        Box::pin(async {
            let p = self
                .store
                .at(&Caller::person())
                .project_items(self.id)
                .await
                .map_err(failed)?;
            Ok(p.expect("test project exists").project.revision)
        })
    }
    fn load(&self) -> BoxFuture<'_, Result<(i64, Files), NoProject>> {
        Box::pin(async { Ok((self.revision().await?, Files::new())) })
    }
    fn save(&self, _: i64, _: Vec<Change>) -> BoxFuture<'_, Result<i64, NoProject>> {
        Box::pin(async { panic!("table homes apply a change set") })
    }
    fn snapshot(&self) -> BoxFuture<'_, Result<Snapshot, NoProject>> {
        Box::pin(async {
            let tx = self
                .store
                .snapshot_tx(&Caller::person())
                .await
                .map_err(failed)?;
            let p = shaked::store::project_items_in(&tx, self.id)
                .await
                .map_err(failed)?
                .expect("test project exists");
            let rows = p.items.into_iter().map(model_row).collect();
            let files = world_files(&tx, self.id).await.map_err(failed)?;
            tx.commit().await.map_err(failed)?;
            Ok(Snapshot {
                revision: p.project.revision,
                tables: TableKinds::HOSTED,
                settings: Some(p.project.settings),
                rows,
                store: Arc::new(MemStore::read_only(files)),
            })
        })
    }
    fn apply(&self, change: ChangeSet) -> BoxFuture<'_, Result<(), NoProject>> {
        Box::pin(async move { self.apply_in(change).await.map_err(failed) })
    }
    fn history(
        &self,
        limit: i64,
    ) -> BoxFuture<'_, Result<Vec<shaked::store::ChangeRow>, NoProject>> {
        Box::pin(async move {
            self.store
                .at(&Caller::person())
                .project_changes(self.id, limit)
                .await
                .map_err(failed)
        })
    }
}

impl Home {
    async fn apply_in(&self, change: ChangeSet) -> anyhow::Result<()> {
        let tx = self.store.tx(&Caller::person()).await?;
        shaked::store::apply_project_change_in(&tx, self.id, &change).await?;
        for file in change.files {
            let row = tx
                .query_opt(
                    "SELECT body FROM test_project_files WHERE project_id = $1 AND path = $2",
                    &[&self.id, &file.path],
                )
                .await?;
            anyhow::ensure!(
                row.map(|r| r.get::<_, Vec<u8>>(0)) == file.before,
                "file changed"
            );
            match file.after {
                Some(bytes) => {
                    tx.execute("INSERT INTO test_project_files VALUES ($1, $2, $3) ON CONFLICT (project_id, path) DO UPDATE SET body = $3", &[&self.id, &file.path, &bytes]).await?;
                }
                None => {
                    tx.execute(
                        "DELETE FROM test_project_files WHERE project_id = $1 AND path = $2",
                        &[&self.id, &file.path],
                    )
                    .await?;
                }
            }
        }
        tx.commit().await
    }
}

fn model_row(i: shaked::store::ProjectItem) -> Row {
    Row {
        id: i.id,
        slug: i.slug,
        body: i.body,
        revision: i.revision,
        kind: match i.kind {
            ItemKind::Scenario => Kind::Scenario,
            ItemKind::Setup => Kind::Setup,
            ItemKind::Experiment => Kind::Experiment,
        },
    }
}

async fn world_files(tx: &shaked::store::Tx, id: Uuid) -> anyhow::Result<Files> {
    let mut files = Files::new();
    for r in tx
        .query(
            "SELECT path, body FROM test_project_files WHERE project_id = $1",
            &[&id],
        )
        .await?
    {
        files.insert(r.get(0), r.get::<_, Vec<u8>>(1).into());
    }
    Ok(files)
}
