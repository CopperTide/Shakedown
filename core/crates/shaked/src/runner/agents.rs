//! Running a branch's agents: one after another or all at once, each between its `AgentStart`
//! and `AgentEnd`, with the handoff notes it is given.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{
    lifecycle::{Ending, StartPoint},
    score::{check, freeze},
};
use crate::{
    agents::{AgentResult, AgentRun, Driver, FailureKind},
    app::AppState,
    exec::{mail_for, BranchCtx},
    model::*,
    prompts,
    world_clock::WorldClock,
};

/// What every agent's input starts with: the task's ticket and the benchmark's instructions
/// under their title.
#[derive(Debug, Clone)]
pub(crate) struct Brief {
    pub(crate) ticket: String,
    pub(crate) title: String,
    pub(crate) instructions: String,
    /// The world's own clock, when the task's world keeps a calendar of its own.
    pub(crate) clock: Option<WorldClock>,
}

pub(super) async fn drive(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    from: StartPoint,
) -> Result<Ending> {
    let brief = brief_of(state, ctx).await?;
    restore_notes(ctx, &from)?;
    let agents = async {
        match ctx.rules.runner {
            Runner::Chain => run_chain(state, ctx, &brief, from).await,
            Runner::Concurrent => run_concurrent(state, ctx, &brief, from.agent_index).await,
            Runner::Dispatch => crate::dispatch::run(state, ctx, &brief).await,
            Runner::External => Err(anyhow!("an external system drives this run, not shaked")),
        }
    };
    // Stopping drops the agents where they are (a CLI agent's process is killed with it).
    let ended = tokio::select! {
        ended = agents => ended?,
        () = ctx.flow.stopped() => return Ok(Ending::STOPPED),
    };
    if let Some(end) = ended {
        // Over its spend limit, a run is scored as it stands (a failure).
        if end.status == BranchStatus::BudgetExceeded {
            freeze(ctx).await;
            check(state, ctx).await?;
        }
        return Ok(end);
    }
    freeze(ctx).await;
    check(state, ctx).await
}

/// The task's ticket, the benchmark's instructions and the world's clock.
async fn brief_of(state: &AppState, ctx: &BranchCtx) -> Result<Brief> {
    let bench = state.hub.bench_describe(ctx.world.bench()).await?;
    let task = (bench.tasks.iter().find(|t| t.id == ctx.task_id))
        .ok_or_else(|| anyhow!("unknown task {}", ctx.task_id))?;
    Ok(Brief {
        ticket: task.ticket.clone(),
        title: prompts::instructions_title(&bench).to_string(),
        clock: WorldClock::of(task.clock_start.as_deref(), ctx.clock.speed()),
        instructions: bench.instructions,
    })
}

/// The agents from the start point on, one after another; `Some` when one ends the branch.
async fn run_chain(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    brief: &Brief,
    from: StartPoint,
) -> Result<Option<Ending>> {
    let mut resume = from.resume;
    for idx in from.agent_index..ctx.roster.team.agents.len() {
        if let Some(end) = run_agent(state, ctx, idx, brief, resume.take()).await? {
            return Ok(Some(end));
        }
    }
    Ok(None)
}

/// The agents from `first` on, all at once. Every one runs to its end; the branch then ends
/// with the worst of their endings, if any.
async fn run_concurrent(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    brief: &Brief,
    first: usize,
) -> Result<Option<Ending>> {
    let runs = (first..ctx.roster.team.agents.len()).map(|i| run_agent(state, ctx, i, brief, None));
    let mut worst = None;
    for ended in futures_util::future::join_all(runs).await {
        worst = worse(worst, ended?);
    }
    Ok(worst)
}

/// The worse of two endings: an infrastructure error before a failure, the first of equals.
fn worse(a: Option<Ending>, b: Option<Ending>) -> Option<Ending> {
    let infra = |e: &Option<Ending>| e.as_ref().is_some_and(Ending::is_infra);
    if infra(&a) || b.is_none() {
        return a;
    }
    if a.is_none() || infra(&b) {
        return b;
    }
    a
}

/// Carries over the recorded handoff notes of the agents before the start point.
fn restore_notes(ctx: &BranchCtx, from: &StartPoint) -> Result<()> {
    let agents = &ctx.roster.team.agents;
    if from.agent_index >= agents.len() {
        return Err(anyhow!(
            "start agent index {} out of range",
            from.agent_index
        ));
    }
    let mut h = ctx.mailbox.handoffs.lock().unwrap();
    for (spec, note) in agents.iter().zip(&from.notes).take(from.agent_index) {
        h.insert(spec.id.clone(), note.clone());
    }
    Ok(())
}

/// The ticket, the instructions, and the handoff notes left for the agent: in a legacy run the
/// previous agent's (which must exist), otherwise every note recorded so far for it.
async fn agent_input(
    state: &AppState,
    ctx: &BranchCtx,
    idx: usize,
    brief: &Brief,
) -> Result<String> {
    let Brief {
        ticket,
        title,
        instructions,
        clock,
    } = brief;
    let mut input = clock.map_or_else(String::new, |c| c.brief(ctx.clock.now_ms()));
    input.push_str(&format!("{ticket}\n# {title}\n\n{instructions}\n"));
    let notes = match ctx.rules.handoff_required {
        true => legacy_note(ctx, idx)?.into_iter().collect(),
        false => notes_for(state, ctx, &ctx.roster.team.agents[idx].id).await?,
    };
    for (from, note) in notes {
        input.push_str(&format!("\n# Handoff note from {from}\n\n{note}\n"));
    }
    Ok(input)
}

