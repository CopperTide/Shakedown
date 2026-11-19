//! The CRM's starting records: the ERP's customers as accounts with their contacts, and an open
//! opportunity per sales order, closing when the order is due. An opportunity's name starts with
//! its order's customer PO number, by which the checks find it.
use anyhow::Result;
use arena_part::helpers::twenty::Crm;
use serde_json::{json, Value};

use crate::data::{Contact, Customer, Data, Order};

pub fn opportunity_name(o: &Order) -> String {
    format!("{} Corvexa x {}", o.po, o.qty)
}

pub fn opportunity(d: &Data, o: &Order, account: &str) -> Value {
    let micros = (o.qty as f64 * d.product.price * 1e6).round() as u64;
    json!({"name": opportunity_name(o), "stage": "PROPOSAL", "companyId": account, "closeDate": o.due,
           "amount": {"amountMicros": micros, "currencyCode": d.currency}})
}

pub fn person(c: &Contact, account: &str) -> Value {
    json!({"name": {"firstName": c.first, "lastName": c.last}, "emails": {"primaryEmail": c.email},
           "jobTitle": c.title, "companyId": account})
}

/// The account of a customer; the address names a state only where the customer has one.
pub fn company(c: &Customer) -> Value {
    let country = match c.region.as_str() {
        "US" => "United States",
        _ => c.country.as_deref().unwrap_or_default(),
    };
    let mut address = json!({"addressCity": c.city, "addressCountry": country});
    if let Some(state) = &c.state {
        address["addressState"] = json!(state);
    }
    json!({"name": c.name, "domainName": {"primaryLinkUrl": format!("https://{}", c.domain)},
           "address": address})
}

async fn seed_account(crm: &Crm, d: &Data, c: &Customer) -> Result<()> {
    let id = crm.create("companies", "Company", &company(c)).await?;
    let people: Vec<Value> = c.contacts.iter().map(|p| person(p, &id)).collect();
    crm.create_all("people", &people).await?;
    let deals: Vec<Value> = (d.orders.iter())
        .filter(|o| o.customer == c.name)
        .map(|o| opportunity(d, o, &id))
        .collect();
    crm.create_all("opportunities", &deals).await
}

/// Writes the accounts, contacts and opportunities (`info` is the crm part's `connect` info).
pub async fn seed(info: &Value, d: &Data) -> Result<()> {
    let crm = Crm::connect(info)?;
    for c in &d.customers {
        seed_account(&crm, d, c).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data;

    #[test]
    fn an_opportunity_is_named_by_its_order_and_closes_when_due() {
        let d = data();
        let o = d.order("PO-SBMC-4471").unwrap();
        let deal = opportunity(d, o, "acc");
        assert_eq!(deal["name"], "PO-SBMC-4471 Corvexa x 400");
        assert_eq!(deal["closeDate"], "2026-11-10");
        assert_eq!(deal["amount"]["amountMicros"], 496_000_000_000u64);
        let eu = company(d.customer("Hôpital Saint-Aubin").unwrap());
        assert_eq!(eu["address"]["addressCountry"], "France");
        assert!(eu["address"].get("addressState").is_none());
    }
}
