"""Traces, metrics and logs as OTLP/HTTP JSON to the platform's collector (`OTEL_EXPORTER_OTLP_ENDPOINT`), stamped
with wall time like the apps' (the platform's gateway restamps them to world time). A journey is a trace: an
internal span with a client span per request, whose `traceparent` the apps continue. Request durations are the
OpenTelemetry client histogram `http.client.request.duration` (cumulative, the apps' second buckets). Spans and
logs wait in a capped buffer between exports; what does not fit is dropped and counted."""
import bisect
import os
import threading
import time

from .client import request

BOUNDS = (0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0)
SCOPE = {"name": "traffic", "version": "1"}
SEVERITY = {"INFO": 9, "WARN": 13, "ERROR": 17}
KIND_INTERNAL, KIND_CLIENT, STATUS_ERROR = 1, 3, 2


def value(v) -> dict:
    if isinstance(v, bool):
        return {"boolValue": v}
    if isinstance(v, int):
        return {"intValue": str(v)}
    if isinstance(v, float):
        return {"doubleValue": v}
    return {"stringValue": str(v)}


def attributes(attrs: dict) -> list[dict]:
    return [{"key": k, "value": value(v)} for k, v in attrs.items()]


def traceparent(trace_id: str, span_id: str) -> str:
    return f"00-{trace_id}-{span_id}-01"


class Telemetry:
    def __init__(self, resource: dict, endpoint: str | None, start_ns: int | None = None, cap: int = 2048):
        self.resource = {"attributes": attributes(resource)}
        self.endpoint = endpoint.rstrip("/") if endpoint else None
        self.start_ns, self.cap = start_ns or time.time_ns(), cap
        self._spans: list[dict] = []
        self._logs: list[dict] = []
        self._histograms: dict[tuple, list] = {}
        self.counts = {"sent": 0, "failed": 0, "dropped": 0}
        self._lock = threading.Lock()

    @staticmethod
    def trace_id() -> str:
        return os.urandom(16).hex()

    @staticmethod
    def span_id() -> str:
        return os.urandom(8).hex()

    def span(self, name, trace_id, span_id, parent_id, start_ns, end_ns, attrs, error=False, internal=False):
        span = {"traceId": trace_id, "spanId": span_id, "name": name,
                "kind": KIND_INTERNAL if internal else KIND_CLIENT, "startTimeUnixNano": str(start_ns),
                "endTimeUnixNano": str(end_ns), "attributes": attributes(attrs),
                "status": {"code": STATUS_ERROR if error else 0}}
        if parent_id:
            span["parentSpanId"] = parent_id
        self._buffer(self._spans, span)

    def log(self, severity, text, attrs, time_ns, trace_id=None, span_id=None):
        record = {"timeUnixNano": str(time_ns), "observedTimeUnixNano": str(time_ns), "severityText": severity,
                  "severityNumber": SEVERITY[severity], "body": {"stringValue": text}, "attributes": attributes(attrs)}
        if trace_id:
            record.update(traceId=trace_id, spanId=span_id)
        self._buffer(self._logs, record)

    def duration(self, attrs: dict, seconds: float) -> None:
        key = tuple(sorted(attrs.items()))
        with self._lock:
            h = self._histograms.setdefault(key, [[0] * (len(BOUNDS) + 1), 0.0, 0])
            h[0][bisect.bisect_left(BOUNDS, seconds)] += 1
            h[1] += seconds
            h[2] += 1

    def bodies(self, now_ns: int) -> dict:
        """The three export bodies (None for a signal with nothing to send); spans and logs are drained."""
        with self._lock:
            spans, logs, self._spans, self._logs = self._spans, self._logs, [], []
            points = [self._point(dict(k), h, now_ns) for k, h in self._histograms.items()]
        return {"traces": self._wrap("resourceSpans", "scopeSpans", "spans", spans),
                "metrics": self._wrap("resourceMetrics", "scopeMetrics", "metrics", [{
                    "name": "http.client.request.duration", "unit": "s",
                    "description": "Time a synthetic user waited for an answer",
                    "histogram": {"aggregationTemporality": 2, "dataPoints": points}}] if points else []),
                "logs": self._wrap("resourceLogs", "scopeLogs", "logRecords", logs)}

    def export(self) -> None:
        if not self.endpoint:
            return
        paths = {"traces": "/v1/traces", "metrics": "/v1/metrics", "logs": "/v1/logs"}
        for signal, body in self.bodies(time.time_ns()).items():
            if body is not None:
                out = request("POST", self.endpoint + paths[signal], body, timeout_s=5)
                self.counts["sent" if out.status is not None and 200 <= out.status < 300 else "failed"] += 1

    def _buffer(self, buffer: list, item: dict) -> None:
        with self._lock:
            if len(buffer) < self.cap:
                buffer.append(item)
            else:
                self.counts["dropped"] += 1

    def _point(self, attrs: dict, h: list, now_ns: int) -> dict:
        return {"attributes": attributes(attrs), "startTimeUnixNano": str(self.start_ns),
                "timeUnixNano": str(now_ns), "count": str(h[2]), "sum": h[1],
                "bucketCounts": [str(c) for c in h[0]], "explicitBounds": list(BOUNDS)}

    def _wrap(self, outer: str, scoped: str, items_key: str, items: list) -> dict | None:
        if not items:
            return None
        return {outer: [{"resource": self.resource, scoped: [{"scope": SCOPE, items_key: items}]}]}
