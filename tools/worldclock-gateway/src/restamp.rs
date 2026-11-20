//! Every time an OTLP export carries, mapped from wall to world time: spans' start and end and
//! their events', log records' time and observed time, and every metric point's start and time
//! and its exemplars'. A time that is not set (0) stays unset.

use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use opentelemetry_proto::tonic::metrics::v1::{
    ExponentialHistogramDataPoint, HistogramDataPoint, NumberDataPoint, SummaryDataPoint,
};

use crate::clock::{Clock, Timeline};

pub fn traces(req: &mut ExportTraceServiceRequest, c: &Clock) {
    let t = c.timeline();
    let at = |ns: &mut u64| *ns = t.world_ns(*ns);
    let spans = (req.resource_spans.iter_mut())
        .flat_map(|r| &mut r.scope_spans)
        .flat_map(|s| &mut s.spans);
    for span in spans {
        at(&mut span.start_time_unix_nano);
        at(&mut span.end_time_unix_nano);
        span.events
            .iter_mut()
            .for_each(|e| at(&mut e.time_unix_nano));
    }
}

pub fn logs(req: &mut ExportLogsServiceRequest, c: &Clock) {
    let t = c.timeline();
    let records = (req.resource_logs.iter_mut())
        .flat_map(|r| &mut r.scope_logs)
        .flat_map(|s| &mut s.log_records);
    for r in records {
        r.time_unix_nano = t.world_ns(r.time_unix_nano);
        r.observed_time_unix_nano = t.world_ns(r.observed_time_unix_nano);
    }
}

pub fn metrics(req: &mut ExportMetricsServiceRequest, c: &Clock) {
    let t = c.timeline();
    let data = (req.resource_metrics.iter_mut())
        .flat_map(|r| &mut r.scope_metrics)
        .flat_map(|s| &mut s.metrics)
        .filter_map(|m| m.data.as_mut());
    for d in data {
        restamp_data(d, &t);
    }
}

/// The points of one metric.
fn restamp_data(d: &mut Data, t: &Timeline) {
    match d {
        Data::Gauge(g) => all(&mut g.data_points, t),
        Data::Sum(s) => all(&mut s.data_points, t),
        Data::Histogram(h) => all(&mut h.data_points, t),
        Data::ExponentialHistogram(h) => all(&mut h.data_points, t),
        Data::Summary(s) => all(&mut s.data_points, t),
    }
}

fn all<P: Stamped>(points: &mut [P], t: &Timeline) {
    points.iter_mut().for_each(|p| p.stamp(t));
}

/// A metric point whose start and time, and its exemplars' times, go to world time.
trait Stamped {
    fn stamp(&mut self, t: &Timeline);
}

macro_rules! stamped_with_exemplars {
    ($($point:ty),*) => {$(
        impl Stamped for $point {
            fn stamp(&mut self, t: &Timeline) {
                self.start_time_unix_nano = t.world_ns(self.start_time_unix_nano);
                self.time_unix_nano = t.world_ns(self.time_unix_nano);
                (self.exemplars.iter_mut()).for_each(|e| e.time_unix_nano = t.world_ns(e.time_unix_nano));
            }
        }
    )*};
}

stamped_with_exemplars!(
    NumberDataPoint,
    HistogramDataPoint,
    ExponentialHistogramDataPoint
);

impl Stamped for SummaryDataPoint {
    fn stamp(&mut self, t: &Timeline) {
        self.start_time_unix_nano = t.world_ns(self.start_time_unix_nano);
        self.time_unix_nano = t.world_ns(self.time_unix_nano);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ClockFile;
    use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use opentelemetry_proto::tonic::metrics::v1::{
        Exemplar, Gauge, Histogram, Metric, ResourceMetrics, ScopeMetrics,
    };
    use opentelemetry_proto::tonic::trace::v1::span::Event;
    use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};

    const W: i64 = 1_773_000_000_000;

    fn ns(ms: i64) -> u64 {
        ms as u64 * 1_000_000
    }

    fn clock() -> Clock {
        Clock::of(ClockFile {
            epoch_ms: 1_773_122_400_000,
            clock_ms: 60_000,
            speed: 1.0,
            wall_ms: W,
            world: true,
        })
    }

