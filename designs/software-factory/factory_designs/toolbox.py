"""The tools both designs give their agents, with an ``AgentEndpoint``'s tool surface (``id``,
``tools``, ``call``), so part 1's adapters build each framework's tools from it unchanged:

    box = ToolBox(a.id, bench, run, ["shell", "read_file"])
    langgraph.tools(box)          # or openai_agents.tools(box)
"""

from __future__ import annotations

import os
import time
from datetime import datetime, timezone
from typing import Any, Callable, Optional

from shakedown.mcp import Tool, ToolResult

from .workbench import Workbench
from shakedown.settings import setting

TOOL_NAMES = ("shell", "read_file", "write_file", "run_tests", "wait_minutes")
#: The longest one ``wait_minutes`` call waits, in world minutes.
MAX_WAIT_MINUTES = 30.0

_STR = {"type": "string"}
#: Each tool's description and JSON schema: the same for every design.
SPECS: dict[str, dict[str, Any]] = {
    "shell": {
        "description": "Run a shell command in your workspace (git, gh, curl, python and the "
                       "company's API clients are installed). Answers the exit code and the "
                       "output's tail. There is no network except the company's services.",
        "properties": {"cmd": {**_STR, "description": "The command, run with sh -euc."},
                       "cwd": {**_STR, "description": "Folder in your workspace (default .)."}},
        "required": ["cmd"],
    },
    "read_file": {
        "description": "Read a text file in your workspace.",
        "properties": {"path": {**_STR, "description": "Path in your workspace."}},
        "required": ["path"],
    },
    "write_file": {
        "description": "Write a text file in your workspace, replacing it if it exists.",
        "properties": {"path": {**_STR, "description": "Path in your workspace."},
                       "text": {**_STR, "description": "The file's whole new content."}},
        "required": ["path", "text"],
    },
    "run_tests": {
        "description": "Run the tests (python -m pytest -q) in a folder of your workspace.",
        "properties": {"cwd": {**_STR, "description": "The repository's folder (default .)."},
                       "args": {**_STR, "description": "Extra pytest arguments, e.g. a path."}},
        "required": [],
    },
    "wait_minutes": {
        "description": "Wait while world time passes (at most 30 minutes), then answer the "
                       "world's time. Use it between checks when there is nothing to do.",
        "properties": {"minutes": {"type": "number", "description": "World minutes to wait."}},
        "required": ["minutes"],
    },
}


def tool(name: str) -> Tool:
    spec = SPECS[name]
    schema = {"type": "object", "properties": spec["properties"], "required": spec["required"]}
    return Tool(name=name, description=spec["description"], input_schema=schema)


def wait_minutes(run: Any, minutes: float, poll_s: float = 2.0) -> str:
    """Waits until the run's world clock moved ``minutes`` (at most 30), polling every
    ``poll_s`` s; a paused clock stands still, so a pause lengthens the wait."""
    want_ms = min(max(float(minutes), 0.0), MAX_WAIT_MINUTES) * 60_000
    clock = run.clock()
    start = clock["clock_ms"]
    while clock["clock_ms"] - start < want_ms:
        time.sleep(poll_s)
        clock = run.clock()
    at = datetime.fromtimestamp(clock["world_ms"] / 1000, tz=timezone.utc)
    return f"World time is {at.strftime('%Y-%m-%dT%H:%M:%SZ')}."


def window_minutes(run: Any) -> Optional[float]:
    """The run's world time window in minutes: ``SHAKEDOWN_WORLD_MINUTES`` when shaked gives it,
    else the run's start record; None when the run has no window."""
    if setting("SHAKEDOWN_WORLD_MINUTES"):
        return float(setting("SHAKEDOWN_WORLD_MINUTES"))
    try:
        value = run.branch.record().run_start.get("world_minutes")
    except Exception:  # noqa: BLE001 - an older shaked: no window known
        return None
    return float(value) if value else None


def time_left(run: Any) -> bool:
    """Whether the run's window is known and has world time left."""
    m = window_minutes(run)
    return bool(m) and run.clock()["clock_ms"] < m * 60_000


def minutes_text(run: Any) -> str:
    """The window for a prompt: ``60``, or ``a limited number of`` when it is not known."""
    m = window_minutes(run)
    return f"{m:g}" if m else "a limited number of"


class ToolBox:
    """The tools ``names`` of agent ``id``, run on ``bench`` (and the run's clock)."""

    def __init__(self, id: str, bench: Workbench, run: Any, names: list[str]):
        unknown = [n for n in names if n not in SPECS]
        if unknown:
            raise ValueError(f"no such tool: {', '.join(unknown)}")
        self.id = id
        self.bench = bench
        self.run = run
        self.tools = [tool(n) for n in names]
        self._calls: dict[str, Callable[..., str]] = {
            "shell": bench.shell, "read_file": bench.read_file, "write_file": bench.write_file,
            "run_tests": bench.run_tests, "wait_minutes": lambda minutes: wait_minutes(run, minutes),
        }

    def call(self, name: str, arguments: dict[str, Any] | None = None, **kw: Any) -> ToolResult:
        if name not in {t.name for t in self.tools}:
            return ToolResult(f"{name} is not one of your tools", is_error=True)
        args = {**(arguments or {}), **kw}
        try:
            return ToolResult(self._calls[name](**args), is_error=False)
        except TypeError as e:
            return ToolResult(f"bad arguments for {name}: {e}", is_error=True)
