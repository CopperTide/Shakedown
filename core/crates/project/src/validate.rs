//! Checks references between items: a scenario's world, an experiment's world, scenario, setups
//! and parent. A broken reference is a problem on the item that holds it.

use crate::format::Kind;
use crate::item::{ItemBody, Problem};
use crate::load::Items;
use crate::Id;

/// Every broken reference between `items`.
pub(crate) fn cross_check(items: &Items) -> Vec<Problem> {
    let mut problems = Vec::new();
    for ((kind, id), item) in items {
        let mut report = |pointer: &str, message: String| {
            problems.push(Problem {
                kind: Some(*kind),
                id: Some(id.to_string()),
                pointer: pointer.into(),
                message,
            });
        };
        match &item.body {
            ItemBody::Scenario(s) => require(items, Kind::World, &s.world, "/world", &mut report),
            ItemBody::Experiment(e) => check_experiment(items, e, &mut report),
            ItemBody::World(_) | ItemBody::Setup(_) => {}
        }
    }
    problems
}

fn check_experiment(
    items: &Items,
    e: &crate::format::ExperimentFile,
    report: &mut impl FnMut(&str, String),
) {
    require(items, Kind::World, &e.world, "/world", report);
    require(items, Kind::Scenario, &e.scenario, "/scenario", report);
    for (i, setup) in e.setups.iter().enumerate() {
        require(items, Kind::Setup, setup, &format!("/setups/{i}"), report);
    }
    if let Some(parent) = &e.parent {
        require(items, Kind::Experiment, parent, "/parent", report);
    }
    if let Some(world) = scenario_world(items, &e.scenario) {
        if world != e.world {
            report(
                "/world",
                format!("the scenario happens in {world}, not {}", e.world),
            );
        }
    }
}

fn require(
    items: &Items,
    kind: Kind,
    name: &str,
    pointer: &str,
    report: &mut impl FnMut(&str, String),
) {
    let found = Id::parse(name).is_ok_and(|id| items.contains_key(&(kind, id)));
    if !found {
        report(
            pointer,
            format!("no {} named {name}", crate::item::kind_name(kind)),
        );
    }
}

fn scenario_world<'a>(items: &'a Items, scenario: &str) -> Option<&'a str> {
    let id = Id::parse(scenario).ok()?;
    match &items.get(&(Kind::Scenario, id))?.body {
        ItemBody::Scenario(s) => Some(&s.world),
        _ => None,
    }
}
