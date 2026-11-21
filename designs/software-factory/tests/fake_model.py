"""A scripted OpenAI-compatible server in the shape vLLM v0.32.0 answers for gpt-oss (the plan's
Task 1 shape), and stand-ins for a driver's run and agents. Tests only: never in a run.

    with FakeModel({"coder": [tool_call("shell", cmd="ls"), say("Done.")]}) as fake:
        run = FakeRun(fake)
        ...
        fake.requests  # [(agent, body), ...]

It also stands in for the model server behind shaked's gateway: ``POST /v1/chat/completions``
(agent ``""``). ``python tests/fake_model.py --port N`` serves :func:`look_then_answer` there, so
both designs run end to end against a real local world with no real model:
``SHAKEDOWN_LOCAL_MODELS=runpod=http://127.0.0.1:N/v1`` for shaked.
"""

from __future__ import annotations

import itertools
import json
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable, Optional, Union

MODEL = "openai/gpt-oss-120b"
USAGE = {"prompt_tokens": 812, "completion_tokens": 64, "total_tokens": 876,
         "prompt_tokens_details": {"cached_tokens": 768}}
_ids = itertools.count(1)

Script = Union[dict[str, list[dict]], Callable[[str, dict], dict]]


def _completion(message: dict, finish: str) -> dict:
    n = next(_ids)
    return {"id": f"chatcmpl-{n}", "object": "chat.completion", "created": 1, "model": MODEL,
            "choices": [{"index": 0, "finish_reason": finish, "message": message}],
            "usage": dict(USAGE)}


def tool_call(name: str, reasoning: str = "I will use a tool.", **args: Any) -> dict:
    """An answer that calls tool ``name`` with ``args``."""
    call = {"id": f"chatcmpl-tool-{next(_ids)}", "type": "function",
            "function": {"name": name, "arguments": json.dumps(args)}}
    message = {"role": "assistant", "content": None, "reasoning": reasoning, "tool_calls": [call]}
    return _completion(message, "tool_calls")


def say(text: str, reasoning: str = "I have the answer.") -> dict:
    """An answer with final text ``text``."""
    return _completion({"role": "assistant", "content": text, "reasoning": reasoning}, "stop")


#: The one look every conversation takes: a read through the agent's own gh (the world's code host).
LOOK = "gh api user --jq .login"
#: The answer after it, with every key a role of either design parses (plan, coder, reviewer).
ANSWER = {"items": [{"ticket": "", "repo": "", "summary": "Look at the sprint.", "risks": [], "contracts": []}],
          "pr": "none", "verdict": "approve", "notes": "the scripted model looked once and answered"}


def look_then_answer(agent: str, body: dict) -> dict:
    """A conversation's first turn calls ``shell`` with :data:`LOOK` (when it has that tool); a turn
    after any tool result answers :data:`ANSWER` as JSON text. Stateless, so any agent of any design
    can be served by it with no script of its own."""
    shell = any(t.get("function", {}).get("name") == "shell" for t in body.get("tools") or [])
    looked = any(m.get("role") == "tool" for m in body.get("messages", []))
    if shell and not looked:
        return tool_call("shell", reasoning="I will look at the code host first.", cmd=LOOK)
    return say(json.dumps(ANSWER), reasoning="I looked; this is my answer.")


def status(code: int, body: Union[str, dict]) -> dict:
    """An error answer: ``body`` as JSON, or as HTML when it is a string."""
    return {"__status__": code, "body": body}


class FakeModel:
    """Answers ``POST /gateway/<b>/<agent>/v1/chat/completions`` with the next scripted answer
    for ``<agent>`` (the last one repeats), or with ``script(agent, body)``."""

    def __init__(self, script: Script, port: int = 0):
        self.script = script
        self.requests: list[tuple[str, dict]] = []
        self._next: dict[str, int] = {}
        self._lock = threading.Lock()
        self._server = ThreadingHTTPServer(("127.0.0.1", port), _handler(self))
        self.url = f"http://127.0.0.1:{self._server.server_address[1]}"

    def __enter__(self) -> "FakeModel":
        threading.Thread(target=self._server.serve_forever, daemon=True).start()
        return self

    def __exit__(self, *exc: Any) -> None:
        self._server.shutdown()
        self._server.server_close()

    def answer(self, agent: str, body: dict) -> dict:
        with self._lock:
            self.requests.append((agent, body))
            if callable(self.script):
                return self.script(agent, body)
            answers = self.script[agent]
            i = self._next.get(agent, 0)
            self._next[agent] = i + 1
            return answers[min(i, len(answers) - 1)]

    def bodies(self, agent: str) -> list[dict]:
        return [b for a, b in self.requests if a == agent]

    def starts(self) -> list[str]:
        """The agent of each conversation's first request (one with no assistant turn yet), in
        order: an agent invoked twice appears twice."""
        return [a for a, b in self.requests
                if not any(m.get("role") == "assistant" for m in b.get("messages", []))]


