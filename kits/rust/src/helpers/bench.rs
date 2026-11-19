//! What a benchmark's checks and scenario options share: the world's end state (`{part: {key:
//! doc}}`) read by part and kind, goal and safety checks, and a disruption option.
use serde_json::Value;

use crate::proto::{DisruptionOption, NamedCheck, ScenarioParam};

/// The end state, `{part: {key: doc}}`.
#[derive(Debug, Clone, Copy)]
pub struct Docs<'a>(pub &'a Value);

impl<'a> Docs<'a> {
    /// The documents of `part` whose keys start with `<kind>/`, in key order.
    pub fn docs(&self, part: &str, kind: &str) -> Vec<&'a Value> {
        let prefix = format!("{kind}/");
        let mut docs: Vec<(&String, &Value)> = (self.0[part].as_object().into_iter().flatten())
            .filter(|(k, _)| k.starts_with(&prefix))
            .collect();
        docs.sort_by_key(|(k, _)| k.as_str());
        docs.into_iter().map(|(_, d)| d).collect()
    }

    /// The document `key` of `part` (null when it has none).
    pub fn doc(&self, part: &str, key: &str) -> &'a Value {
        &self.0[part][key]
    }

    /// The approvals' changes of `action` decided in favour, by anyone.
    pub fn approved(&self, action: &str) -> Vec<&'a Value> {
        let approved = |c: &&Value| c["action"] == action && c["decision"]["approved"] == true;
        self.docs("approvals", "change")
            .into_iter()
            .filter(approved)
            .collect()
    }

    /// The order's id (its ERP sales order's `po_no`) of an order named either by that id or by
    /// its sales order: both identify it.
    pub fn order_id(&self, named: &'a Value) -> &'a str {
        let name = named.as_str().unwrap_or_default();
        let sales_order = &self.0["erp"][format!("Sales Order/{name}")];
        sales_order["po_no"].as_str().unwrap_or(name)
    }
}

/// A goal check: its detail is `fails_with` when it failed.
pub fn check(name: &str, passed: bool, fails_with: String) -> NamedCheck {
    NamedCheck::new(name, passed, fails_with, false)
}

/// A hard safety check: an unsafe action, reported apart from the score.
pub fn safety(name: &str, passed: bool, fails_with: String) -> NamedCheck {
    NamedCheck::new(name, passed, fails_with, true)
}

/// A disruption option: `(id, label, description, part)`, its params, its template, and whether
/// the builder offers it selected.
pub fn option(
    (id, label, description, part): (&str, &str, &str, &str),
    params: Vec<ScenarioParam>,
    template: Value,
    default: bool,
) -> DisruptionOption {
    DisruptionOption {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        part: part.into(),
        default,
        params,
        template,
    }
}
