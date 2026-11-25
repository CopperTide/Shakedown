//! A screen's panels evaluated against the parts' docs at one point: compact `columns` and
//! `rows`, only the fields the view names, times as epoch ms, and a tone (and chip label) per row
//! from the view's styles. A panel that can't be evaluated is `{kind, error}` alone.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};
use shakedown_project::format::{
    BarValue, BarsPanel, BoardPanel, ChatPanel, FeedPanel, Field, Figure, Find, KpisPanel, Panel,
    Shown, TimelinePanel, Tone, ViewFile,
};

use super::query::{conditions, first_of, found, matches, time_ms, WorldDocs};

/// A world event of the branch, as a feed shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedEvent {
    pub seq: i64,
    pub clock_ms: i64,
    pub world_ms: i64,
    pub part: String,
    pub kind: String,
    pub text: String,
    pub data: Value,
}

/// What a screen is evaluated against.
pub struct Ctx<'a> {
    pub view: &'a ViewFile,
    pub docs: &'a WorldDocs,
    /// The world's time at the point, epoch ms.
    pub world_ms: i64,
    /// The branch's world events up to the point, oldest first.
    pub events: &'a [FeedEvent],
}

const BARS_TOP: usize = 40;
const FEED_TOP: usize = 60;

impl Ctx<'_> {
    fn part(&self) -> &str {
        self.view.part.as_deref().unwrap_or("")
    }

    fn found(&self, find: &Find) -> Result<Vec<(&str, &Value)>, String> {
        found(self.docs, find, self.part())
    }

    /// The tone and chip label the style `name` gives `doc` (neutral without a style).
    fn tone(&self, name: Option<&str>, doc: &Value) -> Result<(Tone, Value), String> {
        let Some(name) = name else {
            return Ok((Tone::Neutral, Value::Null));
        };
        let rules = (self.view.styles.get(name)).ok_or_else(|| format!("no style {name}"))?;
        for rule in rules {
            if matches(doc, &conditions(rule.when.as_ref())?) {
                return Ok((rule.tone, json!(rule.label)));
            }
        }
        Ok((Tone::Neutral, Value::Null))
    }
}

fn tone_value(t: Tone) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

/// `{kind, ...}` of panel `panel`, or `{kind, error}`.
pub fn panel(ctx: &Ctx, panel: &Panel) -> Value {
    let (kind, done) = match panel {
        Panel::Kpis(p) => ("kpis", kpis(ctx, p)),
        Panel::Map(p) => ("map", super::map::map(ctx, p)),
        Panel::Board(p) => ("board", board(ctx, p)),
        Panel::Timeline(p) => ("timeline", timeline(ctx, p)),
        Panel::Bars(p) => ("bars", bars(ctx, p)),
        Panel::Feed(p) => ("feed", Ok(feed(ctx, p))),
        Panel::Chat(p) => ("chat", chat(ctx, p)),
    };
    let mut out = done.unwrap_or_else(|error| json!({"error": error}));
    out["kind"] = json!(kind);
    out
}

/// A row's tone and chip, as two trailing values.
pub(super) fn toned(ctx: &Ctx, style: Option<&str>, doc: &Value) -> Result<[Value; 2], String> {
    let (tone, chip) = ctx.tone(style, doc)?;
    Ok([tone_value(tone), chip])
}

fn figure(ctx: &Ctx, value: &Figure) -> Result<Value, String> {
    match value {
        Figure::Key(k) => {
            let part = k.part.as_deref().unwrap_or(ctx.part());
            let docs = super::query::part_docs(ctx.docs, part)?;
            let doc = docs.get(&k.key);
            let picked = match (&k.field, doc) {
                (Some(f), Some(d)) => first_of(d, f).cloned(),
                (None, d) => d.cloned(),
                (Some(_), None) => None,
            };
            Ok(picked.unwrap_or(Value::Null))
        }
        Figure::Count(c) => {
            let find = Find {
                part: c.part.clone(),
                find: c.find.clone(),
                when: c.when.clone(),
            };
            Ok(json!(ctx.found(&find)?.len()))
        }
    }
}

