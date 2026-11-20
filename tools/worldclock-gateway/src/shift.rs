//! Times in store queries and answers, moved between wall and world time. Prometheus and Tempo
//! take seconds; Loki takes nanoseconds (an integer) or seconds (a decimal). Answers carry
//! Prometheus' and Loki's `[t, v]` pairs (seconds as numbers, Loki's stream lines' ns as
//! strings) and Tempo's `startTimeUnixNano`/`endTimeUnixNano`.

use serde_json::{json, Value};

use crate::query::Backend;

const NS_PER_MS: i128 = 1_000_000;

/// The query's parameters, in order.
pub type Params = Vec<(String, String)>;

pub fn parse(text: &[u8]) -> Params {
    form_urlencoded::parse(text).into_owned().collect()
}

pub fn encode(p: &Params) -> String {
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(p)
        .finish()
}

fn get<'a>(p: &'a Params, key: &str) -> Option<&'a str> {
    p.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn set(p: &mut Params, key: &str, value: String) {
    match p.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => p.push((key.into(), value)),
    }
}

/// Seconds `s` (text) moved by `ms`, kept whole when both are.
fn secs_text(s: &str, ms: i64) -> Option<String> {
    let v: f64 = s.parse().ok()?;
    if !s.contains('.') && ms % 1000 == 0 {
        return Some(((v as i64) + ms / 1000).to_string());
    }
    Some(trim(&format!("{:.3}", v + ms as f64 / 1000.0)))
}

fn trim(s: &str) -> String {
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Nanoseconds `s` (an integer as text) moved by `ms`.
fn ns_text(s: &str, ms: i64) -> Option<String> {
    let v: i128 = s.parse().ok()?;
    Some((v + i128::from(ms) * NS_PER_MS).to_string())
}

/// A time parameter of `b` moved by `ms` (left as it is when it is no number: RFC 3339).
fn moved(b: Backend, s: &str, ms: i64) -> Option<String> {
    match b {
        Backend::Loki if !s.contains('.') => ns_text(s, ms),
        _ => secs_text(s, ms),
    }
}

/// Grafana's window, moved from wall to world time (`offset_ms` = world − wall).
pub fn shift_params(b: Backend, p: &mut Params, offset_ms: i64) {
    for (k, v) in p.iter_mut() {
        if matches!(k.as_str(), "start" | "end" | "time") {
            if let Some(m) = moved(b, v, offset_ms) {
                *v = m;
            }
        }
    }
}

/// An agent's query without its time: instant queries at world now, ranges ending at world now
/// (Loki's and Tempo's a hour long when they have no start either).
pub fn default_params(b: Backend, path: &str, p: &mut Params, world_now_ms: i64) {
    let now_s = trim(&format!("{:.3}", world_now_ms as f64 / 1000.0));
    let now_ns = (i128::from(world_now_ms) * NS_PER_MS).to_string();
    let hour_ago = |now: &str| moved(b, now, -3_600_000).unwrap_or_default();
    let (instant, range) = match b {
        Backend::Prometheus => (
            path.ends_with("/api/v1/query"),
            path.ends_with("/api/v1/query_range"),
        ),
        Backend::Loki => (
            path.ends_with("/api/v1/query"),
            path.ends_with("/api/v1/query_range"),
        ),
        Backend::Tempo => (false, path.ends_with("/api/search")),
    };
    let now = match b {
        Backend::Loki => now_ns,
        Backend::Tempo => world_now_ms.div_euclid(1000).to_string(), // Tempo takes whole seconds
        Backend::Prometheus => now_s,
    };
    if instant && get(p, "time").is_none() {
        set(p, "time", now.clone());
    }
    if range && get(p, "end").is_none() {
        set(p, "end", now.clone());
        if b != Backend::Prometheus && get(p, "start").is_none() {
            set(p, "start", hour_ago(&now));
        }
    }
}

/// A JSON answer of `b` with its times moved back by `offset_ms` (to wall time); any other
/// answer as it is.
pub fn shift_answer(b: Backend, body: Vec<u8>, offset_ms: i64) -> Vec<u8> {
    let Ok(mut v) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    match b {
        Backend::Tempo => nanos_back(&mut v, -offset_ms),
        _ => pairs_back(&mut v, -offset_ms),
    }
    serde_json::to_vec(&v).unwrap_or(body)
}

/// Prometheus' and Loki's `data.result[].value` and `.values[]` pairs moved by `ms`.
fn pairs_back(v: &mut Value, ms: i64) {
    let Some(result) = v.pointer_mut("/data/result").and_then(Value::as_array_mut) else {
        return;
    };
    for series in result {
        if let Some(pair) = series.get_mut("value") {
            move_pair(pair, ms);
        }
        if let Some(Value::Array(pairs)) = series.get_mut("values") {
            pairs.iter_mut().for_each(|p| move_pair(p, ms));
        }
    }
}

fn move_pair(pair: &mut Value, ms: i64) {
    let Some(t) = pair.get_mut(0) else { return };
    let moved = match &*t {
        Value::Number(n) if n.is_i64() && ms % 1000 == 0 => {
            Some(json!(n.as_i64().unwrap_or(0) + ms / 1000))
        }
        Value::Number(n) => n
            .as_f64()
            .map(|s| json!(((s * 1000.0).round() + ms as f64) / 1000.0)),
        Value::String(s) => ns_text(s, ms).map(Value::String),
        _ => None,
    };
    if let Some(m) = moved {
        *t = m;
    }
}

/// Every `startTimeUnixNano`/`endTimeUnixNano` in Tempo's answer moved by `ms`.
fn nanos_back(v: &mut Value, ms: i64) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                let stamp = k == "startTimeUnixNano" || k == "endTimeUnixNano";
                match x {
                    Value::String(s) if stamp => *s = ns_text(s, ms).unwrap_or_else(|| s.clone()),
                    _ => nanos_back(x, ms),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|x| nanos_back(x, ms)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_stay_whole_or_keep_their_milliseconds() {
        assert_eq!(secs_text("10000", -3_600_000).as_deref(), Some("6400"));
        assert_eq!(
            secs_text("1773122405.123", 1_500).as_deref(),
            Some("1773122406.623")
        );
        assert_eq!(secs_text("2026-03-10T06:00:00Z", 1_000), None);
        assert_eq!(ns_text("1000", 1).as_deref(), Some("1001000"));
    }

    #[test]
    fn tempo_ranges_are_whole_seconds() {
        let mut p = vec![("q".to_string(), "{}".to_string())];
        default_params(Backend::Tempo, "/api/search", &mut p, 1_773_122_405_123);
        assert_eq!(get(&p, "end"), Some("1773122405"));
        assert_eq!(get(&p, "start"), Some("1773118805"));
    }
}
