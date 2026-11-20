//! Usage and quotas in the metadata database: an idempotent metering ledger, monthly totals kept
//! in the same transaction, and limits (a quotas row, else the org's plan preset).

use anyhow::Result;
use chrono::NaiveDate;

use super::Scoped;
use crate::quotas::{verdict, Limit, Meter, Period, Presets, Verdict};

const INSERT_USAGE: &str = "INSERT INTO usage (org_id, user_id, run_id, branch_id, meter, quantity, source, idempotency_key)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (idempotency_key) DO NOTHING RETURNING id";
const ADD_MONTHLY: &str = "INSERT INTO usage_monthly (org_id, month, meter, quantity)
     VALUES ($1, date_trunc('month', now())::date, $2, $3)
     ON CONFLICT (org_id, month, meter) DO UPDATE SET quantity = usage_monthly.quantity + EXCLUDED.quantity";

impl Scoped<'_> {
    /// The org's limit on `m`: its quotas row, else its plan's preset; None: unlimited.
    pub async fn limit(&self, m: Meter) -> Result<Option<Limit>> {
        let sql = "SELECT limit_value, hard, period FROM quotas WHERE org_id = $1 AND meter = $2";
        if let Some(r) = self.query_opt(sql, &[&self.org(), &m.as_str()]).await? {
            let period = if r.get::<_, &str>(2) == "month" {
                Period::Month
            } else {
                Period::None
            };
            return Ok(Some(Limit {
                value: r.get::<_, i64>(0).max(0) as u64,
                hard: r.get(1),
                period,
            }));
        }
        let plan = self
            .query_one(
                "SELECT plan FROM identity.orgs WHERE id = $1",
                &[&self.org()],
            )
            .await?;
        Ok(Presets::active().limit(plan.get(0), m))
    }

    /// Records `qty` of `m` once under `key` (a resend counts once), with the month's total in the
    /// same transaction: false when `key` was recorded already.
    pub async fn meter(
        &self,
        m: Meter,
        qty: i64,
        source: &str,
        key: &str,
        run: Option<&str>,
        branch: Option<&str>,
    ) -> Result<bool> {
        let tx = self.store.tx(self.scope).await?;
        let params: [&(dyn tokio_postgres::types::ToSql + Sync); 8] = [
            &self.org(),
            &self.user(),
            &run,
            &branch,
            &m.as_str(),
            &qty,
            &source,
            &key,
        ];
        let new = tx.query_opt(INSERT_USAGE, &params).await?.is_some();
        if new {
            tx.execute(ADD_MONTHLY, &[&self.org(), &m.as_str(), &qty])
                .await?;
        }
        tx.commit().await?;
        Ok(new)
    }

    /// Whether adding `adding` of `m` stays within the org's limit (this month's total for a
    /// monthly one).
    pub async fn check(&self, m: Meter, adding: u64) -> Result<Verdict> {
        let limit = self.limit(m).await?;
        if limit.is_none() {
            return Ok(Verdict::Ok);
        }
        let sql = "SELECT coalesce(sum(quantity), 0)::bigint FROM usage_monthly
                    WHERE org_id = $1 AND meter = $2 AND month = date_trunc('month', now())::date";
        let used: i64 = self
            .query_one(sql, &[&self.org(), &m.as_str()])
            .await?
            .get(0);
        Ok(verdict(m, limit, used.max(0) as u64, adding))
    }

    /// The monthly totals of what the scope's own user started, newest month first.
    pub async fn own_usage_by_month(&self) -> Result<Vec<(NaiveDate, Meter, i64)>> {
        let sql = "SELECT date_trunc('month', at)::date, meter, sum(quantity)::bigint FROM usage
                    WHERE org_id = $1 AND user_id = $2 GROUP BY 1, 2 ORDER BY 1 DESC, 2";
        let rows = self.query(sql, &[&self.org(), &self.user()]).await?;
        Ok(rows
            .iter()
            .filter_map(|r| Some((r.get(0), Meter::parse(r.get(1))?, r.get(2))))
            .collect())
    }

    /// The org's monthly totals, newest month first.
    pub async fn usage_by_month(&self) -> Result<Vec<(NaiveDate, Meter, i64)>> {
        let sql = "SELECT month, meter, quantity FROM usage_monthly WHERE org_id = $1 ORDER BY month DESC, meter";
        let rows = self.query(sql, &[&self.org()]).await?;
        Ok(rows
            .iter()
            .filter_map(|r| Some((r.get(0), Meter::parse(r.get(1))?, r.get(2))))
            .collect())
    }
}