fn kpis(ctx: &Ctx, p: &KpisPanel) -> Result<Value, String> {
    let item = |i: &shakedown_project::format::KpiItem| {
        let value = figure(ctx, &i.value)?;
        let (tone, chip) = ctx.tone(i.style.as_deref(), &json!({"value": value}))?;
        Ok(json!({"value": value, "tone": tone_value(tone), "chip": chip}))
    };
    let items: Result<Vec<Value>, String> = p.items.iter().map(item).collect();
    Ok(json!({"items": items?}))
}

/// A field's value as a row carries it: a time as epoch ms, else as the doc has it.
fn cell(doc: &Value, field: &Field, shown: Option<Shown>) -> Value {
    let v = first_of(doc, field);
    match (v, shown) {
        (Some(v), Some(Shown::Time)) => json!(time_ms(v)),
        (Some(v), _) => v.clone(),
        (None, _) => Value::Null,
    }
}

/// How two sort values order: numbers, then text, then nothing (nulls last either way).
fn sort_order(a: &Value, b: &Value, desc: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let order = match (a, b) {
        (Value::Null, Value::Null) => return Equal,
        (Value::Null, _) => return Greater,
        (_, Value::Null) => return Less,
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64()).partial_cmp(&y.as_f64()).unwrap_or(Equal)
        }
        (x, y) => x.to_string().cmp(&y.to_string()),
    };
    if desc {
        order.reverse()
    } else {
        order
    }
}

/// Which of the board's filters `doc` passes, as bits.
fn filter_bits(p: &BoardPanel, doc: &Value) -> Result<u64, String> {
    let mut bits = 0u64;
    for (i, f) in p.filters.iter().enumerate().take(64) {
        if matches(doc, &conditions(Some(&f.when))?) {
            bits |= 1 << i;
        }
    }
    Ok(bits)
}

fn board_row(ctx: &Ctx, p: &BoardPanel, key: &str, doc: &Value) -> Result<Value, String> {
    let mut row = vec![json!(key)];
    row.extend(p.columns.iter().map(|c| cell(doc, &c.field, c.format)));
    row.extend(toned(ctx, p.style.as_deref(), doc)?);
    row.push(json!(filter_bits(p, doc)?));
    Ok(Value::Array(row))
}

fn board(ctx: &Ctx, p: &BoardPanel) -> Result<Value, String> {
    let mut docs = ctx.found(&p.data)?;
    if let Some(sort) = &p.sort {
        let by = |d: &Value| first_of(d, &sort.field).cloned().unwrap_or(Value::Null);
        docs.sort_by(|a, b| sort_order(&by(a.1), &by(b.1), sort.desc));
    }
    let total = docs.len();
    docs.truncate(p.top.unwrap_or(total));
    let rows: Result<Vec<Value>, String> = (docs.iter())
        .map(|(k, d)| board_row(ctx, p, k, d))
        .collect();
    let mut columns = vec![json!("key")];
    columns.extend(p.columns.iter().map(|c| json!(c.field.name())));
    columns.extend([json!("tone"), json!("chip"), json!("filters")]);
    Ok(json!({"columns": columns, "rows": rows?, "total": total}))
}

/// A lane's name: text as it is, anything else as JSON.
fn lane_name(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

struct Bar<'a> {
    lane: String,
    key: &'a str,
    doc: &'a Value,
    start: i64,
    end: i64,
}

/// The docs of a timeline that have a lane, a start and an end.
fn timeline_bars<'a>(p: &TimelinePanel, docs: Vec<(&'a str, &'a Value)>) -> Vec<Bar<'a>> {
    let bar = |(key, doc): (&'a str, &'a Value)| {
        Some(Bar {
            lane: lane_name(first_of(doc, &p.lane)?),
            key,
            doc,
            start: time_ms(first_of(doc, &p.start)?)?,
            end: time_ms(first_of(doc, &p.end)?)?,
        })
    };
    docs.into_iter().filter_map(bar).collect()
}

