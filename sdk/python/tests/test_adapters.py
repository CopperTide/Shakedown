"""The framework adapters against a stand-in for an agent's endpoint: the tools they build call it
with the model's arguments, and a refused call reaches the agent as text. No shaked, no model."""

from __future__ import annotations

import asyncio
import json
from typing import Any, Optional

import pytest

from arena.mcp import Tool, ToolResult

GET_STOCK = Tool("get_stock", "Units in stock of a SKU.", {
    "type": "object", "properties": {"sku": {"type": "string"}}, "required": ["sku"]})
RESERVE = Tool("reserve", "Reserve units for an order.", {
    "type": "object",
    "properties": {"order": {"type": "string"}, "qty": {"type": "integer"}, "note": {"type": "string"}},
    "required": ["order", "qty"]})
REFUSED = "refused: exceeds the limit"


class FakeEndpoint:
    """Stands in for :class:`arena.AgentEndpoint`: two tools, ``reserve`` always refused."""

    id = "planner"
    gateway_url = "http://127.0.0.1:4510/gateway/b1/planner/v1"

    def __init__(self):
        self.tools = [GET_STOCK, RESERVE]
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self.reasoning: list[dict[str, Any]] = []

    def call(self, name: str, arguments: Optional[dict[str, Any]] = None, **kw: Any) -> ToolResult:
        args = {**(arguments or {}), **kw}
        self.calls.append((name, args))
        if name == "reserve":
            return ToolResult(REFUSED, True)
        return ToolResult(json.dumps({"sku": args.get("sku"), "units": 4}), False)

    def report_reasoning(self, text=None, kind="thinking", model_call_seq=None, items=None):
        self.reasoning.append({"text": text, "kind": kind})
        return [len(self.reasoning)]


def by_name(tools, name):
    return next(t for t in tools if t.name == name)


# ---------------------------------------------------------------- schemas

def test_schema_required_and_optional():
    from arena.adapters._schema import model_from_schema

    M = model_from_schema("reserve", RESERVE.input_schema)
    fields = M.model_fields
    assert fields["order"].is_required() and fields["order"].annotation is str
    assert fields["qty"].is_required() and fields["qty"].annotation is int
    assert not fields["note"].is_required() and fields["note"].default is None
    assert M(order="o1", qty=2).model_dump(exclude_none=True) == {"order": "o1", "qty": 2}


def test_schema_odd_shapes_do_not_fail():
    from arena.adapters._schema import model_from_schema

    odd = {"type": "object", "required": ["where", "lines", "mode", "anything"], "properties": {
        "where": {"type": "object", "properties": {"site": {"type": "string"}}},
        "lines": {"type": "array", "items": {"type": "object", "properties": {"sku": {"type": "string"}}}},
        "mode": {"type": "string", "enum": ["fast", "slow"]},
        "anything": {"description": "no type at all"},
        "either": {"anyOf": [{"type": "string"}, {"type": "integer"}]},
    }}
    M = model_from_schema("odd", odd)
    m = M(where={"site": "A"}, lines=[{"sku": "X"}], mode="fast", anything=[1, "two"])
    assert m.where == {"site": "A"} and m.lines == [{"sku": "X"}] and m.mode == "fast"
    assert model_from_schema("empty", {})().model_dump() == {}


def test_result_text_marks_errors():
    from arena.adapters._schema import result_text

    assert result_text(ToolResult("ok", False)) == "ok"
    assert result_text(ToolResult(REFUSED, True)) == f"error: {REFUSED}"


# ---------------------------------------------------------------- LangGraph (LangChain tools)

def test_langgraph_tool_calls_the_endpoint():
    pytest.importorskip("langchain_core", reason="needs the langgraph extra")
    from arena.adapters import langgraph

    ep = FakeEndpoint()
    tools = langgraph.tools(ep)
    assert [t.name for t in tools] == ["get_stock", "reserve"]
    out = by_name(tools, "get_stock").invoke({"sku": "A"})
    assert ep.calls == [("get_stock", {"sku": "A"})]
    assert json.loads(out) == {"sku": "A", "units": 4}


def test_langgraph_refusal_is_text():
    pytest.importorskip("langchain_core", reason="needs the langgraph extra")
    from arena.adapters import langgraph

    ep = FakeEndpoint()
    out = by_name(langgraph.tools(ep), "reserve").invoke({"order": "o1", "qty": 2})
    assert out == f"error: {REFUSED}"
    assert ep.calls == [("reserve", {"order": "o1", "qty": 2})]


