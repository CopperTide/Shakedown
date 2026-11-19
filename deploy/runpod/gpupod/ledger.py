"""The spend ledger, ``ledger.jsonl``: one JSON line per event of the pod's life and every other
cloud spend, so the total spent is known before each start. It never holds keys.

Entry kinds: ``quote``, ``start`` (``pod_id``, ``gpu_id``, ``usd_h``), ``ready``, ``measure``,
``stop`` (``by``: owner, idle, max_hours, budget, unhealthy), ``delete``, ``reconcile``
(``usd_actual``: billed for the pod so far), ``openrouter`` (``usd``), ``runs``.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Iterator, Optional

from .prices import utc_now

AT = "%Y-%m-%dT%H:%M:%SZ"
SECRET_WORDS = ("key", "token", "secret", "password")


def _names(value: Any) -> Iterator[str]:
    if isinstance(value, dict):
        for k, v in value.items():
            yield str(k)
            yield from _names(v)
    elif isinstance(value, list):
        for v in value:
            yield from _names(v)


def parse_at(text: str) -> datetime:
    return datetime.strptime(text, AT).replace(tzinfo=timezone.utc)


class Ledger:
    def __init__(self, path: Path, now: Callable[[], datetime] = utc_now):
        self.path = path
        self.now = now

    def append(self, entry: dict) -> dict:
        """Writes ``entry`` stamped with ``at`` (UTC). A field named like a key is refused."""
        secret = [n for n in _names(entry) if any(w in n.lower() for w in SECRET_WORDS)]
        if secret:
            raise ValueError(f"the ledger never holds keys: refused field {secret[0]}")
        line = {"at": self.now().strftime(AT), **entry}
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a") as f:
            f.write(json.dumps(line) + "\n")
        return line

    def entries(self) -> list[dict]:
        if not self.path.exists():
            return []
        return [json.loads(x) for x in self.path.read_text().splitlines() if x.strip()]

    def spent(self) -> float:
        """Dollars spent: per pod, its latest reconciled total plus its estimated time since
        (else its start→stop hours × rate, a running pod up to now); plus OpenRouter's."""
        entries = self.entries()
        pods = dict.fromkeys(e["pod_id"] for e in entries if e["kind"] == "start")
        other = sum(float(e["usd"]) for e in entries if e["kind"] == "openrouter")
        return other + sum(self._pod_spent([e for e in entries if e.get("pod_id") == p]) for p in pods)

    def _pod_spent(self, entries: list[dict]) -> float:
        reconciled = [e for e in entries if e["kind"] == "reconcile"]
        since = parse_at(reconciled[-1]["at"]) if reconciled else None
        total = float(reconciled[-1]["usd_actual"]) if reconciled else 0.0
        for start, end, rate in self._intervals(entries):
            begin = max(start, since) if since else start
            total += max((end - begin).total_seconds(), 0) / 3600 * rate
        return total

    def _intervals(self, entries: list[dict]) -> Iterator[tuple[datetime, datetime, float]]:
        """Each start → its stop (or delete, or now) of one pod's entries, with its rate."""
        start: Optional[dict] = None
        for e in entries:
            if e["kind"] == "start" and start is None:
                start = e
            elif e["kind"] in ("stop", "delete") and start is not None:
                yield parse_at(start["at"]), parse_at(e["at"]), float(start["usd_h"])
                start = None
        if start is not None:
            yield parse_at(start["at"]), self.now(), float(start["usd_h"])

    def open_session(self) -> Optional[dict]:
        """The last pod started and not deleted: ``{pod_id, gpu_id, usd_h, started_at,
        stopped}`` (``started_at`` its latest start), else None."""
        entries = self.entries()
        starts = [e for e in entries if e["kind"] == "start"]
        if not starts:
            return None
        pod = starts[-1]["pod_id"]
        mine = [e for e in entries if e.get("pod_id") == pod]
        if any(e["kind"] == "delete" for e in mine):
            return None
        last = [e for e in mine if e["kind"] == "start"][-1]
        stopped = any(e["kind"] == "stop" for e in mine[mine.index(last) + 1:])
        return {"pod_id": pod, "gpu_id": last.get("gpu_id"), "usd_h": float(last["usd_h"]),
                "started_at": parse_at(last["at"]), "stopped": stopped}
