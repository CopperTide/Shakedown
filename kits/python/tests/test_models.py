"""The model client of parts and benchmarks, against a fake gateway on this machine."""
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from shakedown_part import Benchmark, Part, models
from shakedown_part.benchmark import dispatch as bench_dispatch
from shakedown_part.models import ModelError, Models
from shakedown_part.serve import dispatch

ANSWER = {"choices": [{"message": {"role": "assistant", "content": "hello"}}], "usage": {"total_tokens": 3}}


class Gateway:
    """A fake gateway: answers each request with the next of `script` (status, body, headers) and keeps what it
    was sent."""

    def __init__(self, script):
        self.script = list(script)
        self.seen = []
        gw = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                gw.seen.append({"path": self.path, "headers": dict(self.headers), "body": body})
                status, answer, headers = gw.script.pop(0)
                data = json.dumps(answer).encode()
                self.send_response(status)
                for k, v in headers.items():
                    self.send_header(k, v)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.base_url = f"http://127.0.0.1:{self.server.server_port}/gateway/b1/world.shop/v1"

    def close(self):
        self.server.shutdown()
        self.server.server_close()


@pytest.fixture
def gateway(request):
    gw = Gateway(request.param)
    yield gw
    gw.close()


def client(gw):
    return Models(gw.base_url, "sdw_t", backoff_s=0.0)


@pytest.mark.parametrize("gateway", [[(200, ANSWER, {})]], indirect=True)
def test_a_call_goes_to_chat_completions_with_the_token_provider_and_actor(gateway):
    tools = [{"type": "function", "function": {"name": "f"}}]
    msgs = [{"role": "user", "content": "hi"}]
    got = client(gateway).chat(msgs, "m1", tools=tools, provider="openai", actor="customer", temperature=0)
    assert got == ANSWER
    seen = gateway.seen[0]
    assert seen["path"] == "/gateway/b1/world.shop/v1/chat/completions"
    assert seen["headers"]["Authorization"] == "Bearer sdw_t"
    assert (seen["headers"]["X-Shakedown-Provider"], seen["headers"]["X-Shakedown-Actor"]) == ("openai", "customer")
    assert seen["body"] == {"model": "m1", "messages": msgs, "temperature": 0, "tools": tools}


@pytest.mark.parametrize("gateway", [[(200, ANSWER, {})]], indirect=True)
def test_no_provider_actor_or_tools_sends_none(gateway):
    client(gateway).chat([], "m1")
    seen = gateway.seen[0]
    assert "X-Shakedown-Provider" not in seen["headers"] and "X-Shakedown-Actor" not in seen["headers"]
    assert seen["body"] == {"model": "m1", "messages": []}


@pytest.mark.parametrize("gateway", [[(429, {"error": "slow down"}, {"Retry-After": "0"}),
                                      (503, {"error": "busy"}, {}), (200, ANSWER, {})]], indirect=True)
def test_rate_limits_and_overload_are_tried_again(gateway):
    assert client(gateway).chat([], "m1") == ANSWER
    assert len(gateway.seen) == 3


@pytest.mark.parametrize("gateway", [[(400, {"error": "bad model"}, {})]], indirect=True)
def test_a_refusal_is_a_model_error_at_once(gateway):
    with pytest.raises(ModelError) as e:
        client(gateway).chat([], "m1")
    assert (e.value.status, e.value.body) == (400, {"error": "bad model"})
    assert "bad model" in str(e.value) and len(gateway.seen) == 1


@pytest.mark.parametrize("gateway", [[(502, {"error": "down"}, {})] * 3], indirect=True)
def test_a_failure_that_lasts_is_a_model_error_after_the_retries(gateway):
    m = Models(gateway.base_url, "sdw_t", retries=2, backoff_s=0.0)
    with pytest.raises(ModelError) as e:
        m.chat([], "m1")
    assert e.value.status == 502 and len(gateway.seen) == 3


def test_a_gateway_that_cannot_be_reached_is_a_model_error():
    m = Models("http://127.0.0.1:9/v1", "t", retries=1, backoff_s=0.0, timeout_s=2)
    with pytest.raises(ModelError) as e:
        m.chat([], "m1")
    assert e.value.status is None


def test_models_params_make_a_client_and_none_make_none():
    m = Models.of({"base_url": "http://h/v1/", "token": "sdw_x"})
    assert (m.base_url, m.token) == ("http://h/v1", "sdw_x")
    assert Models.of(None) is None and Models.of({"base_url": "http://h"}) is None


class Shop(Part):
    def start(self, instance, config, seed, clock_ms, from_=None):
        self.started = instance


def test_a_part_keeps_each_instances_models_from_start():
    p = Shop()
    models_given = {"base_url": "http://h/gateway/b/world.shop/v1", "token": "sdw_x"}
    dispatch(p, "start", {"instance": "i1", "config": {}, "seed": 1, "clock_ms": 0, "models": models_given})
    dispatch(p, "start", {"instance": "i2", "config": {}, "seed": 1, "clock_ms": 0})
    assert p.models("i1").base_url == models_given["base_url"] and p.models("i1").token == "sdw_x"
    assert p.models("i2") is None and p.models("nope") is None


class Judged(Benchmark):
    def score(self, task, docs):
        m = self.models()
        return {"checks": [{"name": "judge", "passed": m is not None, "detail": m.base_url if m else ""}]}


def test_a_benchmark_has_the_score_calls_models_only_while_it_scores():
    b = Judged()
    given = {"base_url": "http://h/gateway/b/world.scoring/v1", "token": "sdw_x"}
    out = bench_dispatch(b, "score", {"task": "t", "docs": {}, "models": given})
    assert out["checks"][0] == {"name": "judge", "passed": True, "detail": given["base_url"]}
    assert b.models() is None
    assert bench_dispatch(b, "score", {"task": "t", "docs": {}})["checks"][0]["passed"] is False


def test_the_module_is_exported():
    assert models.Models is Models
