from traffic.otlp import BOUNDS, Telemetry, attributes, traceparent

RESOURCE = {"service.name": "traffic", "deployment.environment.name": "production"}


def telemetry(endpoint=None, cap=100) -> Telemetry:
    return Telemetry(RESOURCE, endpoint, start_ns=1_000, cap=cap)


def test_attributes_are_typed_as_otlp_json_wants():
    assert attributes({"a": "x", "b": 3, "c": 0.5, "d": True}) == [
        {"key": "a", "value": {"stringValue": "x"}}, {"key": "b", "value": {"intValue": "3"}},
        {"key": "c", "value": {"doubleValue": 0.5}}, {"key": "d", "value": {"boolValue": True}}]


def test_spans_are_sent_as_otlp_json_with_hex_ids_and_client_kind():
    t = telemetry()
    trace, root, child = t.trace_id(), t.span_id(), t.span_id()
    t.span("journey order", trace, root, None, 10, 90, {"journey": "order"}, internal=True)
    t.span("POST /checkout", trace, child, root, 20, 80, {"http.response.status_code": 502}, error=True)
    spans = t.bodies(100)["traces"]["resourceSpans"][0]["scopeSpans"][0]["spans"]
    assert [(s["name"], s["kind"], s["status"]["code"]) for s in spans] == [("journey order", 1, 0),
                                                                             ("POST /checkout", 3, 2)]
    assert len(spans[1]["traceId"]) == 32 and len(spans[1]["spanId"]) == 16 and spans[1]["parentSpanId"] == root
    assert (spans[1]["startTimeUnixNano"], spans[1]["endTimeUnixNano"]) == ("20", "80")
    assert "parentSpanId" not in spans[0] and t.bodies(200)["traces"] is None   # drained
    assert traceparent(trace, child) == f"00-{trace}-{child}-01"


def test_request_durations_are_one_cumulative_histogram_per_attribute_set():
    t = telemetry()
    t.duration({"journey": "browse", "http.response.status_code": 200}, 0.004)
    t.duration({"journey": "browse", "http.response.status_code": 200}, 0.3)
    first = t.bodies(500)["metrics"]["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]
    t.duration({"journey": "browse", "http.response.status_code": 200}, 20.0)
    second = t.bodies(900)["metrics"]["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]
    assert (first["name"], first["unit"], first["histogram"]["aggregationTemporality"]) == \
        ("http.client.request.duration", "s", 2)
    point = second["histogram"]["dataPoints"][0]
    assert point["explicitBounds"] == list(BOUNDS) and len(point["bucketCounts"]) == len(BOUNDS) + 1
    assert (point["count"], point["bucketCounts"][0], point["bucketCounts"][-1]) == ("3", "1", "1")
    assert (point["startTimeUnixNano"], point["timeUnixNano"], point["sum"]) == ("1000", "900", 20.304)


def test_logs_carry_their_trace_and_severity():
    t = telemetry()
    t.log("WARN", "checkout failed: 502", {"journey": "order"}, 50, "a" * 32, "b" * 16)
    record = t.bodies(60)["logs"]["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]
    assert (record["severityText"], record["severityNumber"], record["body"]) == \
        ("WARN", 13, {"stringValue": "checkout failed: 502"})
    assert (record["traceId"], record["spanId"], record["timeUnixNano"]) == ("a" * 32, "b" * 16, "50")


def test_a_full_buffer_drops_new_spans_and_counts_them():
    t = telemetry(cap=2)
    for n in range(5):
        t.span(f"s{n}", t.trace_id(), t.span_id(), None, 1, 2, {})
    spans = t.bodies(3)["traces"]["resourceSpans"][0]["scopeSpans"][0]["spans"]
    assert [s["name"] for s in spans] == ["s0", "s1"] and t.counts["dropped"] == 3


def test_export_posts_each_signal_to_the_collector(serve):
    base, seen = serve({"POST /v1/traces": {}, "POST /v1/metrics": {}, "POST /v1/logs": {}})
    t = telemetry(base)
    t.span("x", t.trace_id(), t.span_id(), None, 1, 2, {})
    t.duration({"journey": "browse"}, 0.1)
    t.export()
    assert sorted(r["path"] for r in seen) == ["/v1/metrics", "/v1/traces"]
    assert seen[0]["body"]["resourceSpans"] or seen[0]["body"]["resourceMetrics"]
    assert t.counts == {"sent": 2, "failed": 0, "dropped": 0}


def test_without_a_collector_nothing_is_sent(closed_port):
    t = telemetry(None)
    t.duration({"journey": "browse"}, 0.1)
    t.export()
    assert t.counts["sent"] == 0
    failing = telemetry(closed_port)
    failing.duration({"journey": "browse"}, 0.1)
    failing.export()
    assert failing.counts["failed"] == 1
