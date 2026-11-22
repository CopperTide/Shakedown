//! The calls the schedule takes part in: arming it (the scenario tool) and an agent's call, which
//! fires and counts its disruptions before it is answered.

use anyhow::Result;
use serde_json::{json, Value};
use shakedown_part::proto::WorldEvent;
use tokio_postgres::Client;

use super::{before_call, checked, effects, store, Disruption};
use crate::tools::ToolOutcome;

/// A call's answer and the world events it gives.
pub struct Answer {
    pub outcome: ToolOutcome,
    pub events: Vec<WorldEvent>,
}

impl Answer {
    fn refused(error: String) -> Answer {
        let outcome = ToolOutcome {
            result: None,
            error: Some(error),
            reads: vec![],
            writes: vec![],
        };
        Answer {
            outcome,
            events: vec![],
        }
    }
}

/// The internal keys of `ds`, added to `writes` (sorted, as a tool's are).
fn with_keys(mut writes: Vec<String>, ds: &[Disruption]) -> Vec<String> {
    writes.extend(ds.iter().map(Disruption::key));
    writes.sort();
    writes.dedup();
    writes
}

/// Arms `args.disruptions` (refusing them all if one is bad or an id is taken).
pub async fn arm(c: &Client, args: &Value) -> Result<Answer> {
    let Some(given) = args["disruptions"].as_array() else {
        return Ok(Answer::refused("disruptions must be a list".into()));
    };
    let new: Result<Vec<Disruption>, String> = given.iter().map(checked).collect();
    let new = match new {
        Ok(new) => new,
        Err(e) => return Ok(Answer::refused(e)),
    };
    let mut ids: Vec<String> = store::load(c).await?.into_iter().map(|d| d.id).collect();
    ids.extend(new.iter().map(|d| d.id.clone()));
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != ids.len() {
        return Ok(Answer::refused("two disruptions have the same id".into()));
    }
    store::save(c, &new).await?;
    let outcome = ToolOutcome {
        result: Some(json!({"armed": new.len()})),
        error: None,
        reads: vec![],
        writes: with_keys(vec![], &new),
    };
    Ok(Answer {
        outcome,
        events: vec![],
    })
}

/// An agent's call of `name`: the disruptions it fires act on its answer, and every one it
/// changed names its key in `writes` (so the step is saved with the count).
pub async fn agent_call(c: &mut Client, name: &str, args: &Value) -> Result<Answer> {
    let mut schedule = store::load(c).await?;
    let on = before_call(&mut schedule, name);
    store::save(c, &on.changed).await?;
    let mut outcome = effects::answer(c, name, args, &on.fired).await?;
    outcome.writes = with_keys(outcome.writes, &on.changed);
    let events = on.fired.iter().map(Disruption::event).collect();
    Ok(Answer { outcome, events })
}