    fn one_span(start: u64, end: u64) -> ExportTraceServiceRequest {
        let event = Event {
            time_unix_nano: end,
            ..Event::default()
        };
        let span = Span {
            start_time_unix_nano: start,
            end_time_unix_nano: end,
            events: vec![event],
            ..Span::default()
        };
        let scope = ScopeSpans {
            spans: vec![span],
            ..ScopeSpans::default()
        };
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![scope],
                ..ResourceSpans::default()
            }],
        }
    }

    fn span_times(req: &ExportTraceServiceRequest) -> (u64, u64) {
        let s = &req.resource_spans[0].scope_spans[0].spans[0];
        (s.start_time_unix_nano, s.end_time_unix_nano)
    }

    #[test]
    fn restamped_spans_follow_the_world_clock() {
        let c = clock();
        let mut req = one_span(ns(W + 500), ns(W + 700));
        traces(&mut req, &c);
        assert_eq!(
            span_times(&req),
            (ns(1_773_122_460_500), ns(1_773_122_460_700))
        );
        let event = &req.resource_spans[0].scope_spans[0].spans[0].events[0];
        assert_eq!(event.time_unix_nano, ns(1_773_122_460_700));
    }

    #[test]
    fn log_records_get_world_time_and_an_unset_time_stays_unset() {
        let record = |time, observed| LogRecord {
            time_unix_nano: time,
            observed_time_unix_nano: observed,
            ..LogRecord::default()
        };
        let scope = ScopeLogs {
            log_records: vec![record(ns(W + 100), ns(W + 200)), record(0, ns(W))],
            ..ScopeLogs::default()
        };
        let mut req = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                scope_logs: vec![scope],
                ..ResourceLogs::default()
            }],
        };
        logs(&mut req, &clock());
        let got: Vec<(u64, u64)> = req.resource_logs[0].scope_logs[0]
            .log_records
            .iter()
            .map(|r| (r.time_unix_nano, r.observed_time_unix_nano))
            .collect();
        let world = |ms: i64| ns(1_773_122_460_000 + ms);
        assert_eq!(got, vec![(world(100), world(200)), (0, world(0))]);
    }

    fn metric(data: Data) -> Metric {
        Metric {
            name: "m".into(),
            data: Some(data),
            ..Metric::default()
        }
    }

    /// A gauge point with an exemplar.
    fn gauge() -> Metric {
        let exemplar = Exemplar {
            time_unix_nano: ns(W + 30),
            ..Exemplar::default()
        };
        let point = NumberDataPoint {
            start_time_unix_nano: ns(W - 1_000),
            time_unix_nano: ns(W + 10),
            exemplars: vec![exemplar],
            ..NumberDataPoint::default()
        };
        let data_points = vec![point];
        metric(Data::Gauge(Gauge { data_points }))
    }

    /// A histogram point without a start.
    fn histogram() -> Metric {
        let point = HistogramDataPoint {
            start_time_unix_nano: 0,
            time_unix_nano: ns(W + 20),
            ..HistogramDataPoint::default()
        };
        let (data_points, aggregation_temporality) = (vec![point], 2);
        metric(Data::Histogram(Histogram {
            data_points,
            aggregation_temporality,
        }))
    }

    fn two_points() -> ExportMetricsServiceRequest {
        let scope = ScopeMetrics {
            metrics: vec![gauge(), histogram()],
            ..ScopeMetrics::default()
        };
        let rm = ResourceMetrics {
            scope_metrics: vec![scope],
            ..ResourceMetrics::default()
        };
        ExportMetricsServiceRequest {
            resource_metrics: vec![rm],
        }
    }

    #[test]
    fn metric_points_and_exemplars_get_world_time() {
        let mut req = two_points();
        metrics(&mut req, &clock());
        let world = |ms: i64| ns(1_773_122_460_000 + ms);
        let m = &req.resource_metrics[0].scope_metrics[0].metrics;
        let Some(Data::Gauge(g)) = &m[0].data else {
            panic!()
        };
        let p = &g.data_points[0];
        assert_eq!(
            (p.start_time_unix_nano, p.time_unix_nano),
            (world(-1_000), world(10))
        );
        assert_eq!(p.exemplars[0].time_unix_nano, world(30));
        let Some(Data::Histogram(h)) = &m[1].data else {
            panic!()
        };
        let p = &h.data_points[0];
        assert_eq!((p.start_time_unix_nano, p.time_unix_nano), (0, world(20)));
    }
}
