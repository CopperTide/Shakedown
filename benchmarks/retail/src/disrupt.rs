//! Scheduled disruptions in the retail world (docs/protocol.md, "Scheduled disruptions"): faults
//! that act on one agent's call, armed by the world's own tool `retail_scenario`. Each counts only
//! the calls it would change, fires once, deterministically, on the one that crosses its count,
//! and acts on that call's answer only. The schedule is kept in the retail database (`store`), so a save
//! keeps it and a fork meets the same schedule.
//!
//! - `when: {after_calls, tool?}` counts the agents' calls (only of `tool`, which may also be a
//!   list of tools: calls of any of them).
//! - `do.kind`: `stale_status` (the order's status reads delivered for pending and pending for
//!   delivered), `timeout` (the call isn't made and answers that it timed out), `variant_unavailable
//!   {item_id?}` (the variant, else the product's first in stock, reads out of stock) and
//!   `lost_write` (the write answers as if done but nothing is written).

pub mod calls;
pub mod effects;
pub mod store;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shakedown_part::proto::WorldEvent;

use crate::tools;

/// The world's own tool that arms disruptions (a scenario's setup calls it; agents don't get it).
pub const SCENARIO_TOOL: &str = "retail_scenario";

/// The kind of the world event a disruption gives when it fires.
pub const EVENT: &str = "disruption";

const KINDS: [&str; 4] = [
    "stale_status",
    "timeout",
    "variant_unavailable",
    "lost_write",
];

/// When a disruption fires: after `after_calls` counted calls (of `tool` only, when given).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct When {
    pub after_calls: u64,
    pub tool: Option<Vec<String>>,
}

/// One disruption as the schedule keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Disruption {
    pub id: String,
    pub when: When,
    #[serde(rename = "do")]
    pub act: Value,
    pub text: String,
    /// The counted calls so far.
    pub seen: u64,
    pub fired: bool,
}

impl Disruption {
    /// Its internal key (`_disruption/<id>`): the part's bookkeeping, not the world's state.
    pub fn key(&self) -> String {
        format!("_disruption/{}", self.id)
    }

    pub fn kind(&self) -> &str {
        self.act["kind"].as_str().unwrap_or_default()
    }

    /// True when its count is reached: the next call it would change fires it.
    pub fn due(&self) -> bool {
        self.seen >= self.when.after_calls
    }

    /// True when a call of `tool` may count for it (it hasn't fired).
    pub fn counts(&self, tool: &str) -> bool {
        let of = self.when.tool.as_ref();
        !self.fired && of.is_none_or(|t| t.iter().any(|n| n == tool))
    }

    /// The world event it gives when it fires.
    pub fn event(&self) -> WorldEvent {
        let mut data = json!({"id": self.id});
        if let Some(act) = self.act.as_object() {
            data.as_object_mut().unwrap().extend(act.clone());
        }
        WorldEvent {
            kind: EVENT.into(),
            text: self.text.clone(),
            data,
        }
    }
}

/// `given` checked, as the schedule keeps it (not seen, not fired); the message says what is wrong.
pub fn checked(given: &Value) -> Result<Disruption, String> {
    let id = given["id"].as_str().filter(|s| !s.is_empty());
    let id = id.ok_or("each disruption needs an id (text)")?.to_string();
    let text = given["text"].as_str();
    let text = text.ok_or(format!("disruption {id}: text must be text"))?;
    let when = when(&id, &given["when"])?;
    let act = act(&id, &given["do"])?;
    if when.after_calls == 0 && when.tool.is_none() {
        return Err(format!(
            "disruption {id}: retail's disruptions act on a call, so give when.tool or after_calls above 0"
        ));
    }
    Ok(Disruption {
        id,
        when,
        act,
        text: text.to_string(),
        seen: 0,
        fired: false,
    })
}

fn when(id: &str, w: &Value) -> Result<When, String> {
    let n = w["after_calls"].as_u64();
    let n = n.ok_or(format!(
        "disruption {id}: when.after_calls must be a whole number, 0 or more"
    ))?;
    Ok(When {
        after_calls: n,
        tool: when_tool(id, &w["tool"])?,
    })
}

/// `when.tool`: none, a retail tool, or a list of them (any of them counts).
fn when_tool(id: &str, tool: &Value) -> Result<Option<Vec<String>>, String> {
    let names: Vec<String> = match tool {
        Value::Null => return Ok(None),
        Value::String(t) => vec![t.clone()],
        Value::Array(ts) if ts.iter().all(Value::is_string) => ts
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => {
            let e = format!("disruption {id}: when.tool must be a tool or a list of tools");
            return Err(e);
        }
    };
    let retail = |t: &String| tools::tool_names().any(|n| n == t.as_str());
    match names.iter().find(|t| !retail(t)) {
        Some(t) => Err(format!(
            "disruption {id}: when.tool {t:?} is not a retail tool"
        )),
        None => Ok(Some(names)),
    }
}

