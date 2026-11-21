from traffic.stats import Stats

MIN = 60_000


def filled(*records, retention_ms=6 * 60 * MIN) -> Stats:
    stats = Stats(retention_ms=retention_ms, slo_window_ms=30 * MIN, objective=0.01)
    for r in records:
        stats.record(*r)
    return stats


def test_totals_count_server_errors_timeouts_and_failed_connections_apart_from_client_errors():
    stats = filled((1_000, "browse", 200, 40), (2_000, "order", 503, 10), (3_000, "order", "timeout", 2_000),
                   (4_000, "order", "connection", 1), (5_000, "order", 409, 30))
    doc = stats.totals()
    assert {k: doc[k] for k in ("requests", "errors_5xx", "timeouts", "connection_errors", "errors")} == \
        {"requests": 5, "errors_5xx": 1, "timeouts": 1, "connection_errors": 1, "errors": 3}
    assert doc["by_journey"] == {"browse": {"requests": 1, "errors": 0}, "order": {"requests": 4, "errors": 3}}


def test_a_window_counts_only_requests_in_its_world_time_and_gives_the_error_rate_and_latency():
    stats = filled(*[(t * 1_000, "browse", 200, t) for t in range(1, 101)], (101_000, "browse", 500, 900))
    doc = stats.window(to_ms=101_000, length_ms=51_000)    # (50 s, 101 s]: requests 51..101
    assert (doc["requests"], doc["errors"], doc["from_world_ms"], doc["to_world_ms"]) == (51, 1, 50_000, 101_000)
    assert doc["error_rate"] == 1 / 51 and doc["p50_ms"] == 76 and doc["p95_ms"] == 99


def test_the_slo_is_the_last_thirty_world_minutes_against_the_objective():
    stats = filled((1 * MIN, "order", 500, 10), (40 * MIN, "browse", 200, 20), (41 * MIN, "browse", 200, 30))
    assert stats.slo(at_ms=45 * MIN) == {"error_rate_30m": 0.0, "p95_ms": 30, "requests_30m": 2, "errors_30m": 0,
                                         "objective": 0.01, "meets_objective": True, "at_world_ms": 45 * MIN}


def test_with_no_requests_the_rate_and_latency_are_unknown_not_zero():
    doc = filled().slo(at_ms=30 * MIN)
    assert (doc["error_rate_30m"], doc["p95_ms"], doc["requests_30m"], doc["meets_objective"]) == (None, None, 0, None)


def test_records_past_the_retention_are_dropped_but_still_counted_in_the_totals():
    stats = filled((1_000, "browse", 500, 5), (10 * MIN, "browse", 200, 5), retention_ms=5 * MIN)
    assert stats.window(to_ms=10 * MIN, length_ms=60 * MIN)["requests"] == 1
    assert stats.totals()["requests"] == 2 and stats.totals()["errors"] == 1
