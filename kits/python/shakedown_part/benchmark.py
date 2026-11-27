"""Benchmarks in Python: the calls of docs/protocol.md, "Benchmark calls", over the same line protocol.

Subclass `Benchmark`, override what you need (`describe` always; `score` with `scored: true`; `view` with
`view`), and end the program with `serve_benchmark(MyBenchmark())`.
"""
import json
from pathlib import Path

from . import errors, worlddata
from .models import Models
from .serve import _need, announced, serve


class Benchmark:
    """A benchmark: names its world and tasks, seeds fresh instances, and scores or draws the world from
    its parts' docs. Methods take and return plain JSON values."""

    def describe(self):
        raise NotImplementedError("describe")

    def seed(self, connections):
        return None  # a world of simulated parts starts from their configs

    def call_tool(self, name, args, connections):
        return {"error": f"this benchmark has no tool {name}", "part": "", "reads": [], "writes": []}

    def gold_actions(self, task):
        return {"actions": []}

    def score(self, task, docs):
        raise errors.method_not_found("score")

    def view(self, connections, docs):
        raise errors.method_not_found("view")

    def models(self):
        """The model client of the `score` call in progress (a judge's calls, recorded as `world.scoring`), when
        shaked gave it an endpoint; else None."""
        return getattr(self, "_scoring_models", None)

    def world_data(self):
        """The world's data (`worlddata.WorldData`) of the `gold_actions` or `score` call in progress, when the
        run's world has some; else None."""
        return getattr(self, "_world_data", None)


def load(path: Path) -> dict:
    """A benchmark's config: its JSON data file."""
    return json.loads(Path(path).read_text(encoding="utf-8"))


def check(name, passed, detail, safety=False, measure=None) -> dict:
    """A named check over the end state (docs/protocol.md, "Scoring by named checks"); `measure` only when
    given."""
    out = {"name": name, "passed": bool(passed), "detail": detail, "safety": safety}
    return {**out, "measure": measure} if measure else out


class SingleTaskBenchmark(Benchmark):
    """A benchmark with one task over a world of simulated parts, scored by named checks and drawn as a twin.

    Give `NAME`, `VERSION`, `TASK` (the task's id), `DATA` (the config's JSON file), `INSTRUCTIONS`,
    `INSTRUCTIONS_TITLE`, `TEAM` and `TEAMS` (the suggested agent setups), and `world()` (the parts, each
    `{part, integration, config}`), `task()` (the task), `evaluate(docs)` (the checks) and `twin(docs)` (the
    view). The view draws every part's docs."""
    NAME = ""
    VERSION = ""
    TASK = ""
    DATA: Path | None = None
    INSTRUCTIONS = ""
    INSTRUCTIONS_TITLE = ""
    TEAM: dict = {}
    TEAMS: list = []

    def __init__(self, config: dict | None = None):
        self.config = config if config is not None else load(self.DATA)

    def world(self) -> list:
        raise NotImplementedError("world")

    def task(self) -> dict:
        raise NotImplementedError("task")

    def evaluate(self, docs) -> list:
        raise NotImplementedError("evaluate")

    def twin(self, docs) -> dict:
        raise NotImplementedError("twin")

    def describe(self):
        world = self.world()
        return {"name": self.NAME, "version": self.VERSION, "protocol": 1, "world": world,
                "tasks": [self.task()], "instructions": self.INSTRUCTIONS,
                "instructions_title": self.INSTRUCTIONS_TITLE, "team": self.TEAM, "teams": list(self.TEAMS),
                "tools": [], "scored": True, "view": {"docs": [w["part"] for w in world]}}

    def score(self, task, docs):
        self._task(task)
        return {"checks": self.evaluate(docs)}

    def view(self, connections, docs):
        return {"view": self.twin(docs)}

    def _task(self, task):
        if task != self.TASK:
            raise errors.invalid(f"unknown task {task}")


def _seed(bench, params):
    bench.seed(*_need(params, "connections"))
    return {}


def _score(bench, params):
    """`score`, with the call's model client and world data set while it runs."""
    bench._scoring_models = Models.of(params.get("models")) if isinstance(params, dict) else None
    bench._world_data = worlddata.of(params)
    try:
        return bench.score(*_need(params, "task", "docs"))
    finally:
        bench._scoring_models = None
        bench._world_data = None


def _gold_actions(bench, params):
    """`gold_actions`, with the call's world data set while it runs."""
    bench._world_data = worlddata.of(params)
    try:
        return bench.gold_actions(*_need(params, "task"))
    finally:
        bench._world_data = None


_METHODS = {
    "describe": lambda b, p: announced(b.describe()),
    "seed": _seed,
    "call_tool": lambda b, p: b.call_tool(*_need(p, "name", "args", "connections")),
    "gold_actions": _gold_actions,
    "score": _score,
    "view": lambda b, p: b.view(*_need(p, "connections", "docs")),
}


def dispatch(bench, method, params):
    handler = _METHODS.get(method)
    if handler is None:
        raise errors.method_not_found(method)
    return handler(bench, params)


def serve_benchmark(bench, stdin=None, stdout=None):
    """Serve a benchmark until the input closes."""
    serve(bench, stdin, stdout, dispatch_fn=dispatch)
