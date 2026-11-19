"""The metrics every service emits, named once here (`platform/metrics.txt` lists their Prometheus names):
request durations by route, method and status, query durations by operation, the deployed commit, and how
long the longest lock wait in the service's database has been waiting."""
import time
from contextlib import contextmanager
from typing import Callable

from opentelemetry.metrics import Meter, Observation

from .settings import Settings

SECONDS_BUCKETS = (0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0)


class Metrics:
    def __init__(self, meter: Meter, settings: Settings, lock_wait_s: Callable[[], float] | None = None):
        self.request_duration = meter.create_histogram(
            "http.server.request.duration", unit="s", description="Time to answer an HTTP request",
            explicit_bucket_boundaries_advisory=SECONDS_BUCKETS)
        self.query_duration = meter.create_histogram(
            "db.query.duration", unit="s", description="Time a database query took",
            explicit_bucket_boundaries_advisory=SECONDS_BUCKETS)
        meter.create_observable_gauge("deploy.info", description="The commit this process runs (always 1)",
                                      callbacks=[lambda _: [Observation(1, {"sha": settings.sha})]])
        if lock_wait_s is not None:
            meter.create_observable_gauge(
                "db.lock.wait", unit="s", description="Longest wait for a lock in this service's database",
                callbacks=[lambda _: [Observation(lock_wait_s())]])

    @contextmanager
    def query(self, operation: str):
        start = time.perf_counter()
        try:
            yield
        finally:
            self.query_duration.record(time.perf_counter() - start, {"db.operation": operation})


class RequestMetrics:
    """ASGI middleware recording each HTTP request's duration under its route template (`unmatched` when no
    route matched, so stray paths add no series)."""

    def __init__(self, app, metrics: Metrics):
        self.app, self.metrics = app, metrics

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            return await self.app(scope, receive, send)
        start, status = time.perf_counter(), [500]

        async def send_status(message):
            if message["type"] == "http.response.start":
                status[0] = message["status"]
            await send(message)

        try:
            await self.app(scope, receive, send_status)
        finally:
            route = getattr(scope.get("route"), "path", "unmatched")
            self.metrics.request_duration.record(time.perf_counter() - start, {
                "http.request.method": scope["method"], "http.route": route, "http.response.status_code": status[0]})
