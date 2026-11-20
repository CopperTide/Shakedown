"""A built world served as a benchmark: its world, tasks, instructions and teams from world.json (with
`${WORLD_DIR}` its own folder), scored by each task's checks."""
import json
from pathlib import Path

from shakedown_part import errors
from shakedown_part.benchmark import Benchmark
from shakedown_part.expect import evaluate

from .expand import WORLD_DIR

_TASK_FIELDS = ("id", "reason", "ticket", "setup", "clock_start")


def _with_folder(value, folder: str):
    if isinstance(value, dict):
        return {k: _with_folder(v, folder) for k, v in value.items()}
    if isinstance(value, list):
        return [_with_folder(v, folder) for v in value]
    return value.replace(WORLD_DIR, folder) if isinstance(value, str) else value


class WorldBenchmark(Benchmark):
    """The benchmark of one built world (`world.json`, format `shakedown-world/1`)."""

    def __init__(self, world_file: Path, version: str = "0.0.0"):
        folder = str(Path(world_file).resolve().parent)
        self.world = _with_folder(json.loads(Path(world_file).read_text(encoding="utf-8")), folder)
        self.version = version

    def describe(self):
        w = self.world
        out = {"name": w["name"], "version": self.version, "protocol": 1, "world": w["parts"],
               "tasks": [{k: t[k] for k in _TASK_FIELDS if k in t} for t in w["tasks"]],
               "instructions": w["instructions"], "teams": w["teams"][1:], "tools": [], "links": w["links"],
               "gates": w["gates"], "scored": True}
        if w["teams"]:
            out["team"] = w["teams"][0]
        for k in ("instructions_title", "scenario_options"):
            if k in w:
                out[k] = w[k]
        return out

    def score(self, task, docs):
        found = next((t for t in self.world["tasks"] if t["id"] == task), None)
        if found is None:
            raise errors.invalid(f"no task {task} in this world")
        return {"checks": evaluate(found["checks"], docs)}
