"""The idle watchdog, started detached with the pod: every minute it probes vLLM and shaked and
stops the pod when it has been idle for ``idle_minutes``, ran ``max_session_hours``, would pass
the budget within ten more minutes, or was unhealthy for ten minutes in a row.

Busy means vLLM is running a request, has finished one since the last probe, or a run in
shaked is running (an experiment between model calls, or one holding its world time window).
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from datetime import datetime, timedelta
from typing import Optional

import httpx

from .pod import Ops, shaked_running
from .runpodctl import RunpodError, call
from .spec import PodSpec

PROBE_S = 60.0
UNHEALTHY = timedelta(minutes=10)
#: The spend the next stretch could add: the budget stop keeps this much in hand.
AHEAD_H = 10 / 60


@dataclass(frozen=True)
class Probe:
    running_requests: Optional[int]
    success_total: Optional[float]
    shaked_running: Optional[bool]
    healthy: Optional[bool]


@dataclass(frozen=True)
class WatchState:
    last_busy: Optional[datetime] = None
    last_success: Optional[float] = None
    unhealthy_since: Optional[datetime] = None


@dataclass(frozen=True)
class Decision:
    stop: bool
    by: Optional[str] = None


def _busy(state: WatchState, p: Probe) -> bool:
    finished = (p.success_total is not None and state.last_success is not None
                and p.success_total != state.last_success)
    return bool(p.running_requests) or finished or bool(p.shaked_running)


def observe(state: WatchState, p: Probe, now: datetime) -> WatchState:
    """The state after probe ``p`` at ``now`` (the first probe counts as busy)."""
    busy = state.last_busy is None or _busy(state, p)
    return replace(
        state,
        last_busy=now if busy else state.last_busy,
        last_success=p.success_total if p.success_total is not None else state.last_success,
        unhealthy_since=(state.unhealthy_since or now) if p.healthy is False else None,
    )


def decide(now: datetime, state: WatchState, started: datetime, rate: float, spent: float,
           cfg: PodSpec) -> Decision:
    """Whether to stop the pod now; ``spent`` is the ledger's total, this session included."""
    if now - started >= timedelta(hours=cfg.max_session_hours):
        return Decision(True, "max_hours")
    if spent + AHEAD_H * rate > cfg.budget_usd:
        return Decision(True, "budget")
    if state.unhealthy_since and now - state.unhealthy_since >= UNHEALTHY:
        return Decision(True, "unhealthy")
    if state.last_busy and now - state.last_busy >= timedelta(minutes=cfg.idle_minutes):
        return Decision(True, "idle")
    return Decision(False)


def parse_metrics(text: str) -> tuple[Optional[int], Optional[float]]:
    """``vllm:num_requests_running`` and ``vllm:request_success_total``, each summed over its
    labels (None when absent), from Prometheus text."""
    sums: dict[str, float] = {}
    for line in text.splitlines():
        if not line or line.startswith("#"):
            continue
        name = line.split("{", 1)[0].split(" ", 1)[0]
        try:
            sums[name] = sums.get(name, 0.0) + float(line.rsplit(" ", 1)[1])
        except (IndexError, ValueError):
            continue
    running = sums.get("vllm:num_requests_running")
    return (int(running) if running is not None else None, sums.get("vllm:request_success_total"))


def probe(http: httpx.Client, base: str, shaked_url: str) -> Probe:
    """vLLM's public ``/health`` and ``/metrics`` (no key needed) and shaked's runs."""
    try:
        healthy = http.get(f"{base}/health", timeout=30).status_code == 200
    except httpx.HTTPError:
        healthy = False
    try:
        r = http.get(f"{base}/metrics", timeout=30)
        running, success = parse_metrics(r.text) if r.status_code == 200 else (None, None)
    except httpx.HTTPError:
        running, success = None, None
    return Probe(running, success, shaked_running(http, shaked_url), healthy)


def _stop(ops: Ops, pod_id: str, by: str) -> bool:
    try:
        call(ops.runner, "pod", "stop", pod_id)
    except RunpodError as e:
        ops.out(f"watchdog: stopping {pod_id} failed ({e}); trying again in a minute")
        return False
    ops.ledger.append({"kind": "stop", "pod_id": pod_id, "by": by})
    ops.out(f"watchdog: stopped {pod_id} ({by}); spent ${ops.ledger.spent():.2f}")
    return True


def watch(ops: Ops, pod_id: str) -> Optional[str]:
    """Probes every minute until it stops the pod (returns why), or the pod was stopped or
    deleted another way (returns None)."""
    state = WatchState()
    while True:
        s = ops.ledger.open_session()
        if not s or s["pod_id"] != pod_id or s["stopped"]:
            return None
        now = ops.now()
        state = observe(state, probe(ops.http, ops.base(pod_id), ops.shaked_url), now)
        d = decide(now, state, s["started_at"], s["usd_h"], ops.ledger.spent(), ops.spec)
        if d.stop and _stop(ops, pod_id, d.by or "idle"):
            return d.by
        ops.sleep(PROBE_S)
