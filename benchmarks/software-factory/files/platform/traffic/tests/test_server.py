import json
import urllib.error
import urllib.request

import pytest
from test_runner import EPOCH, SHOP, traffic

from traffic.server import start


@pytest.fixture
def probe(serve, tmp_path):
    base, _ = serve(SHOP)
    t, clock = traffic(tmp_path, base)
    t.tick()
    clock.clock_ms = 600_000
    t.tick()
    t.wait()
    server = start(t, port=0)
    yield t, f"http://127.0.0.1:{server.server_address[1]}"
    server.shutdown()
    server.server_close()


def get(url: str):
    try:
        with urllib.request.urlopen(url, timeout=5) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


def test_stats_give_the_totals_and_a_world_time_window_when_asked(probe):
    t, base = probe
    status, doc = get(f"{base}/stats")
    assert status == 200 and doc["requests"] == t.stats.totals()["requests"] > 0 and doc["clock"] == "file"
    assert {"errors_5xx", "timeouts", "connection_errors", "by_journey", "journeys", "telemetry"} <= set(doc)
    _, windowed = get(f"{base}/stats?window_s=60&time={(EPOCH + 600_000) // 1000}")
    assert windowed["window"] == t.stats.window(EPOCH + 600_000, 60_000)


def test_the_slo_is_read_at_the_world_time_the_probe_names(probe):
    t, base = probe
    _, doc = get(f"{base}/slo?time={(EPOCH + 600_000) // 1000}")
    assert doc == t.stats.slo(EPOCH + 600_000) and doc["requests_30m"] > 0 and doc["error_rate_30m"] == 0.0
    _, now = get(f"{base}/slo")
    assert now["at_world_ms"] == EPOCH + 600_000


def test_contracts_health_and_unknown_paths(probe):
    t, base = probe
    assert get(f"{base}/contracts") == (200, t.contracts_doc())
    assert get(f"{base}/healthz") == (200, {"ok": True})
    assert get(f"{base}/nope")[0] == 404
    assert get(f"{base}/slo?time=soon") == (400, {"error": "time must be world seconds, not 'soon'"})
