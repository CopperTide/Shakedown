//! References between items: a scenario's world, an experiment's world, scenario, setups and
//! parent. The one walk of them serves both the cross-check (a broken reference is a problem on
//! the item that holds it) and the dependents of an item.

use crate::format::{ExperimentFile, Kind};
use crate::item::{ItemBody, Problem};
use crate::load::Items;
use crate::Id;

/// Every item `body` names: the kind, the name and the JSON pointer of the name in the file.
pub(crate) fn references(body: &ItemBody) -> impl Iterator<Item = (Kind, &str, String)> {
    let mut refs = Vec::new();
    match body {
        ItemBody::Scenario(s) => refs.push((Kind::World, s.world.as_str(), "/world".to_string())),
        ItemBody::Experiment(e) => {
            refs.push((Kind::World, e.world.as_str(), "/world".to_string()));
            refs.push((Kind::Scenario, e.scenario.as_str(), "/scenario".to_string()));
            let setups = e.setups.iter().enumerate();
            refs.extend(setups.map(|(i, s)| (Kind::Setup, s.as_str(), format!("/setups/{i}"))));
            if let Some(parent) = &e.parent {
                refs.push((Kind::Experiment, parent.as_str(), "/parent".to_string()));
            }
        }
        ItemBody::World(_) | ItemBody::Setup(_) => {}
    }
    refs.into_iter()
}

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
        for (to, name, pointer) in references(&item.body) {
            if !Id::parse(name).is_ok_and(|n| items.contains_key(&(to, n))) {
                report(
                    &pointer,
                    format!("no {} named {name}", crate::item::kind_name(to)),
                );
            }
        }
        if let ItemBody::Experiment(e) = &item.body {
            same_world(items, e, &mut report);
        }
    }
    problems
}

/// An experiment's world must be its scenario's.
fn same_world(items: &Items, e: &ExperimentFile, report: &mut impl FnMut(&str, String)) {
    if let Some(world) = scenario_world(items, &e.scenario) {
        if world != e.world {
            report(
                "/world",
                format!("the scenario happens in {world}, not {}", e.world),
            );
        }
    }
}

fn scenario_world<'a>(items: &'a Items, scenario: &str) -> Option<&'a str> {
    let id = Id::parse(scenario).ok()?;
    match &items.get(&(Kind::Scenario, id))?.body {
        ItemBody::Scenario(s) => Some(&s.world),
        _ => None,
    }
}
