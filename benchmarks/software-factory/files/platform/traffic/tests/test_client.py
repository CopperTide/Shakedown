from conftest import Route

from traffic.client import request


def test_an_answer_gives_its_status_body_and_latency(serve):
    base, seen = serve({"POST /cart": Route({"id": "c-1"}, 201)})
    out = request("POST", f"{base}/cart", {"a": 1}, {"User-Agent": "t/1"}, timeout_s=2)
    assert (out.status, out.body, out.outcome) == (201, {"id": "c-1"}, 201) and out.latency_ms >= 0
    assert seen[0]["body"] == {"a": 1} and seen[0]["headers"]["user-agent"] == "t/1"


def test_a_server_error_is_an_answer_with_its_status(serve_json):
    base = serve_json({"/x": Route({"detail": "down"}, 503)})
    out = request("GET", f"{base}/x", timeout_s=2)
    assert (out.status, out.outcome, out.body) == (503, 503, {"detail": "down"})


def test_a_slow_answer_is_a_timeout(serve_json):
    base = serve_json({"/slow": Route({}, delay_s=0.6)})
    out = request("GET", f"{base}/slow", timeout_s=0.2)
    assert (out.status, out.outcome) == (None, "timeout") and 150 <= out.latency_ms < 600


def test_nobody_listening_is_a_failed_connection(closed_port):
    out = request("GET", f"{closed_port}/x", timeout_s=1)
    assert (out.status, out.outcome, out.body) == (None, "connection", None)


def test_a_body_that_is_not_json_is_kept_as_not_json(serve_json):
    out = request("GET", f"{serve_json({'/t': Route(b'<html>')})}/t", timeout_s=2)
    assert (out.status, out.body, out.json) == (200, None, False)
