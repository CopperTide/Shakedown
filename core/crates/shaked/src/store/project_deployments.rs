//! Deployment links use design identity, so equal slugs in different projects never collide.
use super::Scoped;
use anyhow::Result;
use uuid::Uuid;

impl Scoped<'_> {
    pub async fn deployments_of_row(&self, row: Option<Uuid>) -> Result<Vec<String>> {
        let rows = self.query("SELECT id FROM experiments WHERE project_experiment_id=$1 ORDER BY created_at DESC, id", &[&row]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }
    pub async fn unlink_deployments_of_row(&self, row: Uuid) -> Result<u64> {
        self.execute("UPDATE experiments SET project_experiment=NULL, project_experiment_id=NULL WHERE project_experiment_id=$1", &[&row]).await
    }
    pub async fn link_deployment_to_row(&self, id: &str, slug: &str, row: Uuid) -> Result<bool> {
        Ok(self.execute("UPDATE experiments SET project_experiment=$2, project_experiment_id=$3 WHERE id=$1 AND project_experiment IS NULL", &[&id,&slug,&row]).await? == 1)
    }
}
