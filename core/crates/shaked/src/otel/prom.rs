//! The metrics: run counts by outcome, pass rate, run time, model calls, tokens, cost and call
//! latency, by world, setup, model and provider ([`families`]), written as Prometheus text
//! ([`prometheus`]) or as OTLP metrics ([`otlp`]).
//!
//! | metric                                   | type      | labels                              |
//! |------------------------------------------|-----------|-------------------------------------|
//! | `shakedown_branches`                     | gauge     | status, world, setup                |
//! | `shakedown_pass_rate`                    | gauge     | world, setup                        |
//! | `shakedown_run_duration_seconds`         | histogram | world, setup                        |
//! | `shakedown_model_calls_total`            | counter   | model, provider, world, setup       |
//! | `shakedown_model_tokens_total`           | counter   | model, provider, world, setup, type |
//! | `shakedown_model_cost_usd_total`         | counter   | model, provider, world, setup       |
//! | `shakedown_model_calls_uncosted_total`   | counter   | model, provider, world, setup       |
//! | `shakedown_model_call_duration_seconds`  | histogram | model, provider                     |

use std::collections::BTreeMap;
use std::fmt::Write;

use super::{encode, proto};
use crate::{model::BranchStatus, store::Snapshot};

const RUN_BOUNDS: [f64; 10] = [
    10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0, 3600.0, 7200.0,
];
const CALL_BOUNDS: [f64; 10] = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0, 60.0, 120.0];

pub type Labels = Vec<(&'static str, String)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Hist {
    pub bounds: &'static [f64],
    /// How many values fell in each bucket (not cumulative), one more than `bounds` (+Inf).
    pub buckets: Vec<u64>,
    pub sum: f64,
}

impl Hist {
    fn new(bounds: &'static [f64]) -> Hist {
        Hist {
            bounds,
            buckets: vec![0; bounds.len() + 1],
            sum: 0.0,
        }
    }

    fn add(&mut self, v: f64) {
        let i = self
            .bounds
            .iter()
            .position(|b| v <= *b)
            .unwrap_or(self.bounds.len());
        self.buckets[i] += 1;
        self.sum += v;
    }

