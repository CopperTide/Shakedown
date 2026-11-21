"""GPU spend of a run, from the RunPod ledger (``deploy/runpod/ledger.jsonl``, written by ``gpupod``): the pod
session open when the run started, and its hourly rate × the run's wall time."""
from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

AT = "%Y-%m-%dT%H:%M:%SZ"


@dataclass(frozen=True)
class Session:
    """One pod start until its stop or delete (None: still running)."""
    gpu_id: str
    usd_h: float
    start: datetime
    end: Optional[datetime]


def _at(text: str) -> datetime:
    return datetime.strptime(text, AT).replace(tzinfo=timezone.utc)


def sessions(path: Optional[Path]) -> list[Session]:
    """Every pod session in the ledger at ``path`` (none when it is missing)."""
    if path is None or not path.exists():
        return []
    entries = [json.loads(x) for x in path.read_text().splitlines() if x.strip()]
    out, open_start = [], {}
    for e in entries:
        pod = e.get("pod_id")
        if e["kind"] == "start":
            open_start[pod] = e
        elif e["kind"] in ("stop", "delete") and pod in open_start:
            s = open_start.pop(pod)
            out.append(Session(s["gpu_id"], float(s["usd_h"]), _at(s["at"]), _at(e["at"])))
    out += [Session(s["gpu_id"], float(s["usd_h"]), _at(s["at"]), None) for s in open_start.values()]
    return sorted(out, key=lambda s: s.start)


def session_at(all_sessions: list[Session], at: datetime) -> Optional[Session]:
    return next((s for s in all_sessions if s.start <= at and (s.end is None or at < s.end)), None)


def gpu_usd(all_sessions: list[Session], began: datetime, wall_minutes: float) -> dict:
    """``{minutes, usd, session}`` of a run that began at ``began`` and took ``wall_minutes``."""
    s = session_at(all_sessions, began)
    if s is None:
        return {"minutes": 0, "usd": 0, "session": None}
    return {"minutes": wall_minutes, "usd": round(wall_minutes / 60 * s.usd_h, 4), "session": s}
