//! What benchmarks' checks and scenario options share: the end state read by part and kind, goal
//! and safety checks, and a disruption option.
use arena_part::helpers::bench::{check, option, safety, Docs};
use arena_part::proto::ScenarioParam;
use serde_json::json;

#[test]
fn docs_of_a_kind_come_in_key_order() {
    let end = json!({"erp": {"Batch/B": {"n": 2}, "Batch/A": {"n": 1}, "Item/X": {"n": 3}}});
    let d = Docs(&end);
    let batches: Vec<_> = d.docs("erp", "Batch").iter().map(|b| &b["n"]).collect();
    assert_eq!(batches, [&json!(1), &json!(2)]);
    assert!(d.docs("crm", "Batch").is_empty());
    assert_eq!(d.doc("erp", "Item/X")["n"], 3);
    assert!(d.doc("erp", "Item/Y").is_null());
}

#[test]
fn approved_changes_are_those_of_the_action_decided_in_favour() {
    let end = json!({"approvals": {
        "change/1": {"id": "1", "action": "po", "decision": {"approved": true, "by": "person"}},
        "change/2": {"id": "2", "action": "po", "decision": {"approved": false}},
        "change/3": {"id": "3", "action": "ship", "decision": {"approved": true}},
        "change/4": {"id": "4", "action": "po"}}});
    let ids: Vec<_> = Docs(&end).approved("po").iter().map(|c| &c["id"]).collect();
    assert_eq!(ids, [&json!("1")]);
}

#[test]
fn an_order_is_named_by_its_id_or_by_its_sales_order() {
    let end = json!({"erp": {"Sales Order/SAL-ORD-1": {"po_no": "A-1"}}});
    let d = Docs(&end);
    assert_eq!(d.order_id(&json!("SAL-ORD-1")), "A-1");
    assert_eq!(d.order_id(&json!("A-2")), "A-2");
    assert_eq!(d.order_id(&json!(null)), "");
}

#[test]
fn a_failed_check_says_why_and_a_passed_one_says_nothing() {
    let goal = check("held", false, "B is not held".into());
    assert_eq!(
        (goal.detail.as_str(), goal.safety),
        ("B is not held", false)
    );
    let ok = safety("no_unsafe_release", true, "released".into());
    assert_eq!((ok.detail.as_str(), ok.safety, ok.passed), ("", true, true));
}

#[test]
fn an_option_is_its_what_its_params_and_its_template() {
    let params = vec![ScenarioParam::int("after", "After", (0, 3, 1))];
    let template = json!({"id": "x", "when": {"after_calls": "{after}"}});
    let o = option(
        ("x", "X", "about x", "lims"),
        params,
        template.clone(),
        true,
    );
    let got = (
        o.id.as_str(),
        o.label.as_str(),
        o.description.as_str(),
        o.part.as_str(),
    );
    assert_eq!(got, ("x", "X", "about x", "lims"));
    assert!(o.default);
    assert_eq!((o.params.len(), o.template), (1, template));
}