fn act(id: &str, act: &Value) -> Result<Value, String> {
    let kind = act["kind"].as_str().filter(|k| KINDS.contains(k));
    let Some(kind) = kind else {
        return Err(format!(
            "disruption {id}: do.kind must be one of {}",
            KINDS.join(", ")
        ));
    };
    let item = &act["item_id"];
    if kind == "variant_unavailable" && !(item.is_null() || item.is_string()) {
        return Err(format!("disruption {id}: do.item_id must be text"));
    }
    Ok(act.clone())
}

/// What the schedule did on one agent's call: the disruptions that fired on it and every
/// disruption it changed (fired or counted).
#[derive(Debug, Default, PartialEq)]
pub struct OnCall {
    pub fired: Vec<Disruption>,
    pub changed: Vec<Disruption>,
}

/// For an agent's call of `tool`, once it is answered: each disruption it may count for and that
/// `alters` says it changes fires when due, else counts the call. A call it wouldn't change (an
/// order neither delivered nor pending, a write that failed anyway) counts for nothing.
pub fn settle(
    schedule: &mut [Disruption],
    tool: &str,
    alters: impl Fn(&Disruption) -> bool,
) -> OnCall {
    let mut out = OnCall::default();
    for d in schedule.iter_mut().filter(|d| d.counts(tool) && alters(d)) {
        if d.due() {
            d.fired = true;
            out.fired.push(d.clone());
        } else {
            d.seen += 1;
        }
        out.changed.push(d.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(after: u64, tool: Value, kind: &str) -> Value {
        json!({"id": "x1", "when": {"after_calls": after, "tool": tool}, "do": {"kind": kind},
               "text": "Stale"})
    }

    #[test]
    fn it_fires_once_on_the_call_that_crosses_its_count_of_its_tools_only() {
        let d = checked(&given(1, json!("get_order_details"), "stale_status")).unwrap();
        let mut s = vec![d];
        let alters = |_: &Disruption| true;
        let other = settle(&mut s, "get_user_details", alters);
        assert_eq!(other, OnCall::default(), "another tool counts for nothing");
        let unchanged = settle(&mut s, "get_order_details", |_| false);
        assert_eq!(
            unchanged,
            OnCall::default(),
            "a call it wouldn't change counts for nothing"
        );
        let first = settle(&mut s, "get_order_details", alters);
        assert!(first.fired.is_empty() && first.changed[0].seen == 1);
        let second = settle(&mut s, "get_order_details", alters);
        assert_eq!(second.fired[0].id, "x1");
        assert_eq!(second.fired[0].key(), "_disruption/x1");
        assert_eq!(
            settle(&mut s, "get_order_details", alters),
            OnCall::default()
        );
    }

    #[test]
    fn a_list_of_tools_counts_a_call_of_any_of_them() {
        let writes = json!(["modify_user_address", "cancel_pending_order"]);
        let mut s = vec![checked(&given(0, writes, "lost_write")).unwrap()];
        let alters = |_: &Disruption| true;
        assert!(settle(&mut s, "get_order_details", alters).fired.is_empty());
        assert_eq!(
            settle(&mut s, "cancel_pending_order", alters).fired.len(),
            1
        );
    }

    #[test]
    fn its_event_carries_its_id_and_what_it_does() {
        let d = checked(&given(
            0,
            json!("get_product_details"),
            "variant_unavailable",
        ))
        .unwrap();
        let e = d.event();
        assert_eq!((e.kind.as_str(), e.text.as_str()), ("disruption", "Stale"));
        assert_eq!(e.data, json!({"id": "x1", "kind": "variant_unavailable"}));
    }

    #[test]
    fn a_bad_disruption_is_refused_saying_why() {
        let bad = [
            (given(0, json!("nope"), "timeout"), "is not a retail tool"),
            (
                given(0, json!("get_order_details"), "boom"),
                "do.kind must be one of",
            ),
            (given(0, Value::Null, "timeout"), "act on a call"),
            (json!({"id": "", "text": "t"}), "needs an id"),
            (
                json!({"id": "a", "when": {"after_calls": -1}, "text": "t"}),
                "after_calls",
            ),
        ];
        for (d, why) in bad {
            let e = checked(&d).unwrap_err();
            assert!(e.contains(why), "{e}");
        }
    }
}
