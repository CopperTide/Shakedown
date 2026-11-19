"""The traffic itself: each tick reads the world clock, plans the journeys that start in (last tick, now] and
hands them to a bounded pool of workers (a journey that finds the backlog full is dropped and counted); every
`contracts.every_s` of world time the contract probe runs on its own worker. A request is recorded at the world
time it was sent, in the stats and as OTLP."""
import random
import threading
import time
from concurrent.futures import ThreadPoolExecutor

from . import contracts, journeys
from .client import request
from .otlp import traceparent
from .schedule import plan
from .stats import Stats

MINUTE_MS = 60_000


class Traffic:
    def __init__(self, config, clock, telemetry):
        self.config, self.clock, self.telemetry = config, clock, telemetry
        self.stats = Stats(config.retention_minutes * MINUTE_MS, config.slo["window_minutes"] * MINUTE_MS,
                           config.slo["objective"])
        self.contracts = contracts.load(config.contracts_dir)
        self._contracts_doc = {f"{c['consumer']}-{c['provider']}": {"passed": None, "failures": [],
                                                                     "at_world_ms": None} for c in self.contracts}
        self._pool = ThreadPoolExecutor(config.workers, thread_name_prefix="journey")
        self._probe = ThreadPoolExecutor(1, thread_name_prefix="contracts")
        self._last_ms = self._next_contracts_ms = None
        self._pending, self._queued, self._idle = 0, 0, threading.Condition()
        self.outcomes = {"completed": 0, "failed": 0, "abandoned": 0, "dropped": 0}

    def tick(self) -> None:
        now = self.clock.now()
        if self._last_ms is None:
            self._last_ms = self._next_contracts_ms = now.clock_ms
        for at_ms, name in plan(self.config.seed, self._last_ms, now.clock_ms, self.config.profile):
            self._submit(self._pool, self._journey, at_ms, name)
        self._last_ms = max(self._last_ms, now.clock_ms)
        if now.clock_ms >= self._next_contracts_ms:
            self._next_contracts_ms = now.clock_ms + self.config.contracts["every_s"] * 1000
            self._submit(self._probe, self._run_contracts, now.world_ms)

    def wait(self) -> None:
        with self._idle:
            self._idle.wait_for(lambda: self._pending == 0)

    def close(self) -> None:
        """Stop at once: journeys not yet started are dropped."""
        for pool in (self._pool, self._probe):
            pool.shutdown(wait=False, cancel_futures=True)

    def stats_doc(self) -> dict:
        return {**self.stats.totals(), "journeys": dict(self.outcomes), "telemetry": dict(self.telemetry.counts),
                "clock": self.clock.source(), "world_ms": self.clock.now().world_ms}

    def contracts_doc(self) -> dict:
        return {k: dict(v) for k, v in self._contracts_doc.items()}

    def _submit(self, pool, fn, *args) -> None:
        journey = pool is self._pool
        with self._idle:
            if journey and self._queued >= self.config.max_pending:
                self.outcomes["dropped"] += 1
                return
            self._pending, self._queued = self._pending + 1, self._queued + journey
        pool.submit(self._done_after, journey, fn, *args)

    def _done_after(self, journey: bool, fn, *args) -> None:
        try:
            fn(*args)
        finally:
            with self._idle:
                self._pending, self._queued = self._pending - 1, self._queued - journey
                self._idle.notify_all()

    def _run_contracts(self, at_world_ms: int) -> None:
        self._contracts_doc.update(contracts.run_all(self.contracts, self.config.services, at_world_ms))

    def _journey(self, at_ms: int, name: str) -> None:
        t, start_ns = self.telemetry, time.time_ns()
        trace, root = t.trace_id(), t.span_id()
        rng = random.Random(f"{self.config.seed}:{at_ms}:{name}")
        send = lambda step, method, url, body: self._send(name, trace, root, step, method, url, body)  # noqa: E731
        result = journeys.run(self.config.journeys[name], self.config.services, rng, send)
        with self._idle:
            self.outcomes[result] += 1
        t.span(f"journey {name}", trace, root, None, start_ns, time.time_ns(), {"journey": name, "result": result},
               error=result == "failed", internal=True)

    def _send(self, journey, trace, root, step, method, url, body):
        t, span = self.telemetry, self.telemetry.span_id()
        world_ms, start_ns = self.clock.now().world_ms, time.time_ns()
        out = request(method, url, body, {"User-Agent": self.config.user_agent,
                                          "traceparent": traceparent(trace, span)}, self.config.timeout_s)
        self.stats.record(world_ms, journey, out.outcome, out.latency_ms)
        attrs = request_attributes(journey, step, method, out)
        t.duration(attrs, out.latency_ms / 1000)
        failed = out.status is None or out.status >= 500
        t.span(f"{method} {step['path']}", trace, span, root, start_ns, time.time_ns(), attrs, error=failed)
        if failed:
            t.log("WARN", f"{method} {step['path']} on {step['service']} failed: {out.outcome}", attrs,
                  time.time_ns(), trace, span)
        return out


def request_attributes(journey: str, step: dict, method: str, out) -> dict:
    attrs = {"journey": journey, "http.request.method": method, "server.address": step["service"],
             "url.template": step["path"]}
    if out.status is not None:
        attrs["http.response.status_code"] = out.status
    if out.status is None or out.status >= 500:
        attrs["error.type"] = out.error or str(out.status)
    return attrs
