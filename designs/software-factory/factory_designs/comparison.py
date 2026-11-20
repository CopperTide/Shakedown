"""``comparison.json``, the data stage 6's comparison page is built on: per design and run what reached
production, what broke, cost, time and the steps that mattered, with each design's summary and a check that
the comparison was fair (one world, paired seeds, the same caps)."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
from statistics import median
from typing import Any, Optional

from shakedown.client import Shakedown

from .experiment import load_experiment
from .gpu import Session, session_at, sessions
from .key_steps import load_rules
from .run_record import Production, run_record

MEDIANS = ("cost_usd_hosted_equiv", "wall_minutes", "gpu_usd")


def summary(runs: list[dict]) -> dict[str, Any]:
    names = sorted({n for r in runs for n in r["checks"]})
    rate = {n: round(sum(1 for r in runs if r["checks"].get(n, {}).get("passed")) / len(runs), 4) for n in names}

    def mid(values: list) -> Optional[float]:
        values = [v for v in values if v is not None]
        return median(values) if values else None
    med = {k: mid([r[k] for r in runs]) for k in MEDIANS}
    med["error_rate_30m"] = mid([r["production"]["error_rate_30m"] for r in runs])
    return {"pass_rate": rate, "median": med}


def fingerprint(fingerprints: Any) -> str:
    return hashlib.sha256(json.dumps(fingerprints, sort_keys=True).encode()).hexdigest()[:16]


def fair(head: dict, setups: list[dict]) -> dict[str, Any]:
    runs = [r for s in setups for r in s["runs"]]
    worlds = {(r["_world"]["benchmark"], fingerprint(r["_world"]["fingerprints"])) for r in runs}
    problems = [f"runs are on {len(worlds)} worlds: {sorted(w[0] for w in worlds)}"] if len(worlds) > 1 else []
    by_repeat: dict[int, set] = {}
    for r in runs:
        by_repeat.setdefault(r["repeat"], set()).add(r["seed"])
    unpaired = sorted(k for k, seeds in by_repeat.items() if len(seeds) > 1)
    if unpaired:
        problems.append(f"repeats {unpaired} started from different seeds")
    caps = {(r["_world"]["world_minutes"], s.get("max_cost_usd", head.get("max_cost_usd"))) for s in setups
            for r in s["runs"]}
    if len(caps) > 1:
        problems.append(f"runs had different windows or spend caps: {sorted(caps, key=str)}")
    return {"same_world": len(worlds) <= 1, "paired_seeds": not unpaired, "same_caps": len(caps) <= 1,
            "problems": problems}


def _head(head: dict, setups: list[dict], gpu: Optional[Session]) -> dict[str, Any]:
    first = next((r for s in setups for r in s["runs"]), None)
    world = first["_world"] if first else {}
    model = next((r["_world"]["model"] for s in setups for r in s["runs"] if r["_world"]["model"]), {})
    return {"id": head["id"], "name": head["name"], "task": (head.get("tasks") or [None])[0],
            "world": head.get("benchmark"), "world_fingerprint": fingerprint(world.get("fingerprints")),
            "seed": head.get("seed"), "world_minutes": head.get("world_minutes"),
            "max_cost_usd": head.get("max_cost_usd"), "model": model.get("model"),
            "provider": model.get("provider"),
            "gpu": {"id": gpu.gpu_id, "usd_h": gpu.usd_h} if gpu else None}


def export(sd: Shakedown, experiment_id: str, ledger: Optional[Path] = None,
           experiment: Optional[dict] = None) -> dict[str, Any]:
    """The experiment ``experiment_id`` as comparison.json, GPU dollars from the ``ledger`` file."""
    head = sd.get(f"/api/experiments/{experiment_id}")
    spec, rules, pods = experiment or load_experiment(), load_rules(), sessions(ledger)
    production = Production(sd, spec["production"])
    setups = [{"name": s["name"], "max_cost_usd": s.get("max_cost_usd"),
               "runs": [run_record(sd, row, production, rules, pods) for row in s["runs"] if row.get("branch_id")]}
              for s in head["setups"]]
    began = [r["_world"]["began"] for s in setups for r in s["runs"]]
    out = {"experiment": _head(head, setups, session_at(pods, min(began)) if began else None),
           "setups": [], "fair": fair(head, setups),
           "gpu_total_usd": round(sum(r["gpu_usd"] for s in setups for r in s["runs"]), 4)}
    for s in setups:
        runs = [{k: v for k, v in r.items() if k != "_world"} for r in s["runs"]]
        out["setups"].append({"name": s["name"], "runs": runs, "summary": summary(runs) if runs else {}})
    return out
