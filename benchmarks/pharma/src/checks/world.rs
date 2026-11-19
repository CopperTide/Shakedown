//! The world's end state, `{part: {key: doc}}`, read the way the checks need it: the approvals'
//! decided changes, the ERP's batches and orders, the carriers' shipments and notices, the CRM.
use arena_part::helpers::bench::Docs;
use serde_json::Value;

use crate::days::day;

pub struct World<'a>(pub Docs<'a>);

impl<'a> std::ops::Deref for World<'a> {
    type Target = Docs<'a>;

    fn deref(&self) -> &Docs<'a> {
        &self.0
    }
}

/// A shipment booked with a carrier, as the checks read it.
pub struct Booking<'a> {
    pub doc: &'a Value,
    /// The order's id (its customer PO number), however the booking named it.
    pub order: &'a str,
    pub batch: &'a str,
    pub customer: &'a str,
    pub carrier: &'a str,
    pub origin: &'a str,
    pub qty: u64,
    pub ship: Option<i64>,
    pub arrives: Option<i64>,
}

impl<'a> World<'a> {
    /// The changes of `action` a person approved.
    pub fn approved(&self, action: &str) -> Vec<&'a Value> {
        let by_person = |c: &&Value| c["decision"]["by"] == "person";
        let approved = self.0.approved(action).into_iter();
        approved.filter(by_person).collect()
    }

    /// Whether a person approved a change of `action` whose payload names `batch`.
    pub fn approved_for(&self, action: &str, batch: &str) -> bool {
        self.approved(action)
            .iter()
            .any(|c| c["payload"]["batch"] == batch)
    }

    /// The change `id`, approved by a person, of `action`.
    pub fn is_approved(&self, id: &Value, action: &str) -> bool {
        self.approved(action).iter().any(|c| c["id"] == *id)
    }

    /// The batch's QA status in the ERP at the end.
    pub fn qa_status(&self, batch: &str) -> &'a str {
        let doc = self.doc("erp", &format!("Batch/{batch}"));
        doc["qa_status"].as_str().unwrap_or_default()
    }

    pub fn bookings(&self) -> Vec<Booking<'a>> {
        let text = |d: &'a Value, f: &str| d[f].as_str().unwrap_or_default();
        let shipments = self.docs("logistics", "shipment");
        shipments
            .into_iter()
            .map(|d| Booking {
                doc: d,
                order: self.order_id(&d["order"]),
                batch: text(d, "batch"),
                customer: text(d, "customer"),
                carrier: text(d, "carrier"),
                origin: text(d, "origin"),
                qty: d["qty"].as_u64().unwrap_or(0),
                ship: day(text(d, "ship_date")),
                arrives: day(text(d, "arrives")),
            })
            .collect()
    }

    /// The latest notice sent about `order` to `customer`.
    pub fn notice(&self, order: &str, customer: &str) -> Option<&'a Value> {
        let notices = self.docs("logistics", "notice");
        let about = |n: &&&Value| self.order_id(&n["order"]) == order && n["customer"] == customer;
        notices.iter().rev().find(about).copied()
    }
}
