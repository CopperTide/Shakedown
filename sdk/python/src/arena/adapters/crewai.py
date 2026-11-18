"""CrewAI: an agent's tools as CrewAI tools, its gateway as an ``LLM``, and a step callback that
records what the agent thought at each step.

    from arena.adapters import crewai as sd
    from crewai import Agent, Crew, Task

    with sd.run() as run:
        a = run.agent("support", provider="openai", model="gpt-4.1-mini")
        agent = Agent(role="Support", goal="Resolve the ticket", backstory="...",
                      tools=sd.tools(a), llm=sd.llm(a, "gpt-4.1-mini"), step_callback=sd.step_callback(a))
        Crew(agents=[agent], tasks=[Task(description=run.ticket, expected_output="...", agent=agent)]).kickoff()
"""

from __future__ import annotations

import os
from typing import Any, Callable

# CrewAI sends usage telemetry of its own by default; a run's tickets and tools stay in Shakedown.
os.environ.setdefault("CREWAI_DISABLE_TELEMETRY", "true")

from crewai import LLM  # noqa: E402
from crewai.tools import BaseTool  # noqa: E402

from ..driver import run  # noqa: E402
from ._schema import GATEWAY_KEY, call_text, model_from_schema  # noqa: E402

__all__ = ["tools", "llm", "step_callback", "run"]


class ShakedownTool(BaseTool):
    """One of the agent's tools; each call runs in the world as this agent and is recorded."""

    endpoint: Any = None
    tool_name: str = ""

    def _run(self, **kwargs: Any) -> str:
        return call_text(self.endpoint, self.tool_name, kwargs)


def _tool(agent: Any, t: Any) -> BaseTool:
    return ShakedownTool(
        name=t.name,
        description=t.description or t.name,
        args_schema=model_from_schema(t.name, t.input_schema),
        endpoint=agent,
        tool_name=t.name,
    )


def tools(agent: Any) -> list[BaseTool]:
    return [_tool(agent, t) for t in agent.tools]


def llm(agent: Any, model: str, **kw: Any) -> LLM:
    """An ``LLM`` whose calls go through the agent's gateway (OpenAI-compatible)."""
    return LLM(model=f"openai/{model}", base_url=agent.gateway_url, api_key=GATEWAY_KEY, **kw)


def step_callback(agent: Any) -> Callable[[Any], None]:
    """A ``step_callback`` that records each step's thought (or its text, when it has none) as the
    agent's reasoning."""

    def record(step: Any) -> None:
        text = getattr(step, "thought", None) or getattr(step, "text", None)
        if isinstance(text, str) and text.strip():
            try:
                agent.report_reasoning(text, kind="step")
            except Exception:  # noqa: BLE001 - a note on a step never stops the crew
                pass

    return record
