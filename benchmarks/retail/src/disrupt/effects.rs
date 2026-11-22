//! How the disruptions that may count a call make its answer: whether the call is made, made
//! without writing, and which of them it changes (only those count it or fire on it).

use anyhow::Result;
use serde_json::{json, Value};
use tokio_postgres::Client;

use super::Disruption;
use crate::tools::{self, ToolOutcome};

/// The answer of a call a `timeout` disruption fired on (the call isn't made).
pub const TIMED_OUT: &str = "The request timed out; it may not have been processed.";

/// The call of `name` as the disruptions `counting` it would leave it, before any misreport: not
/// made when a `timeout` is due, made without writing when a `lost_write` is due, else made.
pub async fn call(
    c: &mut Client,
    name: &str,
    args: &Value,
    counting: &[&Disruption],
) -> Result<ToolOutcome> {
    let due = |kind: &str| counting.iter().any(|d| d.due() && d.kind() == kind);
    if due("timeout") {
        return Ok(timed_out());
    }
    match due("lost_write") {
        true => tools::call_unwritten(c, name, args).await,
        false => tools::call(c, name, args).await,
    }
}

/// True when `d` changes the call answered `out`: a timeout any call; a lost write a write that
/// succeeded; a stale status an order delivered or pending; an out-of-stock variant a product
/// with that variant (or one) in stock.
pub fn alters(d: &Disruption, out: &ToolOutcome) -> bool {
    let result = out.result.as_ref().filter(|_| out.error.is_none());
    match (d.kind(), result) {
        ("timeout", _) => true,
        ("lost_write", r) => r.is_some(),
        ("stale_status", Some(r)) => stale_of(r).is_some(),
        ("variant_unavailable", Some(r)) => in_stock(r, d.act["item_id"].as_str()).is_some(),
        _ => false,
    }
}

/// `out` with its result misreported as the disruptions `fired` on it make it.
pub fn misreported(mut out: ToolOutcome, fired: &[Disruption]) -> ToolOutcome {
    if let Some(result) = out.result.as_mut() {
        fired.iter().for_each(|d| misreport(result, d));
    }
    out
}

fn timed_out() -> ToolOutcome {
    ToolOutcome {
        result: None,
        error: Some(TIMED_OUT.into()),
        reads: vec![],
        writes: vec![],
    }
}

fn misreport(result: &mut Value, d: &Disruption) {
    match d.kind() {
        "stale_status" => stale(result),
        "variant_unavailable" => unavailable(result, d.act["item_id"].as_str()),
        _ => {}
    }
}

/// The status an order's stale read shows: pending for delivered, delivered for pending (none
/// for any other status).
fn stale_of(order: &Value) -> Option<&'static str> {
    match order["status"].as_str() {
        Some("delivered") => Some("pending"),
        Some("pending") => Some("delivered"),
        _ => None,
    }
}

fn stale(order: &mut Value) {
    if let Some(was) = stale_of(order) {
        order["status"] = json!(was);
    }
}

/// The id of the product's variant `item` when it is in stock, else (no `item`) of its first
/// variant in stock, by id.
fn in_stock(product: &Value, item: Option<&str>) -> Option<String> {
    let variants = product["variants"].as_object()?;
    let available = |v: &Value| v["available"] == true;
    match item {
        Some(id) => variants
            .get(id)
            .filter(|v| available(v))
            .map(|_| id.to_string()),
        None => variants
            .iter()
            .find(|(_, v)| available(v))
            .map(|(k, _)| k.clone()),
    }
}

/// That variant reads out of stock.
fn unavailable(product: &mut Value, item: Option<&str>) {
    if let Some(id) = in_stock(product, item) {
        product["variants"][id]["available"] = json!(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_status_swaps_delivered_and_pending_only() {
        let swap = |s: &str| {
            let mut o = json!({"status": s});
            stale(&mut o);
            o["status"].as_str().unwrap().to_string()
        };
        assert_eq!(swap("delivered"), "pending");
        assert_eq!(swap("pending"), "delivered");
        assert_eq!(swap("cancelled"), "cancelled");
    }

    #[test]
    fn the_first_variant_in_stock_or_the_one_named_reads_out_of_stock() {
        let p = json!({"variants": {"1": {"available": false}, "2": {"available": true},
                                    "3": {"available": true}}});
        let mut first = p.clone();
        unavailable(&mut first, None);
        assert_eq!(first["variants"]["2"]["available"], false);
        assert_eq!(first["variants"]["3"]["available"], true);
        let mut named = p.clone();
        unavailable(&mut named, Some("3"));
        assert_eq!(named["variants"]["2"]["available"], true);
        assert_eq!(named["variants"]["3"]["available"], false);
        assert_eq!(
            in_stock(&p, Some("1")),
            None,
            "a variant out of stock can't be misreported"
        );
        let none = json!({"variants": {"1": {"available": false}}});
        assert_eq!(in_stock(&none, None), None);
    }

    fn outcome(result: Option<Value>, error: Option<&str>) -> ToolOutcome {
        ToolOutcome {
            result,
            error: error.map(str::to_string),
            reads: vec![],
            writes: vec![],
        }
    }

    fn of(kind: &str) -> Disruption {
        let d = json!({"id": "d", "when": {"after_calls": 0, "tool": "get_order_details"},
                       "do": {"kind": kind}, "text": "t"});
        super::super::checked(&d).unwrap()
    }

    #[test]
    fn only_a_call_a_fault_changes_counts_for_it() {
        let order = |s: &str| outcome(Some(json!({"status": s})), None);
        assert!(alters(&of("stale_status"), &order("delivered")));
        assert!(!alters(&of("stale_status"), &order("cancelled")));
        let failed = outcome(None, Some("Order not found"));
        assert!(!alters(&of("stale_status"), &failed));
        assert!(
            !alters(&of("lost_write"), &failed),
            "a write that failed anyway"
        );
        assert!(alters(&of("lost_write"), &order("delivered")));
        assert!(alters(&of("timeout"), &failed));
        let sold_out = outcome(Some(json!({"variants": {"1": {"available": false}}})), None);
        assert!(!alters(&of("variant_unavailable"), &sold_out));
    }
}
