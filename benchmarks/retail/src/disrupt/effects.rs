//! What the disruptions that fired on a call do to its answer.

use anyhow::Result;
use serde_json::{json, Value};
use tokio_postgres::Client;

use super::Disruption;
use crate::tools::{self, ToolOutcome};

/// The answer of a call a `timeout` disruption fired on (the call isn't made).
pub const TIMED_OUT: &str = "The request timed out; it may not have been processed.";

/// Answers the call of `name` as the disruptions `fired` on it make it: not made (`timeout`),
/// made but not written (`lost_write`), or made with its result misreported.
pub async fn answer(
    c: &mut Client,
    name: &str,
    args: &Value,
    fired: &[Disruption],
) -> Result<ToolOutcome> {
    let fires = |kind: &str| fired.iter().any(|d| d.kind() == kind);
    if fires("timeout") {
        return Ok(timed_out());
    }
    let mut out = match fires("lost_write") {
        true => tools::call_unwritten(c, name, args).await?,
        false => tools::call(c, name, args).await?,
    };
    if let Some(result) = out.result.as_mut() {
        fired.iter().for_each(|d| misreport(result, d));
    }
    Ok(out)
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

/// An order's status as it was before its last change: delivered reads pending and pending
/// reads delivered (others as they are).
fn stale(order: &mut Value) {
    let was = match order["status"].as_str() {
        Some("delivered") => "pending",
        Some("pending") => "delivered",
        _ => return,
    };
    order["status"] = json!(was);
}

/// The product's variant `item` (else its first in stock, by id) reads out of stock.
fn unavailable(product: &mut Value, item: Option<&str>) {
    let Some(variants) = product["variants"].as_object_mut() else {
        return;
    };
    let in_stock = || variants.iter().find(|(_, v)| v["available"] == true);
    let id = item
        .map(str::to_string)
        .or_else(|| in_stock().map(|(k, _)| k.clone()));
    if let Some(v) = id.and_then(|id| variants.get_mut(&id)) {
        v["available"] = json!(false);
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
        let mut named = p;
        unavailable(&mut named, Some("3"));
        assert_eq!(named["variants"]["2"]["available"], true);
        assert_eq!(named["variants"]["3"]["available"], false);
    }
}
