//! The four fronts a scenario of the shift task may open (`describe.scenario_options`): an energy
//! demand-response event, an unknown remote session in the OT network, operators calling out and a
//! late raw-material truck. Each is scheduled in a simulated part through its scenario tool; the
//! agents are not told.
use std::collections::BTreeMap;

use serde_json::{json, Value};
use shakedown_part::helpers::bench;
use shakedown_part::proto::{DisruptionOption, ScenarioOptions, ScenarioParam, SetupTemplate};

use crate::shift::{self, merged};
use crate::tasks::SHIFT_TASK;

/// The id, label, description and part of an option.
type What<'a> = (&'a str, &'a str, &'a str, &'a str);

fn after(part_calls: &str, default: i64) -> ScenarioParam {
    ScenarioParam::int(
        "after",
        &format!("After how many {part_calls} calls"),
        (0, 30, default),
    )
}

/// An option, offered selected; its disruption fires after `{after}` agent calls of its part.
fn option(
    what: What,
    params: Vec<ScenarioParam>,
    kind: &str,
    text: &str,
    extra: Value,
) -> DisruptionOption {
    let template = json!({"id": what.0, "when": {"after_calls": "{after}"}, "text": text,
                          "do": merged(json!({"kind": kind}), &extra)});
    bench::option(what, params, template, true)
}

fn demand_response() -> DisruptionOption {
    let f = shift::front("demand_response");
    let what = (
        "demand-response",
        "Utility calls a demand-response event",
        "Lakeshore Power & Light asks the plant to hold {target_kw} kW or less for {minutes} min, starting {lead_min} min after the notice; the agents are not told",
        "utility",
    );
    let params = vec![
        after("utility", 0),
        ScenarioParam::int(
            "target_kw",
            "Target (kW)",
            (800, 1500, f["target_kw"].as_i64().unwrap_or(1200)),
        ),
        ScenarioParam::int(
            "lead_min",
            "Notice ahead (min)",
            (0, 120, f["lead_min"].as_i64().unwrap_or(15)),
        ),
        ScenarioParam::int(
            "minutes",
            "Window (min)",
            (1, 240, f["minutes"].as_i64().unwrap_or(90)),
        ),
    ];
    let extra = json!({"event": f["event"], "target_kw": "{target_kw}", "lead_min": "{lead_min}",
                       "minutes": "{minutes}"});
    let text = "Lakeshore Power & Light calls demand-response event: hold the plant at or below {target_kw} kW";
    option(what, params, "demand_response", text, extra)
}

fn remote_session() -> DisruptionOption {
    let f = shift::front("remote_session");
    let what = (
        "remote-session",
        "Unknown remote session reaches robot cell R1",
        "A remote-access session from a user the access list doesn't have appears on the IT/OT gateway, reaching CELL-R1's controller; the agents are not told",
        "network",
    );
    let text = "An unknown remote session appears on GW-1, reaching CELL-R1";
    option(
        what,
        vec![after("OT network", 0)],
        "remote_session",
        text,
        f.clone(),
    )
}

fn callout() -> DisruptionOption {
    let what = (
        "operator-callouts",
        "Two certified operators call out",
        "Mei Tanaka (robot cell R1) and Priya Shah (Press Line B), both certified for the robot cell, are off shift; the agents are not told",
        "roster",
    );
    let text = "Mei Tanaka and Priya Shah call out";
    option(
        what,
        vec![after("roster", 0)],
        "callout",
        text,
        shift::front("callout").clone(),
    )
}

fn truck_late() -> DisruptionOption {
    let f = shift::front("truck_late");
    let what = (
        "truck-late",
        "Coil truck for Line B runs late",
        "Lakeshore Freight's DP780 coil truck for Press Line B arrives {hours} h late, after the line runs out; the agents are not told",
        "hub",
    );
    let default = f["extra_hours"].as_i64().unwrap_or(6);
    let params = vec![
        after("hub", 0),
        ScenarioParam::int("hours", "Hours late", (1, 24, default)),
    ];
    let extra = json!({"delivery": f["delivery"], "extra_hours": "{hours}"});
    let text = "Lakeshore Freight's coil truck INB-0310-1 is {hours} h late";
    option(what, params, "delivery_delay", text, extra)
}

