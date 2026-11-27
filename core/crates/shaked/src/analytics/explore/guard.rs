//! The SQL guard: a query is run only when a real SQL parser reads it as exactly one SELECT (or
//! WITH … SELECT) that names nothing but the explore views (or its own CTEs) and calls only
//! allowlisted functions. What runs is the parsed query printed back, never the text as sent, so
//! the database executes exactly what was checked. The guard is one wall of several
//! (`super::sql`): the transaction is read-only and timed, its search_path is the explore schema,
//! and the explore role can read nothing else.

use std::{collections::BTreeSet, ops::ControlFlow};

use sqlparser::{
    ast::{
        Expr, FunctionArguments, Ident, ObjectName, ObjectNamePart, Query, Select, SetExpr,
        Statement, TableFactor, Visit, Visitor,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use super::{SCHEMA, VIEWS};

/// The longest query accepted, in bytes.
pub const MAX_SQL_BYTES: usize = 20_000;

/// The functions a query may call: aggregates, window functions and scalar functions over
/// numbers, text, times and JSON values. None reads a file, a setting, another table or the
/// catalog, sleeps, or changes anything.
#[rustfmt::skip]
pub const FUNCTIONS: &[&str] = &[
    // aggregates
    "count", "sum", "avg", "min", "max", "stddev", "stddev_pop", "stddev_samp", "variance",
    "var_pop", "var_samp", "percentile_cont", "percentile_disc", "mode", "bool_and", "bool_or",
    "every", "string_agg", "array_agg", "json_agg", "jsonb_agg", "corr", "covar_pop",
    "covar_samp", "regr_slope", "regr_intercept", "regr_r2",
    // window functions
    "row_number", "rank", "dense_rank", "percent_rank", "cume_dist", "ntile", "lag", "lead",
    "first_value", "last_value", "nth_value",
    // numbers
    "abs", "ceil", "ceiling", "floor", "round", "trunc", "sign", "sqrt", "cbrt", "power", "exp",
    "ln", "log", "log10", "mod", "div", "greatest", "least", "width_bucket", "to_number",
    // conditionals
    "coalesce", "nullif",
    // text
    "lower", "upper", "length", "char_length", "left", "right", "substr", "substring",
    "concat", "concat_ws", "replace", "split_part", "strpos", "position", "starts_with", "trim",
    "ltrim", "rtrim", "btrim", "initcap", "lpad", "rpad", "reverse", "regexp_replace",
    "regexp_match", "format",
    // times
    "now", "current_date", "current_timestamp", "localtimestamp", "date_trunc", "date_part",
    "date_bin", "extract", "age", "to_char", "to_timestamp", "to_date", "make_interval",
    "justify_interval", "isfinite",
    // JSON values and arrays
    "jsonb_typeof", "jsonb_array_length", "jsonb_extract_path", "jsonb_extract_path_text",
    "jsonb_array_elements", "jsonb_array_elements_text", "jsonb_object_keys",
    "jsonb_build_object", "jsonb_build_array", "to_jsonb", "array_length", "cardinality",
    "array_to_string", "array_position", "unnest",
];

/// A query the guard passed: the parsed SELECT printed back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub sql: String,
}

/// Checks `sql`: the query to run, or why it may not run.
pub fn check(sql: &str) -> Result<Checked, String> {
    if sql.len() > MAX_SQL_BYTES {
        return Err(format!("a query may be at most {MAX_SQL_BYTES} bytes"));
    }
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .map_err(|e| format!("not a query Shakedown can read: {e}"))?;
    let query = match statements.as_slice() {
        [Statement::Query(q)] => q,
        [] => return Err("the query is empty".into()),
        [_] => return Err("only a SELECT may run: no writes, DDL, COPY, SET or EXPLAIN".into()),
        _ => return Err("one statement only".into()),
    };
    let mut guard = Guard::default();
    if let ControlFlow::Break(why) = query.visit(&mut guard) {
        return Err(why);
    }
    Ok(Checked {
        sql: query.to_string(),
    })
}

/// Walks the query, refusing what may not run. CTE names are collected as their queries are
/// entered; a name a CTE gives is allowed wherever it is referenced (an out-of-scope reference
/// fails in the database, never reaches another table: the search_path is the explore schema).
#[derive(Default)]
struct Guard {
    ctes: BTreeSet<String>,
}

type Flow = ControlFlow<String>;

fn refuse<T>(why: impl Into<String>) -> ControlFlow<String, T> {
    ControlFlow::Break(why.into())
}

/// An identifier as Postgres folds it: unquoted ones to lower case.
fn folded(i: &Ident) -> String {
    match i.quote_style {
        None => i.value.to_lowercase(),
        Some(_) => i.value.clone(),
    }
}

/// The identifiers of a name, folded; None when a part is not a plain identifier.
fn parts(name: &ObjectName) -> Option<Vec<String>> {
    name.0
        .iter()
        .map(|p| match p {
            ObjectNamePart::Identifier(i) => Some(folded(i)),
            ObjectNamePart::Function(_) => None,
        })
        .collect()
}

impl Guard {
    fn relation(&self, name: &ObjectName) -> Flow {
        let ok = match parts(name).as_deref() {
            Some([one]) => VIEWS.contains(&one.as_str()) || self.ctes.contains(one),
            Some([schema, view]) => schema == SCHEMA && VIEWS.contains(&view.as_str()),
            _ => false,
        };
        if ok {
            return ControlFlow::Continue(());
        }
        refuse(format!(
            "{name} is not an explore view: query {} (see GET /api/explore/schema)",
            VIEWS.join(", ")
        ))
    }

    fn function(name: &ObjectName, args: &FunctionArguments) -> Flow {
        if matches!(args, FunctionArguments::Subquery(_)) {
            return refuse("pass a function's arguments in parentheses");
        }
        match parts(name).as_deref() {
            Some([one]) if FUNCTIONS.contains(&one.as_str()) => ControlFlow::Continue(()),
            _ => refuse(format!(
                "the function {name} is not allowed (GET /api/explore/schema lists the ones that are)"
            )),
        }
    }

    fn body(body: &SetExpr) -> Flow {
        match body {
            SetExpr::Select(_) | SetExpr::Query(_) | SetExpr::Values(_) => {
                ControlFlow::Continue(())
            }
            SetExpr::SetOperation { left, right, .. } => {
                Guard::body(left)?;
                Guard::body(right)
            }
            _ => refuse("only a SELECT may run: no writes, DDL or TABLE"),
        }
    }
}

impl Visitor for Guard {
    type Break = String;

    fn pre_visit_query(&mut self, q: &Query) -> Flow {
        if !q.locks.is_empty() || q.for_clause.is_some() {
            return refuse("a query may not lock rows (FOR UPDATE / FOR SHARE)");
        }
        if q.settings.is_some() || q.format_clause.is_some() || !q.pipe_operators.is_empty() {
            return refuse("that clause is not Postgres SQL");
        }
        Guard::body(&q.body)?;
        for cte in q.with.iter().flat_map(|w| &w.cte_tables) {
            let name = folded(&cte.alias.name);
            if name.starts_with("pg_") {
                return refuse("a CTE's name may not start with pg_");
            }
            self.ctes.insert(name);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, s: &Select) -> Flow {
        if s.into.is_some() {
            return refuse("SELECT INTO writes a table: not allowed");
        }
        if !s.lateral_views.is_empty() || !s.connect_by.is_empty() || s.prewhere.is_some() {
            return refuse("that clause is not Postgres SQL");
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, name: &ObjectName) -> Flow {
        self.relation(name)
    }

    fn pre_visit_table_factor(&mut self, t: &TableFactor) -> Flow {
        match t {
            TableFactor::Table {
                args: None,
                with_hints,
                version: None,
                partitions,
                json_path: None,
                sample: None,
                index_directives,
                ..
            } if with_hints.is_empty() && partitions.is_empty() && index_directives.is_empty() => {
                ControlFlow::Continue(())
            }
            TableFactor::Derived { .. } | TableFactor::NestedJoin { .. } => {
                ControlFlow::Continue(())
            }
            _ => refuse("FROM takes explore views and subqueries only: no table functions"),
        }
    }

    fn pre_visit_expr(&mut self, e: &Expr) -> Flow {
        match e {
            Expr::Function(f) => Guard::function(&f.name, &f.args),
            _ => ControlFlow::Continue(()),
        }
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
