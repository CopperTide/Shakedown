"""What the traffic saw: each request's world time, journey, outcome (an HTTP status, `timeout` or `connection`)
and latency. Server errors (5xx), timeouts and failed connections are errors, as a customer sees them; client
errors (4xx, such as an item sold out) are not. Totals count everything since start; windows (and the SLO) read
the requests kept, which are the last `retention_ms` of world time."""
import math
import threading
from collections import deque

ERROR_KINDS = {"timeout": "timeouts", "connection": "connection_errors"}


def error_kind(outcome) -> str | None:
    if isinstance(outcome, int):
        return "errors_5xx" if outcome >= 500 else None
    return ERROR_KINDS[outcome]


def percentile(values: list[int], q: float) -> int | None:
    """Nearest rank."""
    if not values:
        return None
    ordered = sorted(values)
    return ordered[max(math.ceil(q * len(ordered)), 1) - 1]


class Stats:
    def __init__(self, retention_ms: int, slo_window_ms: int, objective: float):
        self.retention_ms, self.slo_window_ms, self.objective = retention_ms, slo_window_ms, objective
        self._kept: deque = deque()
        self._totals = {"requests": 0, "errors_5xx": 0, "timeouts": 0, "connection_errors": 0, "errors": 0}
        self._journeys: dict[str, dict] = {}
        self._newest = 0
        self._lock = threading.Lock()

    def record(self, world_ms: int, journey: str, outcome, latency_ms: int) -> None:
        kind = error_kind(outcome)
        with self._lock:
            self._kept.append((world_ms, kind is not None, latency_ms))
            journey_totals = self._journeys.setdefault(journey, {"requests": 0, "errors": 0})
            for counts in (self._totals, journey_totals):
                counts["requests"] += 1
                counts["errors"] += kind is not None
            if kind:
                self._totals[kind] += 1
            self._newest = max(self._newest, world_ms)
            while self._kept and self._kept[0][0] <= self._newest - self.retention_ms:
                self._kept.popleft()

    def totals(self) -> dict:
        with self._lock:
            return {**self._totals, "by_journey": {j: dict(c) for j, c in sorted(self._journeys.items())}}

    def window(self, to_ms: int, length_ms: int) -> dict:
        from_ms = to_ms - length_ms
        with self._lock:
            inside = [(failed, latency) for at, failed, latency in self._kept if from_ms < at <= to_ms]
        latencies, errors = [lat for _, lat in inside], sum(failed for failed, _ in inside)
        return {"from_world_ms": from_ms, "to_world_ms": to_ms, "requests": len(inside), "errors": errors,
                "error_rate": errors / len(inside) if inside else None,
                "p50_ms": percentile(latencies, 0.5), "p95_ms": percentile(latencies, 0.95)}

    def slo(self, at_ms: int) -> dict:
        w = self.window(at_ms, self.slo_window_ms)
        rate = w["error_rate"]
        return {"error_rate_30m": rate, "p95_ms": w["p95_ms"], "requests_30m": w["requests"],
                "errors_30m": w["errors"], "objective": self.objective,
                "meets_objective": None if rate is None else rate <= self.objective, "at_world_ms": at_ms}
