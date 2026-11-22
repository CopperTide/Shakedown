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

/// Drops the disruptions `args.remove` names (a fork's change) and arms `args.disruptions`, or
/// refuses the lot (one is bad, or an id is taken).
pub async fn arm(c: &Client, args: &Value) -> Result<Answer> {
    let armed = store::load(c).await?;
    let (removed, new) = match asked(args, &armed) {
        Ok(asked) => asked,
        Err(e) => return Ok(Answer::refused(e)),
    };
    store::remove(c, &removed).await?;
    store::save(c, &new).await?;
    let gone = removed.iter().map(|id| format!("_disruption/{id}"));
    let outcome = ToolOutcome {
        result: Some(json!({"armed": new.len(), "removed": removed})),
        error: None,
        reads: vec![],
        writes: with_keys(gone.collect(), &new),
    };
    Ok(Answer {
        outcome,
        events: vec![],
    })
}

/// The ids to remove and the disruptions to arm, checked against those `armed`.
fn asked(args: &Value, armed: &[Disruption]) -> Result<(Vec<String>, Vec<Disruption>), String> {
    let removed = ids(&args["remove"]).ok_or("remove must be a list of disruption ids")?;
    let given = args["disruptions"].as_array();
    let given = given.ok_or("disruptions must be a list")?;
    let new: Vec<Disruption> = given.iter().map(checked).collect::<Result<_, _>>()?;
    let kept = armed
        .iter()
        .map(|d| &d.id)
        .filter(|id| !removed.contains(id));
    let mut ids: Vec<&String> = kept.chain(new.iter().map(|d| &d.id)).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    if ids.len() != n {
        return Err("two disruptions have the same id".into());
    }
    Ok((removed, new))
}

/// The ids of `remove`: none when absent, None when it isn't a list of text.
fn ids(remove: &Value) -> Option<Vec<String>> {
    match remove {
        Value::Null => Some(vec![]),
        Value::Array(ids) => ids.iter().map(|i| i.as_str().map(str::to_string)).collect(),
        _ => None,
    }
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
