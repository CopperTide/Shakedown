//! The faults a run's scenario may plant in the retail world (`describe.scenario_options`; X1 to X4
//! of the cs2680 evaluation). Each is a disruption the scenario tool arms (`crate::disrupt`); none is
//! offered selected, and the agents are not told.
use std::collections::BTreeMap;

use serde_json::{json, Value};
use shakedown_part::helpers::bench;
use shakedown_part::proto::{DisruptionOption, ScenarioOptions, ScenarioParam, SetupTemplate};

use crate::disrupt::SCENARIO_TOOL;
use crate::tools;

/// The id, label, description and part of an option.
type What<'a> = (&'a str, &'a str, &'a str, &'a str);

/// A fault that fires on the agents' call of `tool` after the first `{after}` (default 0: the
/// first call), doing `kind`.
fn fault(what: What, calls_of: &str, tool: Value, kind: &str, text: &str) -> DisruptionOption {
    let label = format!("After how many {calls_of} calls");
    let params = vec![ScenarioParam::int("after", &label, (0, 20, 0))];
    let template = json!({"id": what.0, "when": {"after_calls": "{after}", "tool": tool},
                          "do": {"kind": kind}, "text": text});
    bench::option(what, params, template, false)
}

fn stale_status() -> DisruptionOption {
    let what = (
        "stale-order-status",
        "Order status reads stale",
        "The agents' get_order_details call after the first {after} reports a delivered order as pending and a pending one as delivered; the agents are not told",
        "retail",
    );
    let text = "get_order_details reports a stale order status";
    fault(
        what,
        "get_order_details",
        json!("get_order_details"),
        "stale_status",
        text,
    )
}

fn order_timeout() -> DisruptionOption {
    let what = (
        "order-read-timeout",
        "Order lookup times out",
        "The agents' get_order_details call after the first {after} isn't made and answers that it timed out and may not have been processed; the agents are not told",
        "retail",
    );
    let text = "get_order_details times out";
    fault(
        what,
        "get_order_details",
        json!("get_order_details"),
        "timeout",
        text,
    )
}

fn variant_out_of_stock() -> DisruptionOption {
    let what = (
        "variant-out-of-stock",
        "A variant in stock reads out of stock",
        "The agents' get_product_details call after the first {after} reports the product's first variant in stock as out of stock; the agents are not told",
        "retail",
    );
    let text = "get_product_details reports a variant in stock as out of stock";
    let tool = json!("get_product_details");
    fault(
        what,
        "get_product_details",
        tool,
        "variant_unavailable",
        text,
    )
}

fn lost_write() -> DisruptionOption {
    let what = (
        "lost-write",
        "A write is lost",
        "The agents' write call after the first {after} answers as if it was made but changes nothing; the agents are not told",
        "retail",
    );
    let writes: Vec<&str> = tools::tool_names().filter(|t| tools::is_write(t)).collect();
    let text = "A write answers as if made but changes nothing";
    fault(what, "write", json!(writes), "lost_write", text)
}

/// What a scenario may add to any of `tasks`: the four faults, none offered selected.
pub fn scenario_options(tasks: Vec<String>) -> ScenarioOptions {
    let setup = SetupTemplate {
        tool: SCENARIO_TOOL.into(),
        args: json!({}),
    };
    ScenarioOptions {
        base: tasks,
        disruptions: vec![
            stale_status(),
            order_timeout(),
            variant_out_of_stock(),
            lost_write(),
        ],
        setup: BTreeMap::from([("retail".into(), setup)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_faults_on_the_retail_part_none_offered_selected() {
        let o = scenario_options(vec!["27".into()]);
        let ids: Vec<&str> = o.disruptions.iter().map(|d| d.id.as_str()).collect();
        let want = [
            "stale-order-status",
            "order-read-timeout",
            "variant-out-of-stock",
            "lost-write",
        ];
        assert_eq!(ids, want);
        assert!(o
            .disruptions
            .iter()
            .all(|d| !d.default && d.part == "retail"));
        assert_eq!(o.setup["retail"].tool, "retail_scenario");
        let kinds: Vec<&Value> = o
            .disruptions
            .iter()
            .map(|d| &d.template["do"]["kind"])
            .collect();
        let want = [
            "stale_status",
            "timeout",
            "variant_unavailable",
            "lost_write",
        ];
        assert_eq!(kinds, want.map(|k| json!(k)).iter().collect::<Vec<_>>());
    }

    #[test]
    fn the_lost_write_counts_every_write_tool_and_only_those() {
        let o = scenario_options(vec![]);
        let tools = o.disruptions[3].template["when"]["tool"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(tools.len(), 7);
        assert!(tools
            .iter()
            .all(|t| crate::tools::is_write(t.as_str().unwrap())));
    }
}
