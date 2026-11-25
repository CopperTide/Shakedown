"""One HTTP request with a time limit, and what came of it: the status and JSON body of an answer (any status),
or `timeout` / `connection` when none came."""
import json
import socket
import time
import urllib.error
import urllib.request
from dataclasses import dataclass


@dataclass(frozen=True)
class Outcome:
    status: int | None
    error: str | None
    latency_ms: int
    body: object = None
    json: bool = False

    @property
    def outcome(self):
        """What the stats count: the status, else the error."""
        return self.status if self.status is not None else self.error


def parse(raw: bytes) -> tuple[object, bool]:
    try:
        return json.loads(raw), True
    except ValueError:
        return None, False


def is_timeout(exc: BaseException) -> bool:
    reason = getattr(exc, "reason", exc)
    return isinstance(reason, (TimeoutError, socket.timeout)) or "timed out" in str(reason)


def send(req: urllib.request.Request, timeout_s: float) -> tuple[int, bytes]:
    try:
        with urllib.request.urlopen(req, timeout=timeout_s) as answer:
            return answer.status, answer.read()
    except urllib.error.HTTPError as answer:
        return answer.code, answer.read()


def request(method: str, url: str, json_body=None, headers=None, timeout_s: float = 2.0) -> Outcome:
    data = None if json_body is None else json.dumps(json_body).encode()
    req = urllib.request.Request(url, data=data, method=method, headers={
        **({"Content-Type": "application/json"} if data is not None else {}), **(headers or {})})
    start = time.monotonic()
    try:
        status, raw = send(req, timeout_s)
    except (OSError, ValueError) as failed:
        return Outcome(None, "timeout" if is_timeout(failed) else "connection", ms_since(start))
    body, is_json = parse(raw)
    return Outcome(status, None, ms_since(start), body, is_json)


def ms_since(start: float) -> int:
    return int((time.monotonic() - start) * 1000)