def _handler(fake: FakeModel) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:  # noqa: N802 - the stdlib's name
            parts = self.path.strip("/").split("/")
            gateway = len(parts) == 6 and parts[0] == "gateway" and parts[3:] == ["v1", "chat", "completions"]
            if not gateway and parts != ["v1", "chat", "completions"]:
                return self._send(404, {"error": f"no route {self.path}"})
            body = json.loads(self.rfile.read(int(self.headers.get("content-length") or 0)))
            ans = fake.answer(parts[2] if gateway else "", body)
            if "__status__" in ans:
                return self._send(ans["__status__"], ans["body"])
            self._send(200, ans)

        def _send(self, code: int, body: Union[str, dict]) -> None:
            html = isinstance(body, str)
            data = (body if html else json.dumps(body)).encode()
            self.send_response(code)
            self.send_header("content-type", "text/html" if html else "application/json")
            self.send_header("content-length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *_: Any) -> None:
            pass

    return Handler


class FakeEndpoint:
    """An agent of a :class:`FakeRun`, with the surface the designs use of ``AgentEndpoint``."""

    def __init__(self, run: "FakeRun", id: str, spec: dict):
        self.run = run
        self.id = id
        self.spec = spec
        self.gateway_url = f"{run.fake.url}/gateway/b1/{id}/v1"
        self.env: dict[str, str] = dict(run.agent_env)
        self.proxy_url: Optional[str] = None
        self.ended = False

    def report_reasoning(self, text: Optional[str] = None, kind: str = "thinking", **_: Any) -> list[int]:
        self.run.reasoning.append((self.id, kind, text or ""))
        return [len(self.run.reasoning)]

    def end(self, ok: bool = True, error: Optional[str] = None, **_: Any) -> int:
        self.ended = True
        return 0


class _FakeBranch:
    def __init__(self, run_start: dict):
        self.run_start = run_start

    def record(self) -> "_FakeBranch":
        return self


class FakeRun:
    """Stands in for ``DriverRun``: registers :class:`FakeEndpoint`\\ s, records reasoning, and
    has a world clock a test moves with :meth:`advance`."""

    def __init__(self, fake: FakeModel, ticket: str = "Work the sprint of acme.",
                 agent_env: Optional[dict[str, str]] = None, clock_ms: int = 0,
                 world_minutes: Optional[float] = 60.0):
        self.fake = fake
        self.branch = _FakeBranch({"task_id": "sprint", "world_minutes": world_minutes})
        self.ticket = ticket
        self.agent_env = agent_env or {}
        self.registered: dict[str, dict] = {}
        self.reasoning: list[tuple[str, str, str]] = []
        self._agents: dict[str, FakeEndpoint] = {}
        self._clock_ms = clock_ms
        self._lock = threading.Lock()
        self.epoch_ms = int(datetime(2026, 11, 19, 9, 0, tzinfo=timezone.utc).timestamp() * 1000)

    def agent(self, id: str, *, provider: Optional[str] = None, model: Optional[str] = None,
              name: Optional[str] = None, read_only: bool = False) -> FakeEndpoint:
        if id not in self._agents:
            spec = {"provider": provider, "model": model, "read_only": read_only}
            self.registered[id] = spec
            self._agents[id] = FakeEndpoint(self, id, spec)
        return self._agents[id]

    @property
    def agents(self) -> list[FakeEndpoint]:
        return list(self._agents.values())

    def advance(self, ms: int) -> None:
        with self._lock:
            self._clock_ms += ms

    def clock(self) -> dict:
        with self._lock:
            c = self._clock_ms
        return {"clock_ms": c, "speed": 1.0, "paused": False, "world_epoch_ms": self.epoch_ms,
                "world_ms": self.epoch_ms + c}


def main() -> None:
    """Serves :func:`look_then_answer` on ``--port`` (default 0: a free one, printed) until killed."""
    import argparse
    import time

    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("--port", type=int, default=0)
    with FakeModel(look_then_answer, parser.parse_args().port) as fake:
        print(f"scripted model on {fake.url}/v1", flush=True)
        while True:
            time.sleep(3600)


if __name__ == "__main__":
    main()
