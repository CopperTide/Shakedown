//! Changes over a table home's snapshot; file writes remain staged until apply succeeds.

use super::{NoProject, ProjectDb, ProjectHome, ProjectState};
use shakedown_project::store::Overlay;
use shakedown_project::{Project, ProjectError};
use std::sync::Arc;

impl ProjectState {
    pub(super) async fn table_change<T>(
        &self,
        db: &dyn ProjectDb,
        change: impl FnOnce(&mut Project) -> Result<T, ProjectError>,
    ) -> Result<Result<T, ProjectError>, NoProject> {
        let mut snapshot = db.snapshot().await?;
        let base = snapshot.store.clone();
        let overlay = Arc::new(Overlay::new(snapshot.store));
        snapshot.store = overlay.clone();
        let mut project = Project::open_snapshot(snapshot)?;
        let result = change(&mut project);
        if result.is_ok() {
            let mut change = project
                .take_changes()
                .unwrap_or_else(|| shakedown_project::ChangeSet::new("edit files".into()));
            change.files = staged_files(&*base, &overlay)?;
            project.track_setup_files(&mut change)?;
            if !change.is_empty() {
                db.apply(change).await?;
            }
        }
        self.forget();
        Ok(result)
    }

    /// Table history, newest first. Legacy file homes retain their trash interface.
    pub async fn history(&self, limit: i64) -> Result<Vec<crate::store::ChangeRow>, NoProject> {
        match &self.home {
            Some(ProjectHome::Db(db)) => db.history(limit.clamp(1, 100)).await,
            _ => Ok(Vec::new()),
        }
    }
    pub async fn note(&self, change: &str, name: &str, text: &str) -> Result<(), NoProject> {
        if let Some(ProjectHome::Db(db)) = &self.home {
            if !db.table_kinds().is_none() {
                return db.note(change, name, text).await;
            }
        }
        self.change(|p| p.write_trash_note(change, name, text))
            .await??;
        Ok(())
    }
}

fn staged_files(
    base: &dyn shakedown_project::store::Store,
    overlay: &Overlay,
) -> Result<Vec<shakedown_project::FileChange>, NoProject> {
    overlay
        .changes()
        .into_iter()
        .map(|(path, after)| {
            let before = match base.read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(NoProject::Failed(e.into())),
            };
            Ok(shakedown_project::FileChange {
                path,
                before,
                after: after.map(|b| b.to_vec()),
            })
        })
        .collect()
}
