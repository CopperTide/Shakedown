//! What the builder offers: its measures and dimensions, each with the SQL it is made of. A
//! measure or dimension is run-level (one value per run) or step-level (read from the run's steps,
//! each model call's usage joined in); any measure goes with any dimension.

use serde::Serialize;

/// Where a measure or dimension is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Run,
    Step,
}

/// One measure: `sql` aggregates the outer query's rows (`r` a run, `p` its steps' partials).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Measure {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub level: Level,
    pub description: &'static str,
    #[serde(skip)]
    pub sql: &'static str,
    /// The steps a step-level measure counts, for drilling through (`s` a step).
    #[serde(skip)]
    pub steps: &'static str,
}

/// One dimension: `sql` reads it from `r` (run-level) or from `s` (step-level).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Dimension {
    pub id: &'static str,
    pub label: &'static str,
    pub level: Level,
    pub description: &'static str,
    #[serde(skip)]
    pub sql: &'static str,
}

const fn run(
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    description: &'static str,
    sql: &'static str,
) -> Measure {
    Measure {
        id,
        label,
        unit,
        level: Level::Run,
        description,
        sql,
        steps: "true",
    }
}

const fn step(
    id: &'static str,
    label: &'static str,
    unit: &'static str,
    description: &'static str,
    sql: &'static str,
    steps: &'static str,
) -> Measure {
    Measure {
        id,
        label,
        unit,
        level: Level::Step,
        description,
        sql,
        steps,
    }
}

const CALLS: &str = "s.kind = 'model_call'";

#[rustfmt::skip]
pub const MEASURES: &[Measure] = &[
    run("runs", "Runs", "count", "Runs counted (each run's first branch).", "count(*)::float8"),
    run("pass_rate", "Pass rate", "ratio", "Runs that passed, of those judged (passed, failed or over budget).",
        "(count(*) FILTER (WHERE r.passed))::float8 / nullif(count(*) FILTER (WHERE r.judged), 0)"),
    step("cost", "Cost", "usd", "What the model calls cost in USD: as reported, else at list price.",
        "sum(p.cost)::float8", CALLS),
    step("tokens_in", "Input tokens", "tokens", "Prompt tokens of the model calls, cached ones included.",
        "coalesce(sum(p.tokens_in), 0)::float8", CALLS),
    step("tokens_out", "Output tokens", "tokens", "Output tokens of the model calls.",
        "coalesce(sum(p.tokens_out), 0)::float8", CALLS),
    step("tokens_cached", "Cached tokens", "tokens", "Prompt tokens read from the provider's cache.",
        "coalesce(sum(p.tokens_cached), 0)::float8", CALLS),
    step("model_calls", "Model calls", "count", "Model calls made.", "coalesce(sum(p.model_calls), 0)::float8", CALLS),
    step("tool_calls", "Tool calls", "count", "Tool calls made.",
        "coalesce(sum(p.tool_calls), 0)::float8", "s.kind = 'tool_call'"),
    step("refusals", "Refusals", "count", "Tool calls a gate or decision right refused.",
        "coalesce(sum(p.refusals), 0)::float8", "s.outcome = 'refused'"),
    step("errors", "Errors", "count", "Tool calls that failed, and turns or agents that ended in an error.",
        "coalesce(sum(p.errors), 0)::float8", "s.outcome = 'error'"),
    run("duration", "Avg run time", "seconds", "Mean wall-clock time of a run, in seconds.", "avg(r.seconds)::float8"),
    step("thinking", "Thinking time", "seconds", "Estimated time waiting on models: each call's time since its agent's previous step.",
        "coalesce(sum(p.think_s), 0)::float8", CALLS),
    step("steps", "Steps", "count", "Steps recorded (bookkeeping left out).", "coalesce(sum(p.steps), 0)::float8", "true"),
];

#[rustfmt::skip]
pub const DIMENSIONS: &[Dimension] = &[
    Dimension { id: "agent", label: "Agent", level: Level::Step, description: "The agent that took the step (none: the world's own steps).", sql: "s.agent" },
    Dimension { id: "model", label: "Model", level: Level::Step, description: "The model that made the call; for other steps, the model the agent's setup gives it.", sql: "s.model" },
    Dimension { id: "setup", label: "Setup", level: Level::Run, description: "The agent setup.", sql: "r.setup" },
    Dimension { id: "scenario", label: "Scenario", level: Level::Run, description: "The scenario the run attempted.", sql: "r.scenario" },
    Dimension { id: "world", label: "World", level: Level::Run, description: "The world it ran in.", sql: "r.world" },
    Dimension { id: "experiment", label: "Experiment", level: Level::Run, description: "The experiment it belongs to (its id; the result labels it).", sql: "r.experiment_id" },
    Dimension { id: "tool", label: "Tool", level: Level::Step, description: "A tool call's tool (none: other steps).", sql: "s.tool" },
    Dimension { id: "kind", label: "Step kind", level: Level::Step, description: "What the step is: model_call, tool_call, message, reasoning, ...", sql: "s.kind" },
    Dimension { id: "day", label: "Day", level: Level::Run, description: "The day the run started (UTC), YYYY-MM-DD.", sql: "to_char(r.at AT TIME ZONE 'UTC', 'YYYY-MM-DD')" },
];

pub fn measure(id: &str) -> Option<&'static Measure> {
    MEASURES.iter().find(|m| m.id == id)
}

pub fn dimension(id: &str) -> Option<&'static Dimension> {
    DIMENSIONS.iter().find(|d| d.id == id)
}
