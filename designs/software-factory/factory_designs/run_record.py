"""One run of the experiment as comparison.json shows it, read from its branch: its checks as scored, what
reached production, tokens, cost, time, GPU spend, the key steps and why it stopped."""
from __future__ import annotations

import re
from datetime import datetime
from typing import Any, Optional

from shakedown.client import Shakedown

from .gpu import Session, gpu_usd
from .key_steps import key_steps

#: World events that end a run before its agents finish: their part and kind, and the name shown.
STOPS = {("budget", "limit_reached"): "spend limit", ("window", "time_up"): "window"}


def parse_at(text: str) -> datetime:
    return datetime.fromisoformat(text.replace("Z", "+00:00"))


def checks_of(events: list[dict]) -> dict[str, dict]:
    """``{name: {passed, measure, safety}}`` of the run's last scoring."""
    scored = [e for e in events if e["kind"] == "check" and isinstance(e["payload"].get("checks"), list)]
    if not scored:
        return {}
    return {c["name"]: {"passed": c["passed"], "measure": (c.get("measure") or {}).get("value"),
                        "safety": bool(c.get("safety"))} for c in scored[-1]["payload"]["checks"]}


def count_in(detail: Optional[str]) -> Optional[int]:
    """The count a failed find check names (``"1 of pagerduty incident/* match, …"``)."""
    m = re.match(r"(\d+) of ", detail or "")
    return int(m.group(1)) if m else None


def tokens(events: list[dict]) -> dict[str, int]:
    usage = [e["payload"].get("usage") or {} for e in events if e["kind"] == "model_call"]
    return {"prompt": sum(int(u.get("prompt_tokens") or 0) for u in usage),
            "completion": sum(int(u.get("completion_tokens") or 0) for u in usage)}


def stopped(events: list[dict]) -> Optional[str]:
    for e in events:
        p = e["payload"]
        name = STOPS.get((p.get("part"), p.get("kind"))) if e["kind"] == "world_event" else None
        if name:
            return name
    return None


def world_minutes(events: list[dict]) -> float:
    clocks = [e["payload"]["clock_ms"] for e in events if isinstance(e["payload"].get("clock_ms"), (int, float))]
    return round(max(clocks, default=0) / 60_000, 2)


class Production:
    """What a run's end has of production, read from the docs the sprint's checks read (``production`` in
    experiment.yaml names the services and the checks)."""

    def __init__(self, sd: Shakedown, spec: dict):
        self.sd, self.spec = sd, spec

    def _docs(self, branch: str, seq: int, part: str, keys: list[str]) -> dict:
        answer = self.sd.get(f"/api/branches/{branch}/rows", seq=seq, keys=",".join(keys), part=part) or {}
        return answer.get("rows") or {}

    def reached(self, branch: str, seq: int, org: str) -> dict[str, bool]:
        """Each service runs its main's commit, or the seed image while main is the seeded commit."""
        out = {}
        for svc in self.spec["services"]:
            version = (self._docs(branch, seq, "platform", [f"version/{svc}"]).get(f"version/{svc}") or {})
            code = self._docs(branch, seq, "github", [f"branch/{org}/{svc}/main", f"repo/{org}/{svc}"])
            main = (code.get(f"branch/{org}/{svc}/main") or {}).get("sha")
            seed = (code.get(f"repo/{org}/{svc}") or {}).get("seed_sha")
            sha = version.get("sha")
            out[svc] = bool(sha) and (sha == main or (sha == "seed" and main is not None and main == seed))
        return out

    def of(self, branch: str, events: list[dict], checks: dict, org: str) -> dict[str, Any]:
        names, seq = self.spec["checks"], events[-1]["seq"]
        incidents = checks.get(names["incidents"], {})
        open_incidents = 0 if incidents.get("passed") else count_in(_detail(events, names["incidents"]))
        return {"reached": self.reached(branch, seq, org),
                "error_rate_30m": checks.get(names["error_rate"], {}).get("measure"),
                "open_incidents": open_incidents,
                "contracts_held": checks.get(names["contracts"], {}).get("passed")}


def _detail(events: list[dict], name: str) -> Optional[str]:
    scored = [e for e in events if e["kind"] == "check" and isinstance(e["payload"].get("checks"), list)]
    found = [c for c in (scored[-1]["payload"]["checks"] if scored else []) if c["name"] == name]
    return found[0].get("detail") if found else None


def run_record(sd: Shakedown, row: dict, production: Production, rules: list[dict],
               sessions: list[Session]) -> dict[str, Any]:
    """The run of ``row`` (a suite report's run row) as comparison.json has it."""
    branch = sd.get(f"/api/branches/{row['branch_id']}")
    events = branch["events"]
    start = events[0]["payload"] if events and events[0]["kind"] == "run_start" else {}
    began, ended = parse_at(branch["created_at"]), parse_at(branch.get("ended_at") or branch["created_at"])
    wall = round((ended - began).total_seconds() / 60, 2)
    gpu = gpu_usd(sessions, began, wall)
    checks = checks_of(events)
    return {"run_id": row["run_id"], "branch_id": row["branch_id"], "repeat": row["repeat"],
            "seed": row.get("seed") if row.get("seed") is not None else start.get("seed"),
            "status": row["status"], "error": row.get("error"), "checks": checks,
            "production": production.of(row["branch_id"], events, checks, start.get("benchmark", "")),
            "tokens": tokens(events), "cost_usd_hosted_equiv": (row.get("outcome") or {}).get("cost_usd"),
            "wall_minutes": wall, "world_minutes": world_minutes(events),
            "gpu_minutes": gpu["minutes"], "gpu_usd": gpu["usd"],
            "key_steps": key_steps(events, rules), "stopped": stopped(events),
            "_world": {"benchmark": start.get("benchmark"), "fingerprints": start.get("fingerprints"),
                       "world_minutes": start.get("world_minutes"), "model": _model(events), "began": began}}


def _model(events: list[dict]) -> dict:
    """The model and provider of the run's first model call (shaked records the model in its request)."""
    calls = [e["payload"] for e in events if e["kind"] == "model_call"]
    for p in calls:
        model = p.get("model") or (p.get("request") or {}).get("model")
        if model:
            return {"model": model, "provider": p.get("provider")}
    return {}
