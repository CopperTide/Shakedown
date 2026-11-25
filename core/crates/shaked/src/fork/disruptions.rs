//! A fork's change to the world's scheduled disruptions from its start on: options of the
//! benchmark's scenario to add, and disruptions the branch's setups armed to remove. Each part
//! concerned gets one call of its scenario tool on the new world, before any agent, recorded as a
//! setup step: `{disruptions: [added], remove: [ids]}`.

use std::collections::BTreeMap;

use parts::proto::{BenchDescribe, ChosenDisruption, ScenarioRequest, SetupCall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    exec::SETUP_ACTOR,
    model::{Event, EventKind},
    scenario,
};

/// What a fork request changes in the schedule.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DisruptionChange {
    /// Options of the benchmark's `scenario_options`, chosen as a run's scenario chooses them.
    #[serde(default)]
    pub add: Vec<ChosenDisruption>,
    /// Ids of disruptions the branch's setups armed before the fork point.
    #[serde(default)]
    pub remove: Vec<String>,
}

impl DisruptionChange {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

/// Per part: the disruptions to add and the ids to remove.
type ByPart = BTreeMap<String, (Vec<Value>, Vec<String>)>;

/// The calls that make `change` on a fork of `events` starting at `start_seq`; the message says
/// why it can't be made (an option the benchmark doesn't offer, an id no setup armed before the
/// fork point, a part with no scenario tool).
pub fn calls(
    change: &DisruptionChange,
    bench: &BenchDescribe,
    events: &[Event],
    start_seq: i64,
) -> Result<Vec<SetupCall>, String> {
    if change.is_empty() {
        return Ok(vec![]);
    }
    let options = bench.scenario_options.as_ref();
    let asked = ScenarioRequest {
        disruptions: change.add.clone(),
        ..ScenarioRequest::default()
    };
    let mut by_part = ByPart::new();
    for e in scenario::expand(options, &asked)?.disruptions {
        by_part.entry(e.part).or_default().0.push(e.disruption);
    }
    let armed = armed_before(events, start_seq);
    for id in &change.remove {
        let part = armed.get(id).ok_or_else(|| {
            format!("no disruption {id:?} was armed before the fork point, so it can't be removed")
        })?;
        by_part.entry(part.clone()).or_default().1.push(id.clone());
    }
    by_part
        .into_iter()
        .map(|(part, change)| call(bench, part, change))
        .collect()
}

/// The scenario tool call of `part` that adds and removes `change`.
fn call(
    bench: &BenchDescribe,
    part: String,
    (add, remove): (Vec<Value>, Vec<String>),
) -> Result<SetupCall, String> {
    let setup = bench
        .scenario_options
        .as_ref()
        .and_then(|o| o.setup.get(&part));
    let setup = setup.ok_or_else(|| format!("the part {part} has no scenario tool to change"))?;
    Ok(SetupCall {
        part,
        tool: setup.tool.clone(),
        args: json!({"disruptions": add, "remove": remove}),
    })
}

/// Each disruption id the setup steps before `start_seq` armed, with its part.
fn armed_before(events: &[Event], start_seq: i64) -> BTreeMap<String, String> {
    let setup = |e: &&Event| {
        e.seq < start_seq
            && e.kind == EventKind::ToolCall
            && e.agent.as_deref() == Some(SETUP_ACTOR)
    };
    let mut out = BTreeMap::new();
    for e in events.iter().filter(setup) {
        let part = e.payload["part"].as_str().unwrap_or_default();
        let given = e.payload["args"]["disruptions"]
            .as_array()
            .into_iter()
            .flatten();
        for id in given.filter_map(|d| d["id"].as_str()) {
            out.insert(id.to_string(), part.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use parts::proto::{DisruptionOption, ScenarioOptions, SetupTemplate};

    fn bench() -> BenchDescribe {
        let option = DisruptionOption {
            id: "late".into(),
            label: "Late".into(),
            description: String::new(),
            part: "hub".into(),
            default: false,
            params: vec![],
            template: json!({"id": "late", "when": {"after_calls": 0, "tool": "t"}, "do": {"kind": "k"},
                             "text": "Late"}),
        };
        let setup = SetupTemplate {
            tool: "hub_scenario".into(),
            args: json!({}),
        };
        let options = ScenarioOptions {
            base: vec![],
            disruptions: vec![option],
            setup: [("hub".to_string(), setup)].into(),
        };
        let mut b: BenchDescribe = serde_json::from_value(json!({"name": "b", "version": "1",
            "protocol": 1, "world": [], "tasks": [], "instructions": "", "tools": []}))
        .unwrap();
        b.scenario_options = Some(options);
        b
    }

    fn setup_step(seq: i64, part: &str, ids: &[&str]) -> Event {
        let ds: Vec<Value> = ids.iter().map(|i| json!({"id": i})).collect();
        let p = json!({"name": "x_scenario", "part": part, "args": {"disruptions": ds}});
        crate::testkit::event(seq, Some(SETUP_ACTOR), EventKind::ToolCall, p)
    }

    fn change(add: &[&str], remove: &[&str]) -> DisruptionChange {
        let chosen = |id: &&str| ChosenDisruption {
            id: id.to_string(),
            params: Default::default(),
        };
        DisruptionChange {
            add: add.iter().map(chosen).collect(),
            remove: remove.iter().map(|r| r.to_string()).collect(),
        }
    }

    #[test]
    fn adds_and_removes_go_to_each_parts_scenario_tool() {
        let evs = [
            setup_step(2, "hub", &["jam"]),
            setup_step(9, "hub", &["later"]),
        ];
        let made = calls(&change(&["late"], &["jam"]), &bench(), &evs, 5).unwrap();
        assert_eq!(made.len(), 1);
        assert_eq!(
            (made[0].part.as_str(), made[0].tool.as_str()),
            ("hub", "hub_scenario")
        );
        assert_eq!(made[0].args["disruptions"][0]["id"], "late");
        assert_eq!(made[0].args["remove"], json!(["jam"]));
        assert_eq!(calls(&change(&[], &[]), &bench(), &evs, 5).unwrap(), vec![]);
    }

    #[test]
    fn an_unknown_option_or_an_id_not_armed_before_the_fork_is_refused() {
        let evs = [
            setup_step(2, "hub", &["jam"]),
            setup_step(9, "hub", &["later"]),
        ];
        let e = calls(&change(&["nope"], &[]), &bench(), &evs, 5).unwrap_err();
        assert!(e.contains("offers no disruption"), "{e}");
        let e = calls(&change(&[], &["later"]), &bench(), &evs, 5).unwrap_err();
        assert!(e.contains("can't be removed"), "{e}");
    }
}
