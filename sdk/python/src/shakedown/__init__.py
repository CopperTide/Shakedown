"""Shakedown from Python: build environments, run teams, observe, control and export.

    from shakedown import Shakedown, Agent

    sd = Shakedown("http://127.0.0.1:4510")          # or: with Shakedown.local(benchmark="retail") as sd:
    run = sd.run("72", team=[Agent.claude_code("support", model="haiku")], seed=7)
    for event in run.events():                      # every event, live, until the run ends
        print(event.summary())
    print(run.check())
    run.rewind(event=40).fork(model="sonnet")       # back to just before event 40, one thing changed

``arena`` (with ``Arena``) is this package's old name, kept as an alias.

See docs/sdk.md in the Shakedown repository (CopperTide/Shakedown).
"""

from .client import DEFAULT_URL, AgentEndpoint, Branch, Rewind, Shakedown, Suite
from .errors import (
    ApiError,
    ShakedownError,
    BadRequest,
    Conflict,
    ConnectionFailed,
    DiskFull,
    NotFound,
    Unauthorized,
    NotSupported,
)
from . import driver
from .mcp import Tool, ToolResult
from .models import (
    ENDED,
    Agent,
    Benchmark,
    BranchRecord,
    Change,
    CheckResult,
    Disruption,
    Event,
    ForkInfo,
    NamedCheck,
    RunSummary,
    Scenario,
    Task,
    Team,
)

__version__ = "0.2.0"

__all__ = [
    "Shakedown", "Branch", "Rewind", "AgentEndpoint", "Suite", "DEFAULT_URL",
    "Agent", "Team", "Change", "Scenario", "Disruption", "Task", "Benchmark", "Event",
    "BranchRecord", "CheckResult", "NamedCheck", "RunSummary", "ForkInfo", "ENDED",
    "Tool", "ToolResult",
    "ShakedownError", "ApiError", "BadRequest", "Unauthorized", "NotFound", "Conflict", "DiskFull",
    "ConnectionFailed", "NotSupported", "driver",
]
