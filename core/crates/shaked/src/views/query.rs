//! The bindings' grammar over docs, as the expected-state checks have it
//! (`kits/python/shakedown_part/expect.py`): dotted fields, `where` conditions with the same ops
//! (numbers equal within 0.005), and `find` globs over a part's keys.

use serde_json::{Map, Value};
use shakedown_project::format::{Field, Find};

const TOLERANCE: f64 = 0.005;

/// `{part: {key: doc}}`.
pub type WorldDocs = std::collections::BTreeMap<String, Map<String, Value>>;

/// The value at the dotted `path` in `doc` (list items by index); None when there is none.
pub fn field_of<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(doc);
    }
    path.split('.').try_fold(doc, |v, k| match v {
        Value::Object(m) => m.get(k),
        Value::Array(a) => k.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}

/// The first of `field`'s paths present in `doc` and not null.
pub fn first_of<'a>(doc: &'a Value, field: &Field) -> Option<&'a Value> {
    let found = field.paths().into_iter().filter_map(|p| field_of(doc, p));
    found.into_iter().find(|v| !v.is_null())
}

/// A field as a time in epoch ms: a number as it is, or an ISO time (UTC unless it says).
pub fn time_ms(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_f64().map(|f| f as i64),
        Value::String(s) => parse_time(s),
        _ => None,
    }
}

fn parse_time(s: &str) -> Option<i64> {
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(t.timestamp_millis());
    }
    let naive = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"]
        .iter()
        .find_map(|f| chrono::NaiveDateTime::parse_from_str(s, f).ok());
    naive.map(|t| t.and_utc().timestamp_millis())
}

/// True when `text` matches `glob`: `*` any run of characters, `?` one character.
pub fn glob(glob: &str, text: &str) -> bool {
    let (g, t): (Vec<char>, Vec<char>) = (glob.chars().collect(), text.chars().collect());
    let (mut gi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        match g.get(gi) {
            Some('*') => {
                star = Some(gi);
                mark = ti;
                gi += 1;
            }
            Some(&c) if c == '?' || c == t[ti] => {
                gi += 1;
                ti += 1;
            }
            _ => match star {
                Some(s) => {
                    gi = s + 1;
                    mark += 1;
                    ti = mark;
                }
                None => return false,
            },
        }
    }
    g[gi..].iter().all(|c| *c == '*')
}

/// One condition of a `where`: the field, the op and the value.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    pub value: Value,
}

/// The ops of the expected-state grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    Contains,
    Exists,
    Absent,
}

impl Op {
    fn parse(name: &str) -> Result<Op, String> {
        Ok(match name {
            "=" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "in" => Op::In,
            "contains" => Op::Contains,
            "exists" => Op::Exists,
            "absent" => Op::Absent,
            other => return Err(format!("unknown op {other}")),
        })
    }
}

/// `where` as conditions: a plain value is `=`, an object is `{op: value}`.
pub fn conditions(when: Option<&Map<String, Value>>) -> Result<Vec<Condition>, String> {
    let Some(when) = when else {
        return Ok(vec![]);
    };
    let one = |(field, cond): (&String, &Value)| {
        let (op, value) = match cond {
            Value::Object(o) if o.len() == 1 => {
                let (op, v) = o.iter().next().expect("one entry");
                (Op::parse(op)?, v.clone())
            }
            Value::Object(_) => {
                return Err(format!("where {field} needs one op, as {{op: value}}"))
            }
            v => (Op::Eq, v.clone()),
        };
        Ok(Condition {
            field: field.clone(),
            op,
            value,
        })
    };
    when.iter().map(one).collect()
}

/// True when every condition holds for `doc`.
pub fn matches(doc: &Value, conds: &[Condition]) -> bool {
    conds
        .iter()
        .all(|c| compare(c.op, field_of(doc, &c.field), &c.value))
}

fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(|v| match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    })
}

fn equal(a: Option<&Value>, b: &Value) -> bool {
    if b.is_null() {
        return a.is_none_or(Value::is_null);
    }
    match (number(a), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < TOLERANCE,
        _ => a == Some(b),
    }
}

fn ordered(a: Option<&Value>, b: &Value, test: impl Fn(f64, f64) -> bool) -> bool {
    matches!((number(a), b.as_f64()), (Some(x), Some(y)) if test(x, y))
}