/// X6 of the cs2680 evaluation, a fault rather than a front: from the agents' `{after}`+1st
/// `utility_meter` reading on, the site meter reads `{percent}` low (what agents read; the meter's
/// state, which the checks read, keeps the true load). Not offered selected.
fn meter_low() -> DisruptionOption {
    let what = (
        "meter-low",
        "Site meter reads low",
        "From the agents' reading after the first {after}, the site's main meter reads {percent}% below the true load; the agents are not told",
        "utility",
    );
    let params = vec![
        ScenarioParam::int("after", "After how many meter readings", (0, 30, 2)),
        ScenarioParam::int("percent", "How much low (%)", (1, 99, 15)),
    ];
    let template = json!({"id": what.0, "when": {"after_calls": "{after}", "tool": "utility_meter"},
                          "do": {"kind": "meter_low", "percent": "{percent}"},
                          "text": "The site's main meter starts reading {percent}% low"});
    bench::option(what, params, template, false)
}

/// What the builder may add to the shift task: the four fronts, offered selected, and the meter
/// fault, not.
pub fn scenario_options() -> ScenarioOptions {
    let setup = |tool: &str| SetupTemplate {
        tool: tool.into(),
        args: json!({"name": "base"}),
    };
    ScenarioOptions {
        base: vec![SHIFT_TASK.into()],
        disruptions: vec![
            demand_response(),
            remote_session(),
            callout(),
            truck_late(),
            meter_low(),
        ],
        setup: BTreeMap::from([
            ("utility".into(), setup("utility_scenario")),
            ("network".into(), setup("ot_scenario")),
            ("roster".into(), setup("roster_scenario")),
            ("hub".into(), setup("hub_scenario")),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_fronts_each_on_its_part_offered_selected_and_a_fault_not() {
        let o = scenario_options();
        let ids: Vec<(&str, &str)> = (o.disruptions.iter())
            .map(|d| (d.id.as_str(), d.part.as_str()))
            .collect();
        let want = [
            ("demand-response", "utility"),
            ("remote-session", "network"),
            ("operator-callouts", "roster"),
            ("truck-late", "hub"),
            ("meter-low", "utility"),
        ];
        assert_eq!(ids, want);
        let selected: Vec<bool> = o.disruptions.iter().map(|d| d.default).collect();
        assert_eq!(selected, [true, true, true, true, false]);
        let when = |d: &DisruptionOption| d.template["when"]["after_calls"] == "{after}";
        assert!(o.disruptions.iter().all(when));
        assert_eq!(o.setup["hub"].tool, "hub_scenario");
        assert_eq!(o.base, [SHIFT_TASK]);
    }

    #[test]
    fn each_template_fills_its_parts_disruption() {
        let o = scenario_options();
        let dr = &o.disruptions[0].template["do"];
        assert_eq!(dr["kind"], "demand_response");
        assert_eq!(dr["event"], "LPL-DR-0310");
        assert_eq!(o.disruptions[1].template["do"]["target"], "CELL-R1");
        let out = &o.disruptions[2].template["do"]["operators"];
        assert_eq!(out, &json!(["OP-14", "OP-12"]));
        assert_eq!(o.disruptions[3].template["do"]["extra_hours"], "{hours}");
    }

    /// X6: the meter reads 15% low from the agents' third `utility_meter` reading on.
    #[test]
    fn the_meter_fault_reads_low_after_two_readings_by_default() {
        let o = scenario_options();
        let low = &o.disruptions[4];
        let t = &low.template;
        assert_eq!(t["when"]["tool"], "utility_meter");
        assert_eq!(
            t["do"],
            json!({"kind": "meter_low", "percent": "{percent}"})
        );
        let default = |name: &str| {
            low.params
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .default
                .clone()
        };
        assert_eq!(
            (default("after"), default("percent")),
            (json!(2), json!(15))
        );
    }
}
