"""The smoke test of a started pod, before any run uses it: vLLM is healthy, its ``/v1`` needs
the key, gpt-oss calls a tool with its reasoning in ``message.reasoning`` and answers after the
tool's result, and its decode speed; optionally, that a call through shaked's gateway records
the reasoning as a ``reasoning`` event of the call."""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

import httpx

from .pod import Ops

GET_TIME = {"type": "function", "function": {
    "name": "get_time", "description": "The current time.",
    "parameters": {"type": "object", "properties": {}}}}
ASK = [{"role": "user", "content": "What time is it? Use the tool."}]
#: The key shaked's gateway takes from a client (it holds the real one).
GATEWAY_KEY = "shakedown"


@dataclass
class Check:
    name: str
    ok: bool
    detail: str = ""


@dataclass
class SmokeReport:
    checks: list[Check] = field(default_factory=list)
    decode_tok_s: Optional[float] = None

    @property
    def ok(self) -> bool:
        return bool(self.checks) and all(c.ok for c in self.checks)

    def lines(self) -> list[str]:
        return [f"{'ok  ' if c.ok else 'FAIL'} {c.name}" + (f": {c.detail}" if c.detail else "")
                for c in self.checks]


def _thought(message: dict) -> str:
    return message.get("reasoning") or message.get("reasoning_content") or ""


class _Smoke:
    def __init__(self, ops: Ops, base: str, key: str, model: str):
        self.http, self.base, self.key, self.model = ops.http, base, key, model
        self.report = SmokeReport()

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        self.report.checks.append(Check(name, ok, "" if ok else detail))
        return ok

    def chat(self, body: dict) -> dict:
        r = self.http.post(f"{self.base}/v1/chat/completions", json={"model": self.model, **body},
                           headers={"authorization": f"Bearer {self.key}"}, timeout=100)
        if r.status_code != 200:
            raise ValueError(f"answered {r.status_code}: {r.text[:200]}")
        return r.json()

    def models(self) -> None:
        h = self.http.get(f"{self.base}/health", timeout=30)
        self.check("health", h.status_code == 200, f"/health answered {h.status_code}")
        r = self.http.get(f"{self.base}/v1/models", headers={"authorization": f"Bearer {self.key}"})
        ids = [m.get("id") for m in r.json().get("data", [])] if r.status_code == 200 else []
        self.check("models with the key", self.model in ids, f"answered {r.status_code}, models {ids}")
        bare = self.http.get(f"{self.base}/v1/models")
        self.check("models without the key", bare.status_code == 401,
                   f"answered {bare.status_code}, not 401: /v1 is open to anyone")

    def tool_round(self) -> None:
        try:
            msg = self.chat({"messages": ASK, "tools": [GET_TIME], "tool_choice": "auto",
                             "max_tokens": 1024})["choices"][0]["message"]
        except (ValueError, KeyError, httpx.HTTPError) as e:
            self.check("tool call with reasoning", False, str(e))
            self.check("answer after the tool result", False, "skipped: no tool call")
            return
        calls = msg.get("tool_calls") or []
        called = bool(calls) and calls[0]["function"]["name"] == "get_time"
        detail = "no get_time tool call" if not called else "no message.reasoning in the answer"
        self.check("tool call with reasoning", called and bool(_thought(msg)), detail)
        if called:
            self.after_tool(msg, calls)
        else:
            self.check("answer after the tool result", False, "skipped: no tool call")

    def after_tool(self, msg: dict, calls: list) -> None:
        result = {"role": "tool", "tool_call_id": calls[0]["id"], "content": "21:30 UTC"}
        try:
            ans = self.chat({"messages": [*ASK, msg, result], "tools": [GET_TIME], "max_tokens": 1024})
            content = ans["choices"][0]["message"].get("content") or ""
        except (ValueError, KeyError, httpx.HTTPError) as e:
            content = f"error: {e}"
        self.check("answer after the tool result", bool(content) and not content.startswith("error: "),
                   content or "empty content")

    def decode(self) -> None:
        body = {"messages": [{"role": "user", "content": "Write a long story about a lighthouse."}],
                "max_tokens": 512, "reasoning_effort": "low"}
        t0 = time.perf_counter()
        try:
            tokens = self.chat(body)["usage"]["completion_tokens"]
        except (ValueError, KeyError, httpx.HTTPError) as e:
            self.check("decode speed", False, str(e))
            return
        self.report.decode_tok_s = round(tokens / max(time.perf_counter() - t0, 1e-6), 1)
        self.check("decode speed", tokens > 0, "no completion tokens")


def smoke(ops: Ops, base: str, key: str, model: str, through_shaked: Optional[str] = None,
          arena_factory: Optional[Callable[[str], Any]] = None) -> SmokeReport:
    s = _Smoke(ops, base, key, model)
    s.models()
    s.tool_round()
    s.decode()
    if through_shaked:
        _through_shaked(s, through_shaked, arena_factory or _arena, ops.spec.shaked_provider)
    return s.report


def _arena(url: str) -> Any:
    from shakedown import Shakedown

    return Shakedown(url)


def _through_shaked(s: _Smoke, url: str, factory: Callable[[str], Any], provider: str) -> None:
    """One gateway call of an external run of the benchmark's first task; the run is stopped
    (unscored) and deleted after."""
    arena = factory(url)
    branch = arena.external_run(arena.tasks()[0])
    try:
        a = branch.register("smoke", kind="external", provider=f"local:{provider}", model=s.model)
        s.http.post(f"{a.gateway_url}/chat/completions", timeout=100,
                    headers={"authorization": f"Bearer {GATEWAY_KEY}"},
                    json={"model": s.model, "messages": ASK, "tools": [GET_TIME], "max_tokens": 1024})
        found = any(e.kind == "reasoning" and (e.payload or {}).get("source") == "gateway"
                    and (e.payload or {}).get("model_call_seq") is not None for e in branch.record().events)
        s.check("reasoning recorded through shaked", found,
                "no reasoning event from the gateway with its model_call_seq")
    finally:
        branch.stop()
        arena.delete_run(branch.run_id)
