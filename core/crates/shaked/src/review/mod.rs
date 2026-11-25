//! AI review: a real model reads a compact digest of a run (or of two runs, or two suites, compared)
//! and says where it went wrong and what to change (or what matters between the two). Optional:
//! off means nothing is sent to a model ([`settings`]). Reviews are stored and not recomputed
//! unless asked.
//!
//! - [`jobs`]: what a review is of ([`jobs::Subject`]), and the job each subject makes.
//! - [`digest`], [`pair`], [`suites`]: what the reviewer reads, built from the record, bounded.
//! - [`prompt`]: what it is told, and the schema of its answer.
//! - [`reviewer`]: the call to the model (Claude Code headless, or one of shaked's providers).
//! - [`output`]: the answer checked: fields validated, citations checked against the record.
//! - [`ask`], [`asking`]: a question about one step, answered from what the agent had there,
//!   evidence (quotes checked against the record) kept apart from speculation.

pub mod ask;
pub mod asking;
pub mod digest;
pub mod jobs;
pub mod output;
pub mod pair;
pub mod prompt;
pub mod reviewer;
pub mod settings;
pub mod suites;

use crate::auth::Caller;
use std::sync::Arc;

use anyhow::Result;

use crate::{app::AppState, store::ReviewDone};
use output::{Citable, Cites};
use prompt::Prompt;
use settings::ReviewerModel;

pub fn branch_key(id: &str) -> String {
    format!("branch:{id}")
}

pub fn compare_key(a: &str, b: &str) -> String {
    format!("compare:{a}:{b}")
}

pub fn suites_key(a: &str, b: &str) -> String {
    format!("suites:{a}:{b}")
}

/// How an answer is checked: as a run review, or as a comparison citing steps or runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Run,
    Compare(Cites),
}

impl Shape {
    /// The stored review's kind.
    pub fn kind(self) -> &'static str {
        match self {
            Shape::Run => "run",
            Shape::Compare(Cites::Steps) => "compare",
            Shape::Compare(Cites::Runs) => "suites",
        }
    }
}

/// One review to make: the prompt, what it may cite, and how its answer is checked.
pub struct Job {
    pub prompt: Prompt,
    pub citable: Citable,
    pub shape: Shape,
}

/// The ticket of the run's task, read from the benchmark the run was on (its `RunStart`; the
/// current one for a run recorded before runs named theirs), when that benchmark has it.
pub async fn ticket(state: &AppState, events: &[crate::model::Event]) -> Option<String> {
    let start = crate::model::RunStartPayload::of(events)?;
    let bench = match start.benchmark {
        Some(name) => state.bench_named(&name).await,
        None => state.bench().await,
    };
    let (task, bench) = (start.task_id?, bench.ok()?);
    bench
        .tasks
        .into_iter()
        .find(|t| t.id == task)
        .map(|t| t.ticket)
}

/// The answer checked against what the job may cite: the review as JSON, and the issues found.
pub fn check(answer: &serde_json::Value, job: &Job) -> Result<(serde_json::Value, Vec<String>)> {
    let (v, issues) = match job.shape {
        Shape::Run => {
            let (r, i) = output::run_review(answer, &job.citable)?;
            (serde_json::to_value(r)?, i)
        }
        Shape::Compare(cites) => {
            let (r, i) = output::compare_review(answer, &job.citable, cites)?;
            (serde_json::to_value(r)?, i)
        }
    };
    Ok((v, issues.0))
}

/// A checked answer, ready to store: the review (or explanation), what checking found, what
/// the model cost, and the size of what it read.
pub struct Checked {
    pub review: serde_json::Value,
    pub issues: Vec<String>,
    pub answer: reviewer::Answer,
    pub bytes: usize,
}

/// Asks the reviewer and stores its checked answer under `key`, or why it failed.
pub async fn run(
    state: Arc<AppState>,
    scope: Arc<Caller>,
    key: String,
    model: ReviewerModel,
    job: Result<Job>,
) {
    let outcome = answer(&state, &model, job).await;
    settle(&state, &scope, &key, outcome).await;
}

/// Stores how answering `key` ended: its checked answer, or why it (or storing it) failed.
pub async fn settle(state: &AppState, scope: &Caller, key: &str, outcome: Result<Checked>) {
    let records = state.store.at(scope);
    let stored = match outcome {
        Ok(c) => {
            let done = ReviewDone {
                review: &c.review,
                issues: &c.issues,
                cost_usd: c.answer.cost_usd,
                cost_source: c.answer.cost_source.as_deref(),
                tokens: c.answer.tokens,
                seconds: c.answer.seconds,
                digest_bytes: c.bytes,
            };
            records.finish_review(key, done).await
        }
        Err(e) => Err(e),
    };
    if let Err(e) = stored {
        if let Err(e2) = records.fail_review(key, &format!("{e:#}")).await {
            eprintln!("review {key}: {e:#}; storing that failed too: {e2:#}");
        }
    }
}

async fn answer(state: &AppState, model: &ReviewerModel, job: Result<Job>) -> Result<Checked> {
    let job = job?;
    let cfg = &state.config.providers;
    let a = reviewer::ask(model, &job.prompt, cfg, &state.http).await?;
    let (review, issues) = check(&a.value, &job)?;
    Ok(Checked {
        review,
        issues,
        answer: a,
        bytes: job.prompt.user.len(),
    })
}
