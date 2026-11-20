"""The OpenAI Agents SDK: an agent's tools as ``FunctionTool``\\ s and its gateway as a
chat-completions model.

    from shakedown.adapters import openai_agents as sd
    from agents import Agent, Runner

    with sd.run() as run:
        a = run.agent("support", provider="openai", model="gpt-4.1-mini")
        agent = Agent(name="support", instructions="...", tools=sd.tools(a), model=sd.model(a, "gpt-4.1-mini"))
        Runner.run_sync(agent, run.ticket)
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

import os

from agents import FunctionTool, OpenAIChatCompletionsModel, set_tracing_disabled
from openai import AsyncOpenAI

from ..driver import run
from ._schema import GATEWAY_KEY, call_text

__all__ = ["tools", "model", "run"]


def _tool(agent: Any, t: Any) -> FunctionTool:
    async def invoke(_ctx: Any, args: str) -> str:
        try:
            parsed = json.loads(args or "{}")
        except json.JSONDecodeError as e:
            return f"error: the arguments are not JSON: {e}"
        return await asyncio.to_thread(call_text, agent, t.name, parsed)

    return FunctionTool(
        name=t.name,
        description=t.description or t.name,
        params_json_schema=t.input_schema,
        on_invoke_tool=invoke,
        strict_json_schema=False,
    )


def tools(agent: Any) -> list[FunctionTool]:
    """The agent's tools; each call runs in the world as this agent and is recorded."""
    return [_tool(agent, t) for t in agent.tools]


def model(agent: Any, model: str) -> OpenAIChatCompletionsModel:
    """A model whose calls go through the agent's gateway (chat completions). Without an
    ``OPENAI_API_KEY`` here the SDK's own trace export can't work, so it is turned off; the run's
    record is in Shakedown."""
    if not os.environ.get("OPENAI_API_KEY"):
        set_tracing_disabled(True)
    client = AsyncOpenAI(base_url=agent.gateway_url, api_key=GATEWAY_KEY)
    return OpenAIChatCompletionsModel(model=model, openai_client=client)
