//! The explore schema as the database describes it: each view and column with its type and the
//! comment migration `shakedown 9` gives it, in [`VIEWS`] order, plus the functions a query may
//! call, the limits and example queries. The SQL tab's reference, the assistant and
//! docs/analytics.md read the same descriptions.

use serde::Serialize;

use super::VIEWS;

/// One documented column.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColumnDoc {
    pub name: String,
    #[serde(rename = "type")]
    pub pg_type: String,
    pub description: String,
}

/// One documented view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ViewDoc {
    pub name: String,
    pub description: String,
    pub columns: Vec<ColumnDoc>,
}

/// Every column of every view of schema `$1`, with its view's and its own comments, by view.
pub const DESCRIBE: &str = "SELECT c.relname::text, coalesce(obj_description(c.oid, 'pg_class'), ''),
       a.attname::text, format_type(a.atttypid, a.atttypmod), coalesce(col_description(c.oid, a.attnum), '')
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  JOIN pg_attribute a ON a.attrelid = c.oid
 WHERE n.nspname = $1 AND c.relkind = 'v' AND a.attnum > 0 AND NOT a.attisdropped
 ORDER BY c.relname, a.attnum";

/// The views from [`DESCRIBE`]'s rows `(view, view comment, column, type, column comment)`, in
/// [`VIEWS`] order.
pub fn views_of(
    rows: impl IntoIterator<Item = (String, String, String, String, String)>,
) -> Vec<ViewDoc> {
    let mut views: Vec<ViewDoc> = Vec::new();
    for (view, about, name, pg_type, description) in rows {
        if views.last().is_none_or(|v| v.name != view) {
            views.push(ViewDoc {
                name: view,
                description: about,
                columns: Vec::new(),
            });
        }
        let last = views.last_mut().expect("a view was just pushed");
        last.columns.push(ColumnDoc {
            name,
            pg_type,
            description,
        });
    }
    views.sort_by_key(|v| {
        VIEWS
            .iter()
            .position(|n| *n == v.name)
            .unwrap_or(usize::MAX)
    });
    views
}

/// An example query, for the SQL tab and the assistant.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Example {
    pub name: &'static str,
    pub sql: &'static str,
}

#[rustfmt::skip]
pub const EXAMPLES: &[Example] = &[
    Example { name: "Cost and tokens by agent", sql: "SELECT agent, count(*) AS calls, sum(input_tokens) AS input_tokens,\n       sum(output_tokens) AS output_tokens, round(sum(cost_usd)::numeric, 4) AS cost_usd\n  FROM model_calls\n GROUP BY agent\n ORDER BY cost_usd DESC NULLS LAST" },
    Example { name: "Refusals by tool", sql: "SELECT tool, count(*) FILTER (WHERE outcome = 'refused') AS refusals,\n       count(*) FILTER (WHERE outcome = 'error') AS errors, count(*) AS calls\n  FROM tool_calls\n GROUP BY tool\n ORDER BY refusals DESC, calls DESC" },
    Example { name: "Pass rate by setup", sql: "SELECT setup, count(*) AS runs, avg(passed::int) AS pass_rate,\n       round(avg(cost_usd)::numeric, 4) AS cost_per_run\n  FROM runs\n WHERE parent_branch_id IS NULL AND ended_at IS NOT NULL\n GROUP BY setup\n ORDER BY pass_rate DESC NULLS LAST" },
    Example { name: "Slowest model calls", sql: "SELECT branch_id, step, agent, model, round(think_ms) AS think_ms, input_tokens\n  FROM model_calls\n WHERE think_ms IS NOT NULL\n ORDER BY think_ms DESC\n LIMIT 50" },
    Example { name: "Steps per day by kind", sql: "SELECT date_trunc('day', at) AS day, kind, count(*) AS steps\n  FROM steps\n GROUP BY 1, 2\n ORDER BY 1, 3 DESC" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_make_views_in_the_documented_order_and_examples_pass_the_guard() {
        let row = |v: &str, c: &str| {
            (
                v.to_string(),
                format!("{v}."),
                c.to_string(),
                "text".to_string(),
                String::new(),
            )
        };
        let views = views_of([
            row("agents", "agent"),
            row("runs", "run_id"),
            row("runs", "name"),
        ]);
        let names: Vec<_> = views.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["runs", "agents"]);
        assert_eq!(views[0].columns.len(), 2);
        for e in EXAMPLES {
            super::super::guard::check(e.sql).unwrap_or_else(|why| panic!("{}: {why}", e.name));
        }
    }
}