    fn count(&self) -> u64 {
        self.buckets.iter().sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Values {
    Gauge(Vec<(Labels, f64)>),
    Counter(Vec<(Labels, f64)>),
    Histogram(Vec<(Labels, Hist)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Family {
    pub name: &'static str,
    pub help: &'static str,
    pub values: Values,
}

fn scored(s: BranchStatus) -> Option<bool> {
    match s {
        BranchStatus::Passed => Some(true),
        BranchStatus::Failed | BranchStatus::BudgetExceeded => Some(false),
        _ => None,
    }
}

type Key2 = (String, String);
type Key4 = (String, String, String, String);

#[derive(Default)]
struct Sums {
    calls: f64,
    input: f64,
    output: f64,
    cost: f64,
    uncosted: f64,
}

fn l2((a, b): &Key2, n: [&'static str; 2]) -> Labels {
    vec![(n[0], a.clone()), (n[1], b.clone())]
}

fn l4(k: &Key4) -> Labels {
    vec![
        ("model", k.0.clone()),
        ("provider", k.1.clone()),
        ("world", k.2.clone()),
        ("setup", k.3.clone()),
    ]
}

/// What the branches add up to, by status, world and setup.
#[derive(Default)]
struct RunTally {
    counts: BTreeMap<(String, String, String), f64>,
    pass: BTreeMap<Key2, (f64, f64)>,
    time: BTreeMap<Key2, Hist>,
}

fn run_tally(s: &Snapshot) -> RunTally {
    let mut t = RunTally::default();
    for b in &s.branches {
        let ws = (b.world.clone(), b.setup.clone());
        let key = (b.status.as_str().to_string(), ws.0.clone(), ws.1.clone());
        *t.counts.entry(key).or_default() += 1.0;
        if let Some(p) = scored(b.status) {
            let e = t.pass.entry(ws.clone()).or_default();
            (e.0, e.1) = (e.0 + f64::from(u8::from(p)), e.1 + 1.0);
        }
        if let Some(end) = b.ended_at {
            let secs = (end - b.created_at).num_milliseconds().max(0) as f64 / 1000.0;
            let h = t.time.entry(ws).or_insert_with(|| Hist::new(&RUN_BOUNDS));
            h.add(secs);
        }
    }
    t
}

const WS: [&str; 2] = ["world", "setup"];

/// The run families: counts by status, pass rate, run time.
fn run_families(s: &Snapshot) -> Vec<Family> {
    let t = run_tally(s);
    let counts = t
        .counts
        .into_iter()
        .map(|((st, w, se), n)| (vec![("status", st), ("world", w), ("setup", se)], n));
    let rates = t.pass.iter().map(|(k, (p, n))| (l2(k, WS), p / n));
    let times = t.time.into_iter().map(|(k, h)| (l2(&k, WS), h));
    vec![
        Family {
            name: "shakedown_branches",
            help: "Branches (runs and forks) by status.",
            values: Values::Gauge(counts.collect()),
        },
        Family {
            name: "shakedown_pass_rate",
            help: "Passed over scored branches (passed, failed, over budget).",
            values: Values::Gauge(rates.collect()),
        },
        Family {
            name: "shakedown_run_duration_seconds",
            help: "Wall time of ended branches.",
            values: Values::Histogram(times.collect()),
        },
    ]
}

/// What the model calls add up to, by model, provider, world and setup; latency by model and
/// provider.
fn call_tally(s: &Snapshot) -> (BTreeMap<Key4, Sums>, BTreeMap<Key2, Hist>) {
    let mut sums: BTreeMap<Key4, Sums> = BTreeMap::new();
    let mut lat: BTreeMap<Key2, Hist> = BTreeMap::new();
    for b in &s.branches {
        for c in s.calls.get(&b.id).into_iter().flatten() {
            let k = (
                c.model.clone(),
                c.provider.clone(),
                b.world.clone(),
                b.setup.clone(),
            );
            let e = sums.entry(k).or_default();
            e.calls += c.calls as f64;
            e.input += c.input_tokens as f64;
            e.output += c.output_tokens as f64;
            e.cost += c.cost_usd;
            e.uncosted += c.uncosted as f64;
            let key = (c.model.clone(), c.provider.clone());
            let h = lat.entry(key).or_insert_with(|| Hist::new(&CALL_BOUNDS));
            c.seconds.iter().for_each(|v| h.add(*v));
        }
    }
    (sums, lat)
}

/// Input and output tokens as one family, told apart by `type`.
fn tokens(sums: &BTreeMap<Key4, Sums>) -> Vec<(Labels, f64)> {
    let mut out = vec![];
    for (k, v) in sums {
        for (ty, n) in [("input", v.input), ("output", v.output)] {
            let mut l = l4(k);
            l.push(("type", ty.to_string()));
            out.push((l, n));
        }
    }
    out
}

fn counter(name: &'static str, help: &'static str, values: Vec<(Labels, f64)>) -> Family {
    Family {
        name,
        help,
        values: Values::Counter(values),
    }
}

/// The model counters that are one sum each: name, help, and the sum.
type Summed = (&'static str, &'static str, fn(&Sums) -> f64);
const SUMMED: [Summed; 3] = [
    (
        "shakedown_model_calls_total",
        "Model calls through the gateway.",
        |v| v.calls,
    ),
    (
        "shakedown_model_cost_usd_total",
        "Model spend in USD (calls with a known cost).",
        |v| v.cost,
    ),
    (
        "shakedown_model_calls_uncosted_total",
        "Model calls whose cost is not known.",
        |v| v.uncosted,
    ),
];

/// The model families: calls, tokens, cost, latency.
fn model_families(s: &Snapshot) -> Vec<Family> {
    let (sums, lat) = call_tally(s);
    let each = |f: fn(&Sums) -> f64| sums.iter().map(|(k, v)| (l4(k), f(v))).collect();
    let mut out: Vec<Family> = SUMMED
        .iter()
        .map(|(n, h, f)| counter(n, h, each(*f)))
        .collect();
    let help = "Model tokens, input and output.";
    out.insert(
        1,
        counter("shakedown_model_tokens_total", help, tokens(&sums)),
    );
    let latency = lat
        .into_iter()
        .map(|(k, h)| (l2(&k, ["model", "provider"]), h));
    out.push(Family {
        name: "shakedown_model_call_duration_seconds",
        help: "How long the provider took to answer.",
        values: Values::Histogram(latency.collect()),
    });
    out
}

/// Every metric of `s`.
pub fn families(s: &Snapshot) -> Vec<Family> {
    let mut f = run_families(s);
    f.extend(model_families(s));
    f
}

/// A label value escaped for Prometheus' text format.
fn escaped(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn labels_text(l: &Labels, extra: Option<(&str, String)>) -> String {
    let all = l.iter().map(|(k, v)| (*k, v.clone())).chain(extra);
    let parts: Vec<String> = all
        .map(|(k, v)| format!("{k}=\"{}\"", escaped(&v)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn number(v: f64) -> String {
    match v.is_finite() {
        true => format!("{v}"),
        false => "NaN".into(),
    }
}

fn write_hist(out: &mut String, name: &str, l: &Labels, h: &Hist) {
    let mut seen = 0;
    for (i, b) in h.bounds.iter().enumerate() {
        seen += h.buckets[i];
        let _ = writeln!(
            out,
            "{name}_bucket{} {seen}",
            labels_text(l, Some(("le", number(*b))))
        );
    }
    let n = h.count();
    let _ = writeln!(
        out,
        "{name}_bucket{} {n}",
        labels_text(l, Some(("le", "+Inf".into())))
    );
    let _ = writeln!(out, "{name}_sum{} {}", labels_text(l, None), number(h.sum));
    let _ = writeln!(out, "{name}_count{} {n}", labels_text(l, None));
}

/// The families in Prometheus' text exposition format (0.0.4).
pub fn prometheus(families: &[Family]) -> String {
    let mut out = String::new();
    for f in families {
        let kind = match f.values {
            Values::Gauge(_) => "gauge",
            Values::Counter(_) => "counter",
            Values::Histogram(_) => "histogram",
        };
        let _ = writeln!(
            out,
            "# HELP {} {}\n# TYPE {} {kind}",
            f.name, f.help, f.name
        );
        match &f.values {
            Values::Gauge(v) | Values::Counter(v) => {
                for (l, n) in v {
                    let _ = writeln!(out, "{}{} {}", f.name, labels_text(l, None), number(*n));
                }
            }
            Values::Histogram(v) => v
                .iter()
                .for_each(|(l, h)| write_hist(&mut out, f.name, l, h)),
        }
    }
    out
}

fn attrs(l: &Labels) -> Vec<proto::KeyValue> {
    let a = l
        .iter()
        .map(|(k, v)| (k.to_string(), super::span::Attr::Str(v.clone())));
    encode::key_values(&a.collect())
}

fn number_points(v: &[(Labels, f64)], (start, now): (u64, u64)) -> Vec<proto::NumberDataPoint> {
    let point = |(l, n): &(Labels, f64)| proto::NumberDataPoint {
        attributes: attrs(l),
        start_time_unix_nano: start,
        time_unix_nano: now,
        as_double: Some(*n),
    };
    v.iter().map(point).collect()
}

fn hist_points(v: &[(Labels, Hist)], (start, now): (u64, u64)) -> Vec<proto::HistogramDataPoint> {
    let point = |(l, h): &(Labels, Hist)| proto::HistogramDataPoint {
        attributes: attrs(l),
        start_time_unix_nano: start,
        time_unix_nano: now,
        count: h.count(),
        sum: Some(h.sum),
        bucket_counts: h.buckets.clone(),
        explicit_bounds: h.bounds.to_vec(),
    };
    v.iter().map(point).collect()
}

fn metric_of(f: &Family, times: (u64, u64)) -> proto::Metric {
    use proto::Data;
    let data = match &f.values {
        Values::Gauge(v) => Data::Gauge(proto::Gauge {
            data_points: number_points(v, times),
        }),
        Values::Counter(v) => Data::Sum(proto::Sum {
            data_points: number_points(v, times),
            aggregation_temporality: proto::CUMULATIVE,
            is_monotonic: true,
        }),
        Values::Histogram(v) => Data::Histogram(proto::Histogram {
            data_points: hist_points(v, times),
            aggregation_temporality: proto::CUMULATIVE,
        }),
    };
    // A collector's Prometheus exporter adds `_total` to a monotonic sum again.
    let name = f.name.strip_suffix("_total").unwrap_or(f.name);
    proto::Metric {
        name: name.to_string(),
        description: f.help.into(),
        unit: String::new(),
        data: Some(data),
    }
}

/// The families as an OTLP metrics request, counted since `start` (ns) as of `now` (ns).
pub fn otlp(families: &[Family], start: u64, now: u64) -> proto::ExportMetricsServiceRequest {
    proto::ExportMetricsServiceRequest {
        resource_metrics: vec![proto::ResourceMetrics {
            resource: Some(encode::resource(None)),
            scope_metrics: vec![proto::ScopeMetrics {
                scope: Some(encode::scope()),
                metrics: families
                    .iter()
                    .map(|f| metric_of(f, (start, now)))
                    .collect(),
            }],
        }],
    }
}