fn timeline(ctx: &Ctx, p: &TimelinePanel) -> Result<Value, String> {
    let bars = timeline_bars(p, ctx.found(&p.data)?);
    let lanes: Vec<String> = (bars.iter().map(|b| b.lane.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index = |lane: &str| lanes.binary_search_by(|l| l.as_str().cmp(lane)).ok();
    let mut rows = Vec::with_capacity(bars.len());
    for b in &bars {
        let label = (p.label.as_ref()).map_or(Value::Null, |f| cell(b.doc, f, None));
        let at = [json!(b.start), json!(b.end)];
        let mut row = vec![json!(index(&b.lane)), json!(b.key), label];
        row.extend(at);
        row.extend(toned(ctx, p.style.as_deref(), b.doc)?);
        rows.push(Value::Array(row));
    }
    let range = [
        bars.iter().map(|b| b.start).min(),
        bars.iter().map(|b| b.end).max(),
    ];
    Ok(json!({"lanes": lanes, "range": range,
              "columns": ["lane", "key", "label", "start", "end", "tone", "chip"], "rows": rows}))
}

/// A bar's value: the field, or the minutes since its time by the world clock.
fn bar_value(ctx: &Ctx, value: &BarValue, doc: &Value) -> Option<f64> {
    match value {
        BarValue::Field(f) => first_of(doc, f)?.as_f64(),
        BarValue::Since(s) => {
            let at = time_ms(first_of(doc, &s.since)?)?;
            Some((ctx.world_ms - at) as f64 / 60_000.0)
        }
    }
}

fn round(x: f64, places: i32) -> f64 {
    let m = 10f64.powi(places);
    (x * m).round() / m
}

fn bar_row(ctx: &Ctx, p: &BarsPanel, key: &str, doc: &Value) -> Option<Result<Value, String>> {
    let value = bar_value(ctx, &p.value, doc)?;
    let limit = first_of(doc, &p.limit)?.as_f64().filter(|l| *l > 0.0)?;
    let ratio = value / limit;
    let mut seen = doc.as_object().cloned().unwrap_or_default();
    seen.extend([
        ("value".into(), json!(value)),
        ("limit".into(), json!(limit)),
    ]);
    seen.insert("ratio".into(), json!(ratio));
    let label = cell(doc, &p.label, None);
    let row = [
        json!(key),
        label,
        json!(round(value, 1)),
        json!(limit),
        json!(round(ratio, 3)),
    ];
    Some(
        toned(ctx, p.style.as_deref(), &Value::Object(seen)).map(|t| {
            let mut row = row.to_vec();
            row.extend(t);
            Value::Array(row)
        }),
    )
}

fn ratio_of(row: &Value) -> f64 {
    row[4].as_f64().unwrap_or(0.0)
}

fn bars(ctx: &Ctx, p: &BarsPanel) -> Result<Value, String> {
    let docs = ctx.found(&p.data)?;
    let rows: Result<Vec<Value>, String> = (docs.iter())
        .filter_map(|(k, d)| bar_row(ctx, p, k, d))
        .collect();
    let mut rows = rows?;
    rows.sort_by(|a, b| ratio_of(b).total_cmp(&ratio_of(a)));
    rows.truncate(p.top.unwrap_or(BARS_TOP));
    Ok(
        json!({"columns": ["key", "label", "value", "limit", "ratio", "tone", "chip"],
              "rows": rows, "unit": p.unit}),
    )
}

fn feed_row(ctx: &Ctx, p: &FeedPanel, e: &FeedEvent) -> Value {
    let seen = json!({"part": e.part, "kind": e.kind, "text": e.text, "data": e.data});
    let [tone, chip] = toned(ctx, p.style.as_deref(), &seen).unwrap_or([Value::Null, Value::Null]);
    json!([e.seq, e.clock_ms, e.world_ms, e.part, e.kind, e.text, tone, chip])
}

fn feed(ctx: &Ctx, p: &FeedPanel) -> Value {
    let wanted = |e: &&FeedEvent| {
        p.kinds.contains(&e.kind) && (p.parts.is_empty() || p.parts.contains(&e.part))
    };
    let newest = ctx.events.iter().rev().filter(wanted);
    let rows: Vec<Value> = newest
        .take(p.top.unwrap_or(FEED_TOP))
        .map(|e| feed_row(ctx, p, e))
        .collect();
    json!({"columns": ["seq", "clock_ms", "world_ms", "part", "kind", "text", "tone", "chip"],
           "rows": rows})
}

/// How two keys order with their digit runs read as numbers (`msg/2` before `msg/10`).
fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    let runs = |s: &str| -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = vec![];
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    };
    // A number's run sorts by its length without leading zeros, then by its digits.
    let key = |(digit, run): (bool, String)| {
        let width = if digit {
            run.trim_start_matches('0').len()
        } else {
            0
        };
        (width, run)
    };
    let (a, b) = (runs(a), runs(b));
    a.into_iter().map(key).cmp(b.into_iter().map(key))
}

