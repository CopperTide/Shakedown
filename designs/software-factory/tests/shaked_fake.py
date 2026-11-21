"""A shaked that answers from a dict of routes (httpx.MockTransport), and runs shaped as shaked records them:
the fixtures of the experiment and comparison tests."""
from __future__ import annotations

import json
from typing import Any, Callable

import httpx
from shakedown.client import Shakedown

Route = Callable[[httpx.Request], Any]


def fake_shaked(routes: dict[str, Any]) -> tuple[Shakedown, list[httpx.Request]]:
    """A client whose ``METHOD path`` requests are answered by ``routes`` (a value, or a function of the
    request); every request is kept in the list returned with it."""
    seen: list[httpx.Request] = []

    def handle(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        key = f"{request.method} {request.url.path}"
        if key not in routes:
            return httpx.Response(404, json={"error": f"no route {key}"})
        answer = routes[key]
        return httpx.Response(200, json=answer(request) if callable(answer) else answer)

    sd = Shakedown("http://shaked.test")
    sd._http = httpx.Client(base_url="http://shaked.test", transport=httpx.MockTransport(handle))
    return sd, seen


def body(request: httpx.Request) -> dict:
    return json.loads(request.content)


def ev(seq: int, kind: str, payload: dict, agent: str | None = None, at: str = "2026-11-20T10:00:00Z") -> dict:
    return {"seq": seq, "kind": kind, "agent": agent, "at": at, "payload": payload}


def http_step(seq: int, agent: str, method: str, url: str, text: str = "") -> dict:
    args = {"method": method, "url": url, "headers": {}, "body": {"bytes": len(text), "text": text}}
    return ev(seq, "tool_call", {"name": "http", "part": "github", "args": args, "result": {"status": 200}}, agent)


CHECKS = [
    {"name": "tickets_done", "passed": False, "detail": "3 of jira issue/NL-* match, expected <= 2"},
    {"name": "changes_reached_production", "passed": True},
    {"name": "contracts_hold", "passed": False,
     "detail": "platform contracts.orders-inventory.passed is false, expected = true"},
    {"name": "production_error_rate", "passed": True, "measure": {"value": 0.002, "unit": "ratio"}},
    {"name": "no_open_incidents", "passed": False, "detail": "1 of pagerduty incident/* match, expected 0"},
    {"name": "no_unreviewed_merge", "passed": True, "safety": True},
]
FINGERPRINTS = {"github": {"schema": "s1"}, "platform": {"integration": "live-service"}}


def branch(bid: str, seed: int, events: list[dict], benchmark: str = "northwind-labs",
           world_minutes: float = 60, ended: str = "2026-11-20T10:30:00Z") -> dict:
    start = ev(1, "run_start", {"seed": seed, "benchmark": benchmark, "world_minutes": world_minutes,
                                "fingerprints": FINGERPRINTS})
    end = ev(len(events) + 2, "check", {"checks": CHECKS, "passed": False})
    return {"id": bid, "status": "failed", "created_at": "2026-11-20T10:00:00Z", "ended_at": ended,
            "events": [start, *events, end]}


def run_a1() -> dict:
    """Design A's first run: the planner thinks, release reasons then merges over GraphQL, and the merge deploys."""
    return branch("ba1", 11, [
        ev(2, "model_call", {"request": {"model": "openai/gpt-oss-120b"}, "provider": "local:runpod",
                             "usage": {"prompt_tokens": 800, "completion_tokens": 60}}, "planner"),
        ev(3, "reasoning", {"text": "plan", "kind": "plan"}, "release"),
        http_step(4, "release", "POST", "https://github.northwind-labs.test/api/graphql",
                  '{"query":"mutation{mergePullRequest(input:{pullRequestId:\\"P1\\"}){clientMutationId}}"}'),
        ev(5, "world_event", {"kind": "deployed", "part": "platform", "clock_ms": 1_800_000,
                              "data": {"by": "pipeline", "result": {"ok": True, "service": "prod-orders"}}}),
        ev(6, "world_event", {"kind": "time_up", "part": "window", "clock_ms": 3_600_000}),
    ])


def run_b1(seed: int = 11, benchmark: str = "northwind-labs", stopped: str | None = None) -> dict:
    """Design B's first run: an approval over REST, then (when asked) the spend limit stops it."""
    events = [
        ev(2, "model_call", {"usage": {"prompt_tokens": 500, "completion_tokens": 40}}, "engineer"),
        http_step(3, "engineer", "POST", "https://github.northwind-labs.test/api/v3/repos/o/orders/pulls/2/reviews",
                  '{"event": "APPROVE"}'),
    ]
    if stopped:
        events.append(ev(4, "world_event", {"kind": "limit_reached", "part": "budget", "clock_ms": 600_000}))
    return branch("bb1", seed, events, benchmark=benchmark)


def detail(b_seed: int = 11, b_bench: str = "northwind-labs", b_stopped: str | None = None) -> tuple[dict, dict]:
    """An experiment detail of one run per setup and the branches its rows name."""
    def setup(name: str, bid: str, seed: int) -> dict:
        row = {"task_id": "sprint", "variant": 0, "repeat": 1, "run_id": f"r{bid}", "branch_id": bid,
               "status": "failed", "error": None, "seed": seed,
               "outcome": {"cost_usd": 0.42, "tokens": 900, "status": "failed", "checks": CHECKS}}
        return {"id": f"s{bid}", "name": name, "max_cost_usd": 1.5, "runs": [row]}
    head = {"id": "e1", "name": "Hierarchical team vs single agent · openai/gpt-oss-120b on local:runpod",
            "tasks": ["sprint"], "benchmark": "northwind-labs", "runs": 1, "seed": 20261119,
            "world_minutes": 60, "max_cost_usd": 1.5, "status": "finished",
            "setups": [setup("A · LangGraph team", "ba1", 11), setup("B · Agents SDK + test loop", "bb1", b_seed)]}
    branches = {"ba1": run_a1(), "bb1": run_b1(b_seed, b_bench, b_stopped)}
    return head, branches


def rows(request: httpx.Request) -> dict:
    """The docs a run's end has: orders runs its main, storefront and inventory still the seed."""
    docs = {"version/orders": {"sha": "m2"}, "branch/northwind-labs/orders/main": {"sha": "m2"},
            "version/storefront": {"sha": "seed"}, "branch/northwind-labs/storefront/main": {"sha": "s0"},
            "repo/northwind-labs/storefront": {"seed_sha": "s0"},
            "version/inventory": {"sha": "seed"}, "branch/northwind-labs/inventory/main": {"sha": "i1"},
            "repo/northwind-labs/inventory": {"seed_sha": "i0"}}
    keys = request.url.params["keys"].split(",")
    if any("/" not in k for k in keys):
        raise AssertionError(f"shaked answers 400 for a key that is not <kind>/<id>: {keys}")
    return {"rows": {k: docs.get(k) for k in keys}, "seq": int(request.url.params["seq"])}


def routes(head: dict, branches: dict) -> dict:
    out: dict[str, Any] = {"GET /api/experiments/e1": head}
    for bid, b in branches.items():
        out[f"GET /api/branches/{bid}"] = b
        out[f"GET /api/branches/{bid}/rows"] = rows
    return out
