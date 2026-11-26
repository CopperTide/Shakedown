//! What the analytics summary is filtered by, as the operation's input takes it (JSON, or a query
//! string where a list is comma-separated), and checked into a [`Query`] the store binds.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

/// The runs a summary counts: made in `[from, to)` and matching every list given (any of its
/// values). Only each run's first branch counts; reruns are left out.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct Filter {
    /// RFC 3339: runs made at or after it.
    #[serde(default)]
    pub from: Option<String>,
    /// RFC 3339: runs made before it.
    #[serde(default)]
    pub to: Option<String>,
    /// Worlds (in a query string: comma-separated, or a JSON array).
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub world: Vec<String>,
    /// Scenarios (a run's task).
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub scenario: Vec<String>,
    /// Agent setups, by name.
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub setup: Vec<String>,
    /// Models (a run's model is the one that used the most tokens).
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub model: Vec<String>,
    /// Run statuses: passed, failed, budget_exceeded, stopped, interrupted, infra_error.
    #[serde(default, deserialize_with = "list")]
    #[schemars(with = "Option<String>")]
    pub status: Vec<String>,
    /// What the pass-rate series and the cost points are per: `setup` (default) or `model`.
    #[serde(default)]
    pub group: Option<String>,
}

/// What the series and points are per.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Setup,
    Model,
}

impl Group {
    pub fn as_str(self) -> &'static str {
        match self {
            Group::Setup => "setup",
            Group::Model => "model",
        }
    }
}

/// A [`Filter`] checked and ready to bind.
#[derive(Debug, Clone)]
pub struct Query {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub world: Vec<String>,
    pub scenario: Vec<String>,
    pub setup: Vec<String>,
    pub model: Vec<String>,
    pub status: Vec<String>,
    pub group: Group,
}

impl Filter {
    /// The query of this filter; a bad time or group is an error to show the caller.
    pub fn resolve(self) -> Result<Query, String> {
        let group = match self.group.as_deref().unwrap_or("") {
            "" | "setup" => Group::Setup,
            "model" => Group::Model,
            g => return Err(format!("group is setup or model, not {g:?}")),
        };
        Ok(Query {
            from: time(self.from.as_deref(), "from")?,
            to: time(self.to.as_deref(), "to")?,
            world: self.world,
            scenario: self.scenario,
            setup: self.setup,
            model: self.model,
            status: self.status,
            group,
        })
    }
}

/// The pass-rate series' bucket for runs made from `first` to `last`: a day for up to 90 days,
/// else a week.
pub fn bucket_for(first: DateTime<Utc>, last: DateTime<Utc>) -> &'static str {
    if last - first <= chrono::Duration::days(90) {
        "day"
    } else {
        "week"
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

/// A list as a JSON array of strings, or as one query-string value: a JSON array (for names with
/// commas, as the Analytics page sends them) or comma-separated text.
fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum List {
        Many(Vec<String>),
        One(String),
    }
    let items = match Option::<List>::deserialize(d)? {
        None => vec![],
        Some(List::Many(v)) => v,
        Some(List::One(s)) if s.trim_start().starts_with('[') => {
            serde_json::from_str(&s).map_err(serde::de::Error::custom)?
        }
        Some(List::One(s)) => s.split(',').map(str::to_string).collect(),
    };
    Ok(items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(v: serde_json::Value) -> Result<Query, String> {
        serde_json::from_value::<Filter>(v).unwrap().resolve()
    }

    #[test]
    fn lists_come_as_arrays_or_comma_separated_text() {
        let q = resolve(json!({"setup": "a, b,", "model": ["m"], "status": ""})).unwrap();
        assert_eq!(q.setup, ["a", "b"]);
        assert_eq!(q.model, ["m"]);
        assert!(q.status.is_empty());
        assert_eq!(q.group, Group::Setup);
        let q = resolve(json!({"setup": r#"["Team (lead, two helpers)", "b"]"#})).unwrap();
        assert_eq!(q.setup, ["Team (lead, two helpers)", "b"]);
    }

    #[test]
    fn bad_times_and_groups_are_refused() {
        assert!(resolve(json!({"from": "yesterday"})).is_err());
        assert!(resolve(json!({"group": "colour"})).is_err());
        assert_eq!(
            resolve(json!({"group": "model"})).unwrap().group,
            Group::Model
        );
        let q = resolve(json!({"from": "2026-11-01T00:00:00Z"})).unwrap();
        assert_eq!(q.from.unwrap().to_rfc3339(), "2026-11-01T00:00:00+00:00");
    }

    #[test]
    fn the_bucket_is_a_day_up_to_ninety_days_else_a_week() {
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let first = t("2026-01-01T00:00:00Z");
        assert_eq!(bucket_for(first, first), "day");
        assert_eq!(bucket_for(first, t("2026-03-31T00:00:00Z")), "day");
        assert_eq!(bucket_for(first, t("2026-06-01T00:00:00Z")), "week");
    }
}
