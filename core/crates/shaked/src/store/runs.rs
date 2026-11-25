//! Runs and their branches: a run's row, each branch's team, status and restore, and the
//! summaries the API lists.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio_postgres::Row;

use super::{new_id, BranchId, RunId, Scoped, BRANCHES_OF_RUNS};
use crate::model::*;

const RUN_COLS: &str = "id, name, task_id, created_at";
/// A branch's columns, of `b`.
const BRANCH_COLS: &str =
    "b.id, b.run_id, b.parent_id, b.start_seq, b.team, b.changes, b.fork_note, \
     b.status, b.error, b.created_at, b.ended_at, b.restore_mode, b.restored";

impl Scoped<'_> {
    pub async fn create_run(&self, task_id: &str, team: &Team) -> Result<RunId> {
        let id = new_id();
        self.execute(
            "INSERT INTO runs (id, task_id, team) VALUES ($1, $2, $3)",
            &[&id, &task_id, &serde_json::to_value(team)?],
        )
        .await?;
        Ok(id)
    }

    /// Sets a run's display name without changing its task or recorded events.
    pub async fn name_run(&self, id: &str, name: &str) -> Result<bool> {
        Ok(self
            .execute("UPDATE runs SET name = $2 WHERE id = $1", &[&id, &name])
            .await?
            == 1)
    }

    /// `team` is the branch's effective team (the run's team with `changes` applied).
    pub async fn create_branch(
        &self,
        run_id: &str,
        parent: Option<(&str, i64)>,
        team: &Team,
        changes: &Value,
        fork_note: Option<String>,
    ) -> Result<BranchId> {
        let id = new_id();
        let (parent_id, start_seq) = match parent {
            Some((p, s)) => (Some(p.to_string()), Some(s)),
            None => (None, None),
        };
        self.execute(
            "INSERT INTO branches (id, run_id, parent_id, start_seq, team, changes, fork_note)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &id,
                &run_id,
                &parent_id,
                &start_seq,
                &serde_json::to_value(team)?,
                changes,
                &fork_note,
            ],
        )
        .await?;
        Ok(id)
    }

    /// Replaces a branch's team (an external system registered another agent).
    pub async fn set_team(&self, branch_id: &str, team: &Team) -> Result<()> {
        self.execute(
            "UPDATE branches SET team = $2 WHERE id = $1",
            &[&branch_id, &serde_json::to_value(team)?],
        )
        .await?;
        Ok(())
    }

    /// Records how a fork's world was restored: the mode asked for and each part's restore.
    pub async fn set_restore(
        &self,
        branch_id: &str,
        mode: RestoreMode,
        restored: &[RestoredPart],
    ) -> Result<()> {
        self.execute(
            "UPDATE branches SET restore_mode = $2, restored = $3 WHERE id = $1",
            &[
                &branch_id,
                &serde_json::to_value(mode)?.as_str(),
                &serde_json::to_value(restored)?,
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn set_status(
        &self,
        branch_id: &str,
        status: BranchStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.execute(
            "UPDATE branches SET status = $2, error = $3,
                 ended_at = CASE WHEN $2 = 'running' THEN NULL ELSE now() END WHERE id = $1",
            &[&branch_id, &status.as_str(), &error],
        )
        .await?;
        Ok(())
    }

    /// Turns branch `id` from status `from` to `to` (ended now unless `to` goes on): whether
    /// it was `from`.
    pub async fn turn_status(
        &self,
        id: &str,
        (from, to): (BranchStatus, BranchStatus),
    ) -> Result<bool> {
        let sql = "UPDATE branches SET status = $3,
                 ended_at = CASE WHEN $4 THEN NULL ELSE now() END WHERE id = $1 AND status = $2";
        let (from, to, goes_on) = (from.as_str(), to.as_str(), to.goes_on());
        Ok(self.execute(sql, &[&id, &from, &to, &goes_on]).await? == 1)
    }

    pub async fn branch(&self, id: &str) -> Result<Branch> {
        self.find_branch(id)
            .await?
            .ok_or_else(|| anyhow!("no such branch: {id}"))
    }

    /// Like `branch`, but a missing branch is `Ok(None)` rather than an error.
    pub async fn find_branch(&self, id: &str) -> Result<Option<Branch>> {
        let sql = format!("SELECT {BRANCH_COLS} FROM {BRANCHES_OF_RUNS} WHERE b.id = $1");
        let row = self.query_opt(&sql, &[&id]).await?;
        row.as_ref().map(branch_from).transpose()
    }

    pub async fn runs(&self) -> Result<Vec<RunSummary>> {
        let sql = format!("SELECT {RUN_COLS} FROM runs ORDER BY created_at DESC, id");
        let runs = self.query(&sql, &[]).await?;
        let mut by_run = self.branches_by_run().await?;
        Ok(runs
            .iter()
            .map(|r| summary_from(r, by_run.remove(r.get::<_, &str>("id")).unwrap_or_default()))
            .collect())
    }

    /// The run `id` with its branches, oldest first; None when there is no such run.
    pub async fn run(&self, id: &str) -> Result<Option<RunSummary>> {
        let sql = format!("SELECT {RUN_COLS} FROM runs WHERE id = $1");
        let Some(r) = self.query_opt(&sql, &[&id]).await? else {
            return Ok(None);
        };
        let sql = format!(
            "SELECT {BRANCH_COLS} FROM branches b WHERE b.run_id = $1 ORDER BY b.created_at, b.id"
        );
        let rows = self.query(&sql, &[&id]).await?;
        let branches = rows.iter().map(branch_from).collect::<Result<_>>()?;
        Ok(Some(summary_from(&r, branches)))
    }

    /// Every branch, grouped by its run, oldest first.
    async fn branches_by_run(&self) -> Result<HashMap<String, Vec<Branch>>> {
        let sql =
            format!("SELECT {BRANCH_COLS} FROM {BRANCHES_OF_RUNS} ORDER BY b.created_at, b.id");
        let mut by_run: HashMap<String, Vec<Branch>> = HashMap::new();
        for r in &self.query(&sql, &[]).await? {
            let b = branch_from(r)?;
            by_run.entry(b.run_id.clone()).or_default().push(b);
        }
        Ok(by_run)
    }

    /// The ids of the run's branches.
    pub async fn run_branches(&self, run_id: &str) -> Result<Vec<BranchId>> {
        let sql = format!(
            "SELECT b.id FROM {BRANCHES_OF_RUNS} WHERE b.run_id = $1 ORDER BY b.created_at, b.id"
        );
        let rows = self.query(&sql, &[&run_id]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Deletes the run's branches, events and the run itself.
    pub async fn delete_run(&self, run_id: &str) -> Result<()> {
        let ids = self.run_branches(run_id).await?;
        // One statement, so the branch/parent foreign keys are checked at its end.
        self.execute(
            "WITH ev AS (DELETE FROM events WHERE branch_id = ANY($2)),
                      br AS (DELETE FROM branches WHERE run_id = $1)
                 DELETE FROM runs WHERE id = $1",
            &[&run_id, &ids],
        )
        .await?;
        Ok(())
    }

    /// True if some branch of the run has status `running`.
    pub async fn run_has_running(&self, run_id: &str) -> Result<bool> {
        let sql = "SELECT EXISTS (SELECT 1 FROM branches WHERE run_id = $1 AND status = 'running')";
        Ok(self.query_one(sql, &[&run_id]).await?.get(0))
    }

    pub async fn run_exists(&self, run_id: &str) -> Result<bool> {
        let sql = "SELECT EXISTS (SELECT 1 FROM runs WHERE id = $1)";
        Ok(self.query_one(sql, &[&run_id]).await?.get(0))
    }
}

/// The run of row `r` (of [`RUN_COLS`]) with its `branches`.
fn summary_from(r: &Row, branches: Vec<Branch>) -> RunSummary {
    RunSummary {
        id: r.get("id"),
        name: r.get("name"),
        task_id: r.get("task_id"),
        created_at: r.get("created_at"),
        branches,
    }
}

fn branch_from(r: &Row) -> Result<Branch> {
    let status: String = r.get("status");
    let mut team: Value = r.get("team");
    crate::model::legacy_read_only(&mut team);
    let mode: Option<String> = r.get("restore_mode");
    let restored: Value = r.get("restored");
    Ok(Branch {
        id: r.get("id"),
        run_id: r.get("run_id"),
        parent_id: r.get("parent_id"),
        start_seq: r.get("start_seq"),
        team: serde_json::from_value(team)?,
        changes: r.get("changes"),
        fork_note: r.get("fork_note"),
        status: BranchStatus::parse(&status).ok_or_else(|| anyhow!("unknown status {status}"))?,
        error: r.get("error"),
        created_at: r.get("created_at"),
        ended_at: r.get("ended_at"),
        restore_mode: mode
            .map(|m| serde_json::from_value::<RestoreMode>(Value::String(m)))
            .transpose()?,
        restored: serde_json::from_value(restored)?,
    })
}
