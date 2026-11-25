//! The schedule in the retail database: table `_disruption` (`id`, `doc`), made when the first
//! disruption is armed. Its rows are the part's internal keys `_disruption/<id>`, which a save
//! keeps and a comparison of worlds leaves out.

use anyhow::Result;
use tokio_postgres::Client;

use super::Disruption;

const TABLE: &str = "_disruption";

/// Every disruption armed, in id order; none when the table isn't there.
pub async fn load(c: &Client) -> Result<Vec<Disruption>> {
    let exists = c.query_one("SELECT to_regclass($1) IS NOT NULL", &[&TABLE]);
    if !exists.await?.get::<_, bool>(0) {
        return Ok(vec![]);
    }
    let rows = c
        .query(&format!("SELECT doc FROM {TABLE} ORDER BY id"), &[])
        .await?;
    let docs = rows.iter().map(|r| r.get::<_, serde_json::Value>(0));
    Ok(docs.map(serde_json::from_value).collect::<Result<_, _>>()?)
}

/// Drops the disruptions `ids` (an id not armed is no change).
pub async fn remove(c: &Client, ids: &[String]) -> Result<()> {
    if ids.is_empty() || load(c).await?.is_empty() {
        return Ok(());
    }
    let delete = format!("DELETE FROM {TABLE} WHERE id = ANY($1)");
    c.execute(&delete, &[&ids]).await?;
    Ok(())
}

/// Writes `changed` back (adding new ones).
pub async fn save(c: &Client, changed: &[Disruption]) -> Result<()> {
    if changed.is_empty() {
        return Ok(());
    }
    let create =
        format!("CREATE TABLE IF NOT EXISTS {TABLE} (id text PRIMARY KEY, doc jsonb NOT NULL)");
    c.batch_execute(&create).await?;
    let upsert = format!(
        "INSERT INTO {TABLE} (id, doc) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET doc = EXCLUDED.doc"
    );
    for d in changed {
        c.execute(&upsert, &[&d.id, &serde_json::to_value(d)?])
            .await?;
    }
    Ok(())
}