fn contains(a: Option<&Value>, b: &Value) -> bool {
    match (a, b) {
        (Some(Value::Array(items)), _) => items.iter().any(|x| equal(Some(x), b)),
        (Some(Value::String(s)), Value::String(sub)) => s.contains(sub.as_str()),
        _ => false,
    }
}

/// `actual op expected`, as the expected-state checks compare.
pub fn compare(op: Op, actual: Option<&Value>, expected: &Value) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() < TOLERANCE;
    match op {
        Op::Eq => equal(actual, expected),
        Op::Ne => !equal(actual, expected),
        Op::Lt => ordered(actual, expected, |x, y| x < y && !near(x, y)),
        Op::Le => ordered(actual, expected, |x, y| x < y || near(x, y)),
        Op::Gt => ordered(actual, expected, |x, y| x > y && !near(x, y)),
        Op::Ge => ordered(actual, expected, |x, y| x > y || near(x, y)),
        Op::In => (expected.as_array()).is_some_and(|xs| xs.iter().any(|x| equal(actual, x))),
        Op::Contains => contains(actual, expected),
        Op::Exists => actual.is_some_and(|v| !v.is_null()),
        Op::Absent => actual.is_none_or(Value::is_null),
    }
}

/// The docs of a part.
pub fn part_docs<'a>(docs: &'a WorldDocs, part: &str) -> Result<&'a Map<String, Value>, String> {
    docs.get(part)
        .ok_or_else(|| format!("no part {part} in this world"))
}

/// The `(key, doc)`s `find` matches, in key order; `default_part` when it names none.
pub fn found<'a>(
    docs: &'a WorldDocs,
    find: &Find,
    default_part: &str,
) -> Result<Vec<(&'a str, &'a Value)>, String> {
    let part = find.part.as_deref().unwrap_or(default_part);
    let conds = conditions(find.when.as_ref())?;
    let all = part_docs(docs, part)?.iter();
    let hit = |(k, d): &(&String, &Value)| glob(&find.find, k) && matches(d, &conds);
    let mut out: Vec<(&str, &Value)> = all.filter(hit).map(|(k, d)| (k.as_str(), d)).collect();
    out.sort_by(|a, b| a.0.cmp(b.0));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fields_are_dotted_paths_and_a_list_takes_the_first_present() {
        let doc = json!({"a": {"b": [10, 20]}, "on": null, "eta": 5});
        assert_eq!(field_of(&doc, "a.b.1"), Some(&json!(20)));
        assert_eq!(field_of(&doc, "a.c"), None);
        let field = Field::First(vec!["on".into(), "eta".into()]);
        assert_eq!(first_of(&doc, &field), Some(&json!(5)));
    }

    #[test]
    fn where_uses_the_expected_state_ops() {
        let doc = json!({"status": "off", "delay": 15.001, "codes": ["WX"], "prog": null});
        let when = |w: Value| conditions(w.as_object()).unwrap();
        assert!(matches(
            &doc,
            &when(json!({"status": "off", "delay": {">=": 15}}))
        ));
        assert!(
            !matches(&doc, &when(json!({"delay": {">": 15}}))),
            "within 0.005 is equal"
        );
        assert!(matches(
            &doc,
            &when(json!({"status": {"in": ["out", "off"]}}))
        ));
        assert!(matches(&doc, &when(json!({"codes": {"contains": "WX"}}))));
        assert!(matches(
            &doc,
            &when(json!({"prog": {"absent": true}, "nope": null}))
        ));
        assert!(!matches(&doc, &when(json!({"prog": {"exists": true}}))));
        assert!(conditions(json!({"a": {"~": 1}}).as_object()).is_err());
    }

    #[test]
    fn globs_match_star_and_one_character() {
        assert!(glob("flight/*", "flight/UA1"));
        assert!(glob("f?ight/*1", "flight/UA1"));
        assert!(!glob("flight/*", "tail/N1"));
        assert!(glob("*", ""));
    }

    #[test]
    fn times_are_iso_or_epoch_ms() {
        assert_eq!(time_ms(&json!("1970-01-01T00:01:00Z")), Some(60_000));
        assert_eq!(time_ms(&json!("1970-01-01T00:02")), Some(120_000));
        assert_eq!(time_ms(&json!(5)), Some(5));
        assert_eq!(time_ms(&json!("soon")), None);
    }
}
