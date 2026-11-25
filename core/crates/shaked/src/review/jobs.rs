//! What a review is of, and the job each subject makes: the prompt, what it may cite, and how its
//! answer is checked. The checks that come before a review starts (404/409) are the HTTP layer's.

use crate::auth::Caller;
use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    branch_key, compare_key,
    digest::{digest, steps_of, RunRecord, MAX_DIGEST_BYTES},
    output::{Citable, Cites},
    pair::{pair_digest, CompareFacts},
    prompt,
    suites::{runs_of, suite_digest},
    suites_key, ticket, Job, Shape,
};
use crate::{app::AppState, model::Team, suite};

/// What a review is of: one branch, two branches compared, or two suites compared.
#[derive(Clone)]
pub enum Subject {
    Branch(String),
    Compare(String, String),
    Suites(String, String),
}

impl Subject {
    /// The key its review is stored under.
    pub fn key(&self) -> String {
        match self {
            Subject::Branch(id) => branch_key(id),
            Subject::Compare(a, b) => compare_key(a, b),
            Subject::Suites(a, b) => suites_key(a, b),
        }
    }

    /// How its review's answer is checked.
    pub fn shape(&self) -> Shape {
        match self {
            Subject::Branch(_) => Shape::Run,
            Subject::Compare(..) => Shape::Compare(Cites::Steps),
            Subject::Suites(..) => Shape::Compare(Cites::Runs),
        }
    }
}

/// The ids of the teams' agents: the agents a review may cite.
fn agent_ids<'a>(teams: impl IntoIterator<Item = &'a Team>) -> BTreeSet<String> {
    let agents = teams.into_iter().flat_map(|t| &t.agents);
    agents.map(|a| a.id.clone()).collect()
}

/// One branch's review, from its record.
pub async fn branch_job(state: &AppState, caller: &Caller, id: &str) -> Result<Job> {
    let records = state.store.at(caller);
    let branch = records.branch(id).await?;
    let events = records.events(id).await?;
    let ticket = ticket(state, &events).await;
    let rec = RunRecord {
        branch: &branch,
        events: &events,
        ticket: ticket.as_deref(),
    };
    let citable = Citable {
        steps_a: steps_of(&events),
        agents: agent_ids([&branch.team]),
        ..Citable::default()
    };
    Ok(Job {
        prompt: prompt::run_prompt(&digest(&rec, MAX_DIGEST_BYTES)),
        citable,
        shape: Shape::Run,
    })
}

/// Two branches' review, from their records and what comparing them found.
pub fn compare_job(a: &RunRecord, b: &RunRecord, facts: &CompareFacts) -> Job {
    let citable = Citable {
        steps_a: steps_of(a.events),
        steps_b: steps_of(b.events),
        agents: agent_ids([&a.branch.team, &b.branch.team]),
        ..Citable::default()
    };
    Job {
        prompt: prompt::compare_prompt(&pair_digest(a, b, facts)),
        citable,
        shape: Shape::Compare(Cites::Steps),
    }
}

/// Two suites' review: suite `b` against suite `a`, from their reports.
pub async fn suites_job(
    state: &AppState,
    caller: &Caller,
    a: suite::Suite,
    b: suite::Suite,
) -> Result<Job> {
    let ra = suite::load(state.store.at(caller), a).await?;
    let rb = suite::load(state.store.at(caller), b).await?;
    let d = suite_digest(&ra, &rb, &suite::compare(&ra, &rb));
    let citable = Citable {
        runs_a: runs_of(&ra),
        runs_b: runs_of(&rb),
        agents: agent_ids([&ra.suite.team, &rb.suite.team]),
        ..Citable::default()
    };
    Ok(Job {
        prompt: prompt::suites_prompt(&d),
        citable,
        shape: Shape::Compare(Cites::Runs),
    })
}
