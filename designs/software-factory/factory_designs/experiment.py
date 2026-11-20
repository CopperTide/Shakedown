"""The comparison experiment, data in ``experiment.yaml``: each design a Shakedown Code connection, and one
experiment of both with paired seeds, one world time window and one spend cap per run.

    python -m factory_designs.experiment create [--provider P] [--runs N] [--world-minutes M] [--task T]
                                                [--benchmark W] [--url URL]
    python -m factory_designs.experiment export <id> [--ledger deploy/runpod/ledger.jsonl] [--out comparison.json]
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any, Optional

import yaml
from shakedown.client import Shakedown

from .settings import Settings, load_settings

DESIGNS = Path(__file__).resolve().parent.parent
REPO = DESIGNS.parent.parent
EXPERIMENT = DESIGNS / "experiment.yaml"
LEDGER = REPO / "deploy" / "runpod" / "ledger.jsonl"


def load_experiment(path: Path = EXPERIMENT) -> dict:
    return yaml.safe_load(path.read_text())


def driver_command(module: str) -> list[str]:
    """How shaked starts a design's driver, from the repository root."""
    return ["uv", "run", "--project", str(DESIGNS.relative_to(REPO)), "python", "-m", module]


def ensure_connection(sd: Shakedown, name: str, module: str, settings: Settings) -> str:
    """The id of the Code connection named ``name``, made now when there is none."""
    for c in sd.get("/api/connections"):
        if c["name"] == name:
            return c["id"]
    env = {"FACTORY_PROVIDER": settings.provider, "FACTORY_MODEL": settings.model}
    made = sd.post("/api/connections", {"name": name, "command": driver_command(module),
                                        "workdir": str(REPO), "env": env})
    return made["id"]


def create(sd: Shakedown, path: Path = EXPERIMENT, provider: Optional[str] = None, runs: Optional[int] = None,
           world_minutes: Optional[float] = None, task: Optional[str] = None,
           benchmark: Optional[str] = None) -> str:
    """Posts the experiment in ``path`` (``runs``, ``world_minutes`` and ``task`` override its own) with one
    connection per design on ``provider`` (default: shared.yaml's); returns the experiment's id."""
    exp = load_experiment(path)
    settings = load_settings(env={**os.environ, **({"FACTORY_PROVIDER": provider} if provider else {})})
    on = f"{settings.model} on {settings.provider}"
    setups = [{"name": s["name"],
               "connection": ensure_connection(sd, f"{s['name']} · {on}", s["module"], settings)}
              for s in exp["setups"]]
    body: dict[str, Any] = {"name": f"{exp['name']} · {on}", "task": task or exp["task"],
                            "runs": runs or exp["runs"], "seed": exp["seed"],
                            "world_minutes": world_minutes or exp["world_minutes"],
                            "max_cost_usd": exp["max_cost_usd"], "setups": setups}
    if benchmark:
        body["benchmark"] = benchmark
    return sd.post("/api/experiments", body)["id"]


def _args(argv: Optional[list[str]]) -> argparse.Namespace:
    p = argparse.ArgumentParser(prog="python -m factory_designs.experiment", description=__doc__.split("\n")[0])
    p.add_argument("--url", default=os.environ.get("SHAKEDOWN_URL", "http://127.0.0.1:4510"))
    sub = p.add_subparsers(dest="command", required=True)
    c = sub.add_parser("create")
    c.add_argument("--provider")
    c.add_argument("--runs", type=int)
    c.add_argument("--world-minutes", type=float)
    c.add_argument("--task")
    c.add_argument("--benchmark")
    e = sub.add_parser("export")
    e.add_argument("id")
    e.add_argument("--ledger", type=Path, default=LEDGER)
    e.add_argument("--out", type=Path, default=Path("comparison.json"))
    return p.parse_args(argv)


def main(argv: Optional[list[str]] = None) -> None:
    from .comparison import export

    a = _args(argv)
    with Shakedown(a.url) as sd:
        if a.command == "create":
            print(create(sd, provider=a.provider, runs=a.runs, world_minutes=a.world_minutes, task=a.task,
                         benchmark=a.benchmark))
            return
        a.out.parent.mkdir(parents=True, exist_ok=True)
        a.out.write_text(json.dumps(export(sd, a.id, a.ledger), indent=2) + "\n")
        print(a.out)


if __name__ == "__main__":
    main()
