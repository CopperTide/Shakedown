"""Fakes: a runpodctl that answers from the hand-written fixtures (see fixtures/README.md), a
vLLM gpt-oss server and a shaked as httpx transports, and operations wired to them in a temp
folder. Nothing here reaches RunPod, a model or the real home folder."""

from __future__ import annotations

import copy
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable, Optional

import httpx
import pytest

from gpupod.ledger import Ledger
from gpupod.pod import Ops
from gpupod.spec import load_spec

FIXTURES = Path(__file__).resolve().parent / "fixtures"
T0 = datetime(2026, 11, 19, 21, 29, tzinfo=timezone.utc)


def fixture(name: str) -> Any:
    return json.loads((FIXTURES / name).read_text())


class FakeRunner:
    """Answers runpodctl commands from ``answers`` (command words → JSON value or a function of
    the argv), and records every argv. Any other command fails the test."""

    def __init__(self) -> None:
        self.calls: list[list[str]] = []
        self.answers: dict[tuple[str, ...], Any] = {
            ("gpu", "list"): fixture("gpu_list.json"),
            ("pod", "create"): fixture("pod.json"),
            ("pod", "get"): fixture("pod.json"),
            ("pod", "start"): {**fixture("pod.json"), "desiredStatus": "RUNNING"},
            ("pod", "stop"): {**fixture("pod.json"), "desiredStatus": "EXITED"},
            ("pod", "delete"): {"deleted": True},
            ("billing", "pods"): fixture("billing.json"),
            ("user",): fixture("user.json"),
        }

    def run(self, argv: list[str]) -> str:
        self.calls.append(list(argv))
        for n in (2, 1):
            key = tuple(argv[:n])
            if key in self.answers:
                a = self.answers[key]
                return json.dumps(a(argv) if callable(a) else copy.deepcopy(a))
        raise AssertionError(f"unexpected runpodctl call: {argv}")

    def commands(self) -> list[tuple[str, ...]]:
        return [tuple(c[:2]) for c in self.calls]

    def set_gpu(self, gpu_id: str, **fields: Any) -> None:
        gpus = self.answers[("gpu", "list")]
        for g in gpus:
            if g["gpuId"] == gpu_id:
                g.update(fields)


class Clock:
    def __init__(self, start: datetime = T0):
        self.t = start

    def now(self) -> datetime:
        return self.t

    def sleep(self, s: float) -> None:
        self.t += timedelta(seconds=s)


def vllm(key: str, reasoning: bool = True, healthy: bool = True,
         metrics: str = "") -> Callable[[httpx.Request], httpx.Response]:
    """A vLLM v0.32.0 gpt-oss server: /health, /metrics, and /v1 behind ``key``."""

    def handle(req: httpx.Request) -> httpx.Response:
        path = req.url.path
        if path.endswith("/health"):
            return httpx.Response(200 if healthy else 503)
        if path.endswith("/metrics"):
            return httpx.Response(200, text=metrics)
        if req.headers.get("authorization") != f"Bearer {key}":
            return httpx.Response(401, json={"error": "Unauthorized"})
        if path.endswith("/v1/models"):
            return httpx.Response(200, json={"object": "list", "data": [
                {"id": "openai/gpt-oss-120b", "object": "model", "owned_by": "vllm"}]})
        return httpx.Response(200, json=_chat(json.loads(req.content), reasoning))

    return handle


def _chat(body: dict, reasoning: bool) -> dict:
    msgs = body["messages"]
    if body.get("tools") and msgs[-1]["role"] != "tool":
        message = {"role": "assistant", "content": None, "tool_calls": [{
            "id": "chatcmpl-tool-1", "type": "function",
            "function": {"name": "get_time", "arguments": "{}"}}]}
        finish, usage = "tool_calls", (120, 30)
    else:
        message = {"role": "assistant", "content": "It is 21:30 UTC."}
        finish, usage = "stop", (140, body.get("max_tokens") or 40)
    if reasoning:
        message["reasoning"] = "The user wants the time; call get_time."
    return {"id": "chatcmpl-1", "object": "chat.completion", "model": "openai/gpt-oss-120b",
            "choices": [{"index": 0, "finish_reason": finish, "message": message}],
            "usage": {"prompt_tokens": usage[0], "completion_tokens": usage[1],
                      "total_tokens": sum(usage)}}


def shaked(runs: list[dict]) -> Callable[[httpx.Request], httpx.Response]:
    def handle(req: httpx.Request) -> httpx.Response:
        if req.url.path == "/api/runs":
            return httpx.Response(200, json=runs)
        return httpx.Response(404, json={"error": "no such route"})

    return handle


POD_HOST = "p7h9k2m4n6-8000.proxy.runpod.net"
SHAKED_HOST = "127.0.0.1"


def route(hosts: Optional[dict[str, Callable[[httpx.Request], httpx.Response]]] = None) -> httpx.Client:
    """One client whose requests go to the handler of their host (any other host: 502)."""
    hosts = hosts or {}

    def handle(req: httpx.Request) -> httpx.Response:
        h = hosts.get(req.url.host)
        return h(req) if h else httpx.Response(502, text=f"no fake for {req.url.host}")

    return httpx.Client(transport=httpx.MockTransport(handle))


class Spawned:
    def __init__(self) -> None:
        self.argvs: list[list[str]] = []

    def __call__(self, argv: list[str], log: Path) -> int:
        self.argvs.append(argv)
        return 4242


@pytest.fixture
def runner() -> FakeRunner:
    return FakeRunner()


@pytest.fixture
def clock() -> Clock:
    return Clock()


def make_ops(tmp_path: Path, runner: FakeRunner, clock: Clock, http: Optional[httpx.Client] = None,
             out: Optional[list[str]] = None) -> Ops:
    lines = out if out is not None else []
    return Ops(runner=runner, ledger=Ledger(tmp_path / "ledger.jsonl", now=clock.now),
               spec=load_spec(), env_path=tmp_path / "config" / "runpod.env",
               state_dir=tmp_path / "state", http=http or route(), out=lines.append,
               now=clock.now, sleep=clock.sleep, spawn=Spawned(),
               shaked_url="http://127.0.0.1:4510")
