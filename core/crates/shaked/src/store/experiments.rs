//! Experiments in the metadata database: each experiment's definition, its setups being suites
//! (`suites.experiment_id`, in `suites.setup_ord` order).

use anyhow::Result;
use tokio_postgres::Row;

use super::{
    new_id,
    suites::{suite_from, SetupOf, SUITE_COLS},
    NewSuite, Scoped,
};
use crate::{
    experiment::Experiment,
    model::{Runner, Team},
    scenario::Scenario,
    suite::Suite,
};

const COLS: &str = "id, name, task_id, benchmark, runs, created_at, ended_at, error";

/// One setup of a new experiment: its name, team and runner.
pub struct NewSetup<'a> {
    pub name: &'a str,
    pub team: &'a Team,
    pub runner: Runner,
    /// A Code connection: shaked launches its driver for each of the setup's runs.
    pub connection: Option<&'a str>,
}

/// What a new experiment is: its task and scenario, how many runs each setup gets, and the
/// setups.
pub struct NewExperiment<'a> {
    pub name: &'a str,
    pub task: &'a str,
    pub runs: i32,
    pub benchmark: &'a str,
    pub scenario: &'a Scenario,
    pub setups: &'a [NewSetup<'a>],
    /// Each run's model-spend limit in USD.
    pub max_cost_usd: Option<f64>,
    /// The base of the runs' paired seeds: run i of every setup starts from the same seed.
    pub seed: u64,
    /// Each run's world time window in minutes.
    pub world_minutes: Option<f64>,
    /// The project experiment file it is deployed from, if any.
    pub project_experiment: Option<&'a str>,
}

fn experiment_from(r: &Row) -> Experiment {
    Experiment {
        id: r.get("id"),
        name: r.get("name"),
        task_id: r.get("task_id"),
        benchmark: r.get("benchmark"),
        runs: r.get("runs"),
        created_at: r.get("created_at"),
        ended_at: r.get("ended_at"),
        error: r.get("error"),
    }
}

/// The suite of one setup of `new`.
fn suite_of<'a>(new: &NewExperiment<'a>, s: &NewSetup<'a>, tasks: &'a [String]) -> NewSuite<'a> {
    NewSuite {
        name: s.name,
        team: s.team,
        tasks,
        repeats: new.runs,
        runner: s.runner,
        benchmark: new.benchmark,
        scenario: new.scenario,
        variants: &[],
        max_cost_usd: new.max_cost_usd,
        connection: s.connection,
    }
}

impl Scoped<'_> {
    /// Stores a new experiment and one suite per setup (the task `runs` times in the scenario):
    /// the experiment and its suites in setup order.
    pub async fn create_experiment(
        &self,
        new: NewExperiment<'_>,
    ) -> Result<(Experiment, Vec<Suite>)> {
        let id = new_id();
        let sql = "INSERT INTO experiments (id, name, task_id, benchmark, runs, project_experiment)
                   VALUES ($1, $2, $3, $4, $5, $6)";
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 6] = [
            &id,
            &new.name,
            &new.task,
            &new.benchmark,
            &new.runs,
            &new.project_experiment,
        ];
        self.execute(sql, &params).await?;
        let tasks = [new.task.to_string()];
        let mut suites = vec![];
        for (ord, s) in (0..).zip(new.setups) {
            let suite = suite_of(&new, s, &tasks);
            let of = SetupOf {
                experiment: &id,
                ord,
                base_seed: new.seed,
                world_minutes: new.world_minutes,
            };
            suites.push(self.insert_suite(&suite, Some(of)).await?);
        }
        let stored = self.experiment(&id).await?;
        Ok((stored.expect("the experiment was just stored"), suites))
    }

    /// The experiments not deployed from, or moved into, a project experiment, oldest first.
    pub async fn unlinked_experiments(&self) -> Result<Vec<String>> {
        let sql =
            "SELECT id FROM experiments WHERE project_experiment IS NULL ORDER BY created_at, id";
        let rows = self.query(sql, &[]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Links experiment `id` to project experiment `project_experiment`, unless it already has
    /// one: whether it was linked.
    pub async fn link_deployment(&self, id: &str, project_experiment: &str) -> Result<bool> {
        let sql = "UPDATE experiments SET project_experiment = $2 WHERE id = $1 AND project_experiment IS NULL";
        Ok(self.execute(sql, &[&id, &project_experiment]).await? == 1)
    }

    /// Unlinks every experiment deployed from project experiment `project_experiment`, whose
    /// file is deleted: they list as earlier experiments, and a new file with the same id starts
    /// with no deployments. How many were unlinked.
    pub async fn unlink_deployments(&self, project_experiment: &str) -> Result<u64> {
        let sql = "UPDATE experiments SET project_experiment = NULL WHERE project_experiment = $1";
        self.execute(sql, &[&project_experiment]).await
    }

    /// The experiments deployed from project experiment `project_experiment`, newest first.
    pub async fn deployments_of(&self, project_experiment: &str) -> Result<Vec<String>> {
        let sql =
            "SELECT id FROM experiments WHERE project_experiment = $1 ORDER BY created_at DESC, id";
        let rows = self.query(sql, &[&project_experiment]).await?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Every experiment, newest first.
    pub async fn experiments(&self) -> Result<Vec<Experiment>> {
        let sql = format!("SELECT {COLS} FROM experiments ORDER BY created_at DESC, id");
        let rows = self.query(&sql, &[]).await?;
        Ok(rows.iter().map(experiment_from).collect())
    }

    pub async fn experiment(&self, id: &str) -> Result<Option<Experiment>> {
        let sql = format!("SELECT {COLS} FROM experiments WHERE id = $1");
        let row = self.query_opt(&sql, &[&id]).await?;
        Ok(row.as_ref().map(experiment_from))
    }

    /// The experiment's suites, in setup order.
    pub async fn experiment_suites(&self, id: &str) -> Result<Vec<Suite>> {
        let sql =
            format!("SELECT {SUITE_COLS} FROM suites WHERE experiment_id = $1 ORDER BY setup_ord");
        let rows = self.query(&sql, &[&id]).await?;
        rows.iter().map(suite_from).collect()
    }

    /// True once the experiment ended (finished or stopped); a missing one counts as ended.
    pub async fn experiment_ended(&self, id: &str) -> Result<bool> {
        let sql = "SELECT ended_at IS NOT NULL AS ended FROM experiments WHERE id = $1";
        let row = self.query_opt(sql, &[&id]).await?;
        Ok(row.is_none_or(|r| r.get("ended")))
    }

    /// Records that the experiment ended, with why when it stopped early, and ends its suites
    /// that have not ended with the same why. Nothing changes for one that ended already.
    pub async fn end_experiment(&self, id: &str, error: Option<&str>) -> Result<()> {
        let sql = "WITH e AS (UPDATE experiments SET ended_at = now(), error = $2
                              WHERE id = $1 AND ended_at IS NULL RETURNING id)
                   UPDATE suites SET ended_at = now(), error = $2
                   FROM e WHERE suites.experiment_id = e.id AND suites.ended_at IS NULL";
        self.execute(sql, &[&id, &error]).await?;
        Ok(())
    }
}
