"""LangGraph and LangChain: an agent's tools as ``StructuredTool``\\ s and its gateway as a
``ChatOpenAI``.

    from arena.adapters import langgraph as sd
    from langgraph.prebuilt import create_react_agent

    with sd.run() as run:
        a = run.agent("support", provider="openai", model="gpt-4.1-mini")
        graph = create_react_agent(sd.chat_model(a, "gpt-4.1-mini"), sd.tools(a))
        graph.invoke({"messages": [("user", run.ticket)]})
"""

from __future__ import annotations

from typing import Any

from langchain_core.tools import StructuredTool
from langchain_openai import ChatOpenAI

from ..driver import run
from ._schema import GATEWAY_KEY, call_text

__all__ = ["tools", "chat_model", "run"]


def _tool(agent: Any, t: Any) -> StructuredTool:
    def call(**kwargs: Any) -> str:
        return call_text(agent, t.name, kwargs)

    return StructuredTool.from_function(
        func=call, name=t.name, description=t.description or t.name, args_schema=t.input_schema
    )


def tools(agent: Any) -> list[StructuredTool]:
    """The agent's tools; each call runs in the world as this agent and is recorded."""
    return [_tool(agent, t) for t in agent.tools]


def chat_model(agent: Any, model: str, **kw: Any) -> ChatOpenAI:
    """A chat model whose calls go through the agent's gateway (recorded, priced, and its
    reasoning kept)."""
    return ChatOpenAI(model=model, base_url=agent.gateway_url, api_key=GATEWAY_KEY, **kw)
