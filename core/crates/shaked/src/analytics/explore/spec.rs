//! What the builder is asked: measures, up to two dimensions and filters, as the operation takes
//! it, checked into a [`Plan`] the store binds.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::catalog::{self, Dimension, Level, Measure};

/// The most measures, and dimensions, one query takes.
pub const MAX_MEASURES: usize = 8;
pub const MAX_DIMENSIONS: usize = 2;
/// The groups a query returns by default, and at most.
pub const DEFAULT_LIMIT: i64 = 500;
pub const MAX_LIMIT: i64 = 1000;

/// The runs a query counts: each run's first branch with worked-out figures, started in
/// `[from, to)`, of any experiment, setup and agent given.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Filters {
    /// RFC 3339: runs started at or after it.
    #[serde(default)]
    pub from: Option<String>,
    /// RFC 3339: runs started before it.
    #[serde(default)]
    pub to: Option<String>,
    /// Experiment ids.
    #[serde(default)]
    pub experiment: Vec<String>,
    /// Setup names.
    #[serde(default)]
    pub setup: Vec<String>,
    /// Agent ids: runs they took part in, and only their steps.
    #[serde(default)]
    pub agent: Vec<String>,
}

/// A builder query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Spec {
    /// Measure ids (1 to 8): runs, pass_rate, cost, tokens_in, tokens_out, tokens_cached,
    /// model_calls, tool_calls, refusals, errors, duration, thinking, steps.
    pub measures: Vec<String>,
    /// Dimension ids (0 to 2): agent, model, setup, scenario, world, experiment, tool, kind, day.
    #[serde(default)]
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub filters: Filters,
    /// The most groups to return (default 500, at most 1000).
    #[serde(default)]
    pub limit: Option<i64>,
}

/// A [`Spec`] checked and ready to bind.
#[derive(Debug, Clone)]
pub struct Plan {
    pub measures: Vec<&'static Measure>,
    pub dimensions: Vec<&'static Dimension>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub experiment: Vec<String>,
    pub setup: Vec<String>,
    pub agent: Vec<String>,
    pub limit: i64,
}

impl Plan {
    /// True when the query reads the runs' steps: a step-level measure or dimension.
    pub fn needs_steps(&self) -> bool {
        self.measures.iter().any(|m| m.level == Level::Step) || self.has_step_dimension()
    }

    pub fn has_step_dimension(&self) -> bool {
        self.dimensions.iter().any(|d| d.level == Level::Step)
    }
}

fn pick<T>(
    ids: &[String],
    what: &str,
    find: fn(&str) -> Option<&'static T>,
) -> Result<Vec<&'static T>, String> {
    let mut out: Vec<&'static T> = Vec::new();
    for id in ids {
        let found = find(id).ok_or_else(|| format!("no {what} {id:?}"))?;
        if !out.iter().any(|t| std::ptr::eq(*t, found)) {
            out.push(found);
        }
    }
    Ok(out)
}

impl Spec {
    /// The plan of this spec; an unknown name, a bad time or too many picks is an error to show
    /// the caller.
    pub fn resolve(self) -> Result<Plan, String> {
        let measures = pick(&self.measures, "measure", catalog::measure)?;
        let dimensions = pick(&self.dimensions, "dimension", catalog::dimension)?;
        if measures.is_empty() || measures.len() > MAX_MEASURES {
            return Err(format!("pick 1 to {MAX_MEASURES} measures"));
        }
        if dimensions.len() > MAX_DIMENSIONS {
            return Err(format!("pick at most {MAX_DIMENSIONS} dimensions"));
        }
        let f = self.filters;
        Ok(Plan {
            measures,
            dimensions,
            from: time(f.from.as_deref(), "from")?,
            to: time(f.to.as_deref(), "to")?,
            experiment: f.experiment,
            setup: f.setup,
            agent: f.agent,
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }
}

fn time(v: Option<&str>, name: &str) -> Result<Option<DateTime<Utc>>, String> {
    let Some(v) = v.filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(v)
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|e| format!("{name}: {e} (expected an RFC 3339 time)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(v: serde_json::Value) -> Result<Plan, String> {
        serde_json::from_value::<Spec>(v).unwrap().resolve()
    }

    #[test]
    fn a_spec_names_known_measures_and_dimensions_once_each() {
        let p = resolve(json!({"measures": ["cost", "runs", "cost"], "dimensions": ["setup"]}))
            .unwrap();
        let ids: Vec<_> = p.measures.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["cost", "runs"]);
        assert!(p.needs_steps() && !p.has_step_dimension());
        assert_eq!(p.limit, DEFAULT_LIMIT);
        assert!(
            !resolve(json!({"measures": ["runs"], "dimensions": ["day"]}))
                .unwrap()
                .needs_steps()
        );
    }

    #[test]
    fn bad_specs_are_refused() {
        assert!(resolve(json!({"measures": []})).is_err());
        assert!(resolve(json!({"measures": ["colour"]})).is_err());
        assert!(
            resolve(json!({"measures": ["runs"], "dimensions": ["agent", "tool", "day"]})).is_err()
        );
        assert!(resolve(json!({"measures": ["runs"], "filters": {"from": "yesterday"}})).is_err());
        assert_eq!(
            resolve(json!({"measures": ["runs"], "limit": 99999}))
                .unwrap()
                .limit,
            MAX_LIMIT
        );
    }
}