/// The previous agent's name and note, in a legacy run (none for the first agent).
fn legacy_note(ctx: &BranchCtx, idx: usize) -> Result<Option<(String, String)>> {
    let Some(prev) = idx.checked_sub(1).map(|i| &ctx.roster.team.agents[i]) else {
        return Ok(None);
    };
    let spec = &ctx.roster.team.agents[idx];
    let note = ctx
        .mailbox
        .handoff_note(&prev.id)
        .ok_or_else(|| anyhow!("no handoff note from {} to start {}", prev.id, spec.id))?;
    Ok(Some((prev.name.clone(), note)))
}

/// The name of the sender and the note of each handoff recorded so far for `agent`.
async fn notes_for(
    state: &AppState,
    ctx: &BranchCtx,
    agent: &str,
) -> Result<Vec<(String, String)>> {
    let events = ctx.records(&state.store).events(&ctx.branch_id).await?;
    let name_of = |id: &str| {
        let spec = ctx.roster.team.agents.iter().find(|a| a.id == id);
        spec.map_or_else(|| id.to_string(), |a| a.name.clone())
    };
    let mail = mail_for(&events, agent);
    let notes = mail.iter().filter(|m| m["kind"] == "handoff").map(|m| {
        let from = name_of(m["from"].as_str().unwrap_or(""));
        (from, m["text"].as_str().unwrap_or("").to_string())
    });
    Ok(notes.collect())
}

/// Runs agent `idx` between its AgentStart and AgentEnd events; `Some` when the branch ends here.
async fn run_agent(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    idx: usize,
    brief: &Brief,
    resume: Option<Vec<Value>>,
) -> Result<Option<Ending>> {
    let spec = ctx.roster.team.agents[idx].clone();
    let input = agent_input(state, ctx, idx, brief).await?;
    let driver = driver_for(state, spec.kind)?;
    let start = AgentStartPayload::of(&spec);
    ctx.record(&state.store, Some(&spec.id), &start).await?;
    let run = AgentRun {
        branch: ctx.clone(),
        spec: spec.clone(),
        input,
        resume,
    };
    let result = driver.run(run).await;
    record_end(state, ctx, &spec.id, &result).await?;
    Ok(agent_ending(ctx, &spec.id, result))
}

/// Records an agent's AgentEnd; in a legacy run the world is frozen first after the last agent,
/// so nothing happens in it after its end. Otherwise it freezes once every agent is done.
async fn record_end(
    state: &AppState,
    ctx: &BranchCtx,
    agent: &str,
    result: &AgentResult,
) -> Result<()> {
    if ctx.rules.handoff_required && ctx.roster.is_last(agent) {
        freeze(ctx).await;
    }
    let end = serde_json::to_value(agent_end(result))?;
    ctx.append(&state.store, Some(agent), EventKind::AgentEnd, end)
        .await?;
    Ok(())
}

pub(crate) fn driver_for(state: &AppState, kind: AgentKind) -> Result<Arc<dyn Driver>> {
    let drivers = state
        .drivers
        .get()
        .ok_or_else(|| anyhow!("no drivers installed"))?;
    drivers
        .get(&kind)
        .cloned()
        .ok_or_else(|| anyhow!("no driver for agent kind {kind:?}"))
}

pub(crate) fn agent_end(result: &AgentResult) -> AgentEndPayload {
    AgentEndPayload {
        ok: result.ok,
        error: result.error.clone(),
        tokens: Some(result.tokens.total_tokens),
        prompt_tokens: Some(result.tokens.prompt_tokens),
        completion_tokens: Some(result.tokens.completion_tokens),
        cost_usd: result.cost_usd,
        cost_source: result.cost_source.clone(),
        seconds: Some(result.seconds),
    }
}

/// How the branch ends after an agent, if it does: a stopped world program, a failed agent, or
/// (in a legacy run) a missing handoff.
fn agent_ending(ctx: &BranchCtx, agent: &str, result: AgentResult) -> Option<Ending> {
    if let Some(f) = ctx.lifecycle.fatal() {
        return Some(Ending::infra(f));
    }
    if !result.ok {
        let e = result.error.unwrap_or_else(|| "agent failed".to_string());
        // The model's own behavior is a failed attempt, not a broken harness.
        let status = match result.failure {
            Some(FailureKind::Model) => BranchStatus::Failed,
            Some(FailureKind::Budget) => BranchStatus::BudgetExceeded,
            None => BranchStatus::InfraError,
        };
        return Some(Ending::because(status, e));
    }
    // An agent whose cost is known only at its end (`claude -p`, Codex): checked then.
    if let Some(why) = result.cost_usd.and_then(|c| ctx.budget.settle(agent, c)) {
        return Some(Ending::because(BranchStatus::BudgetExceeded, why));
    }
    let required = ctx.rules.handoff_required && !ctx.roster.is_last(agent);
    if required && ctx.mailbox.handoff_note(agent).is_none() {
        return Some(Ending::failed("no handoff"));
    }
    None
}
