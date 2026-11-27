//! Each model call's usage (`call_usage`, migration 9), kept by the analytics job with its branch's
//! figures in one transaction: the figures are current exactly when the calls are.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{analytics::PUT, Store, Tx};
use crate::{analytics::CallUsage, auth::SystemScope};

/// Replaces branch `$1`'s calls with the rows of the arrays `$2`–`$12`.
const INSERT: &str =
    "INSERT INTO call_usage (branch_id, seq, at, agent, provider, model, input_tokens,
       output_tokens, cached_tokens, reasoning_tokens, cost_usd, think_ms)
SELECT $1, * FROM unnest($2::bigint[], $3::timestamptz[], $4::text[], $5::text[], $6::text[],
       $7::bigint[], $8::bigint[], $9::bigint[], $10::bigint[], $11::float8[], $12::float8[])";

async fn replace_calls(tx: &Tx, id: &str, calls: &[CallUsage]) -> Result<()> {
    tx.execute("DELETE FROM call_usage WHERE branch_id = $1", &[&id])
        .await?;
    if calls.is_empty() {
        return Ok(());
    }
    let col = |f: fn(&CallUsage) -> Option<i64>| calls.iter().map(f).collect::<Vec<_>>();
    let text = |f: fn(&CallUsage) -> Option<String>| calls.iter().map(f).collect::<Vec<_>>();
    let real = |f: fn(&CallUsage) -> Option<f64>| calls.iter().map(f).collect::<Vec<_>>();
    let seq: Vec<i64> = calls.iter().map(|c| c.seq).collect();
    let at: Vec<DateTime<Utc>> = calls.iter().map(|c| c.at).collect();
    let (agent, provider, model) = (
        text(|c| c.agent.clone()),
        text(|c| c.provider.clone()),
        text(|c| c.model.clone()),
    );
    let (input, output) = (col(|c| c.input), col(|c| c.output));
    let (cached, reasoning) = (col(|c| c.cached), col(|c| c.reasoning));
    let (cost, think) = (real(|c| c.cost_usd), real(|c| c.think_ms));
    tx.execute(
        INSERT,
        &[
            &id, &seq, &at, &agent, &provider, &model, &input, &output, &cached, &reasoning, &cost,
            &think,
        ],
    )
    .await?;
    Ok(())
}

impl Store {
    /// Keeps branch `id`'s figures and its model calls' usage, if it still ended at `ended_at`
    /// (a system job): false, and nothing kept, when it ended again since.
    pub async fn put_figures(
        &self,
        id: &str,
        ended_at: DateTime<Utc>,
        stats: &Value,
        calls: &[CallUsage],
    ) -> Result<bool> {
        let tx = self.system_tx(SystemScope::analytics_stats()).await?;
        if tx.execute(PUT, &[&id, stats, &ended_at]).await? != 1 {
            return Ok(false);
        }
        replace_calls(&tx, id, calls).await?;
        tx.commit().await?;
        Ok(true)
    }
}