def test_langgraph_chat_model_points_at_the_gateway():
    pytest.importorskip("langchain_openai", reason="needs the langgraph extra")
    from arena.adapters import langgraph

    m = langgraph.chat_model(FakeEndpoint(), "gpt-4.1-mini")
    assert m.model_name == "gpt-4.1-mini"
    assert str(m.openai_api_base).rstrip("/") == FakeEndpoint.gateway_url


# ---------------------------------------------------------------- CrewAI

def test_crewai_tool_calls_the_endpoint():
    pytest.importorskip("crewai", reason="needs the crewai extra")
    from arena.adapters import crewai

    ep = FakeEndpoint()
    tools = crewai.tools(ep)
    out = by_name(tools, "get_stock")._run(sku="A")
    assert ep.calls == [("get_stock", {"sku": "A"})]
    assert json.loads(out) == {"sku": "A", "units": 4}


def test_crewai_refusal_is_text():
    pytest.importorskip("crewai", reason="needs the crewai extra")
    from arena.adapters import crewai

    ep = FakeEndpoint()
    out = by_name(crewai.tools(ep), "reserve")._run(order="o1", qty=2)
    assert out == f"error: {REFUSED}"


def test_crewai_llm_points_at_the_gateway():
    pytest.importorskip("crewai", reason="needs the crewai extra")
    from arena.adapters import crewai

    llm = crewai.llm(FakeEndpoint(), "gpt-4.1-mini")
    assert llm.model.endswith("gpt-4.1-mini")
    assert (llm.base_url or "").rstrip("/") == FakeEndpoint.gateway_url


def test_crewai_step_callback_records_thought():
    pytest.importorskip("crewai", reason="needs the crewai extra")
    from arena.adapters import crewai

    class Step:  # the fields of CrewAI's AgentAction the callback reads
        thought = "Check the stock before reserving."
        text = "Thought: Check the stock before reserving.\nAction: get_stock"

    ep = FakeEndpoint()
    crewai.step_callback(ep)(Step())
    assert ep.reasoning == [{"text": Step.thought, "kind": "step"}]


# ---------------------------------------------------------------- OpenAI Agents SDK

def test_openai_agents_tool_calls_the_endpoint():
    pytest.importorskip("agents", reason="needs the openai-agents extra")
    from arena.adapters import openai_agents

    ep = FakeEndpoint()
    t = by_name(openai_agents.tools(ep), "get_stock")
    assert t.strict_json_schema is False and t.params_json_schema == GET_STOCK.input_schema
    out = asyncio.run(t.on_invoke_tool(None, '{"sku":"A"}'))
    assert ep.calls == [("get_stock", {"sku": "A"})]
    assert json.loads(out) == {"sku": "A", "units": 4}


def test_openai_agents_refusal_is_text():
    pytest.importorskip("agents", reason="needs the openai-agents extra")
    from arena.adapters import openai_agents

    t = by_name(openai_agents.tools(FakeEndpoint()), "reserve")
    assert asyncio.run(t.on_invoke_tool(None, '{"order":"o1","qty":2}')) == f"error: {REFUSED}"


def test_openai_agents_model_points_at_the_gateway():
    pytest.importorskip("agents", reason="needs the openai-agents extra")
    from arena.adapters import openai_agents

    m = openai_agents.model(FakeEndpoint(), "gpt-4.1-mini")
    assert m.model == "gpt-4.1-mini"
    assert str(m._client.base_url).rstrip("/") == FakeEndpoint.gateway_url


def test_each_adapter_offers_the_driver():
    from arena import driver

    for name in ("langgraph", "crewai", "openai_agents"):
        mod = pytest.importorskip(f"arena.adapters.{name}")
        assert mod.run is driver.run


# ---------------------------------------------------------------- review fixes

ODD = Tool("odd", "Odd property names.", {
    "type": "object",
    "properties": {"order-id": {"type": "string"}, "json": {"type": "boolean"}, "from": {"type": "string"}},
    "required": ["order-id"]})


def test_openai_agents_arguments_that_are_not_an_object_are_text():
    pytest.importorskip("agents", reason="needs the openai-agents extra")
    from arena.adapters import openai_agents

    t = by_name(openai_agents.tools(FakeEndpoint()), "get_stock")
    for args in ("[1]", "null", '"sku"'):
        out = asyncio.run(t.on_invoke_tool(None, args))
        assert out.startswith("error: "), (args, out)


def test_crewai_odd_property_names_reach_the_endpoint_as_named():
    pytest.importorskip("crewai", reason="needs the crewai extra")
    from arena.adapters import crewai

    ep = FakeEndpoint()
    ep.tools = [ODD]
    tool = crewai.tools(ep)[0]
    tool.run(**{"order-id": "o1", "json": True, "from": "x"})
    assert ep.calls == [("odd", {"order-id": "o1", "json": True, "from": "x"})]