/// A conversation, oldest first: by key (or `sort`), the last `top` kept.
fn chat(ctx: &Ctx, p: &ChatPanel) -> Result<Value, String> {
    let mut docs = ctx.found(&p.data)?;
    docs.sort_by(|a, b| natural_order(a.0, b.0));
    if let Some(sort) = &p.sort {
        let by = |d: &Value| first_of(d, &sort.field).cloned().unwrap_or(Value::Null);
        docs.sort_by(|a, b| sort_order(&by(a.1), &by(b.1), sort.desc));
    }
    let total = docs.len();
    let kept = &docs[total.saturating_sub(p.top.unwrap_or(total))..];
    let row = |(key, doc): &(&str, &Value)| -> Result<Value, String> {
        let mut row = vec![
            json!(key),
            cell(doc, &p.from, None),
            cell(doc, &p.text, None),
        ];
        row.extend(toned(ctx, p.style.as_deref(), doc)?);
        Ok(Value::Array(row))
    };
    let rows: Result<Vec<Value>, String> = kept.iter().map(row).collect();
    let columns = ["key", "from", "text", "tone", "chip"];
    Ok(json!({"columns": columns, "rows": rows?, "right": p.right, "total": total}))
}

/// The parts a panel's bindings read (the view's default part where they name none).
pub fn parts_of(view: &ViewFile, panel: &Panel) -> BTreeSet<String> {
    let default = view.part.clone().unwrap_or_default();
    let of = |p: &Option<String>| p.clone().unwrap_or_else(|| default.clone());
    match panel {
        Panel::Kpis(k) => (k.items.iter())
            .map(|i| match &i.value {
                Figure::Key(k) => of(&k.part),
                Figure::Count(c) => of(&c.part),
            })
            .collect(),
        Panel::Map(m) => m
            .layers
            .iter()
            .map(|l| of(&super::map::data(l).part))
            .collect(),
        Panel::Board(b) => BTreeSet::from([of(&b.data.part)]),
        Panel::Timeline(t) => BTreeSet::from([of(&t.data.part)]),
        Panel::Bars(b) => BTreeSet::from([of(&b.data.part)]),
        Panel::Chat(c) => BTreeSet::from([of(&c.data.part)]),
        Panel::Feed(_) => BTreeSet::new(),
    }
}

/// `{panel id: evaluated}` of every panel screen `screen` lays out.
pub fn screen(ctx: &Ctx, screen: &shakedown_project::format::Screen) -> Map<String, Value> {
    let names: BTreeSet<&str> = (screen.areas.iter())
        .flat_map(|r| r.split_whitespace())
        .filter(|n| *n != ".")
        .collect();
    let panels = names
        .into_iter()
        .filter_map(|n| Some((n, ctx.view.panels.get(n)?)));
    panels
        .map(|(n, p)| (n.to_string(), panel(ctx, p)))
        .collect()
}

#[cfg(test)]
#[path = "eval_tests.rs"]
mod tests;
