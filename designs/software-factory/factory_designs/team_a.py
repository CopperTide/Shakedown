"""Design A: a hierarchical LangGraph team. A read-only planner plans the sprint; for each work
item a coder opens a pull request, a reviewer approves it or sends it back (at most
``review_rounds`` times), and release merges and watches production; then release stays on call
until the run's world time window ends. The roles, their tools and prompts are data in
``team_a/``; this module is the graph and how each node talks to its role.

    python -m factory_designs.team_a      # as a Shakedown Code connection's driver
"""

from __future__ import annotations

import json
from functools import partial
from pathlib import Path
from typing import Any, Callable, Optional, TypedDict

import yaml
from langchain_core.messages import BaseMessage
from langgraph.errors import GraphRecursionError
from langgraph.graph import END, START, StateGraph
from langgraph.graph.state import CompiledStateGraph
from langgraph.prebuilt import create_react_agent

from arena.adapters import langgraph

from .prompts import fill_in, load_prompt
from .settings import Settings, load_settings
from .stops import run_design
from .toolbox import ToolBox, minutes_text
from .workbench import Workbench, benches

DESIGN = Path(__file__).resolve().parent.parent / "team_a"
VERDICTS = ("approve", "changes", "reject")
#: What ends an item, by the reviewer's last verdict.
ITEM_ENDS = {"approve": "released", "reject": "rejected", "changes": "review rounds used up"}

Bench = Callable[[str, Any], Workbench]


class TeamState(TypedDict):
    ticket: str
    items: list[dict]
    index: int
    pr: Optional[str]
    verdict: Optional[str]
    rounds: int
    notes: list[str]


def load_design(folder: Path = DESIGN) -> dict:
    return yaml.safe_load((folder / "design.yaml").read_text())


def initial_state(run: Any) -> TeamState:
    return TeamState(ticket=run.ticket, items=[], index=0, pr=None, verdict=None, rounds=0,
                     notes=[])


def parse_answer(text: str, keys: set[str]) -> Optional[dict]:
    """The last JSON object in ``text`` (bare or fenced) that has every key in ``keys``."""
    decoder = json.JSONDecoder()
    starts = [i for i, c in enumerate(text) if c == "{"]
    for i in reversed(starts):
        try:
            obj, _ = decoder.raw_decode(text, i)
        except ValueError:
            continue
        if isinstance(obj, dict) and keys <= obj.keys():
            return obj
    return None


def _text(message: BaseMessage) -> str:
    c = message.content
    if isinstance(c, str):
        return c
    return "".join(b.get("text", "") if isinstance(b, dict) else str(b) for b in c)


# ---------------------------------------------------------------------------- routers


def next_item(state: TeamState) -> str:
    return "code" if state["index"] < len(state["items"]) else "monitor"


def after_review(state: TeamState, rounds_max: int) -> str:
    if state["verdict"] == "approve":
        return "release"
    if state["verdict"] == "changes" and state["rounds"] < rounds_max:
        return "code"
    return "finish_item"


# ---------------------------------------------------------------------------- the team


class Team:
    """The roles of design A on one run, each registered once and asked through its own
    LangGraph ReAct agent, with its tools on its own workbench."""

    def __init__(self, run: Any, settings: Settings, bench: Bench, folder: Path = DESIGN):
        self.run = run
        self.settings = settings
        self.bench = bench
        self.folder = folder
        self.design = load_design(folder)
        self._agents: dict[str, Any] = {}

    def endpoint(self, role: str) -> Any:
        spec, s = self.design["roles"][role], self.settings
        return self.run.agent(role, provider=s.provider, model=s.model,
                              read_only=bool(spec["read_only"]))

    def _agent(self, role: str) -> Any:
        if role not in self._agents:
            spec, s, ep = self.design["roles"][role], self.settings, self.endpoint(role)
            model = langgraph.chat_model(ep, s.model, reasoning_effort=s.reasoning_effort,
                                         max_tokens=s.max_tokens, temperature=s.temperature)
            box = ToolBox(ep.id, self.bench(role, ep), self.run, list(spec["tools"]))
            prompt = load_prompt(self.folder / spec["prompt"], world_minutes=minutes_text(self.run),
                                 role_ids=", ".join(self.design["roles"]))
            self._agents[role] = create_react_agent(model, langgraph.tools(box), prompt=prompt)
        return self._agents[role]

    def ask(self, role: str, messages: list) -> tuple[str, Optional[list]]:
        """The role's final answer to ``messages`` and the whole conversation; ``("", None)``
        when it ran out of turns."""
        limit = 2 * int(self.design["roles"][role]["max_turns"]) + 1
        try:
            out = self._agent(role).invoke({"messages": messages}, {"recursion_limit": limit})
        except GraphRecursionError:
            return "", None
        return _text(out["messages"][-1]), out["messages"]

    def say(self, node: str, state: TeamState) -> list:
        """The node's input message to its role, from the design's ``inputs`` and the state."""
        i = state["index"]
        item = state["items"][i] if i < len(state["items"]) else {}
        notes = "\n".join(f"- {n}" for n in state["notes"]) or "- none"
        values = {"ticket": state["ticket"], "item": json.dumps(item, indent=2),
                  "pr": state["pr"] or "none yet", "notes": notes}
        return [("user", fill_in(self.design["inputs"][node], values, f"inputs.{node}"))]

    # ------------------------------------------------------------------------ nodes

    def plan(self, state: TeamState) -> dict:
        text, conversation = self.ask("planner", self.say("plan", state))
        items = _items(parse_answer(text, {"items"}))
        if items is None and conversation is not None:
            retry = [*conversation, ("user", self.design["inputs"]["plan_retry"])]
            items = _items(parse_answer(self.ask("planner", retry)[0], {"items"}))
        if items is None:
            items = [{"ticket": "", "repo": "", "summary": state["ticket"], "risks": [], "contracts": []}]
        self.endpoint("planner").report_reasoning(json.dumps({"items": items}, indent=2), kind="plan")
        return {"items": items, "index": 0}

    def code(self, state: TeamState) -> dict:
        rounds = state["rounds"] + (1 if state["verdict"] == "changes" else 0)
        answer = parse_answer(self.ask("coder", self.say("code", state))[0], {"pr"}) or {}
        notes = [*state["notes"], f"coder: {answer.get('notes') or 'no notes'}"]
        return {"pr": answer.get("pr") or state["pr"], "rounds": rounds, "notes": notes}

    def review(self, state: TeamState) -> dict:
        answer = parse_answer(self.ask("reviewer", self.say("review", state))[0], {"verdict"}) or {}
        verdict = answer.get("verdict") if answer.get("verdict") in VERDICTS else "changes"
        why = answer.get("notes") or "the reviewer gave no verdict"
        self.endpoint("reviewer").report_reasoning(f"{verdict}: {why}", kind="step")
        return {"verdict": verdict, "pr": answer.get("pr") or state["pr"],
                "notes": [*state["notes"], f"reviewer ({verdict}): {why}"]}

    def release(self, state: TeamState) -> dict:
        text, _ = self.ask("release", self.say("release", state))
        return {"notes": [*state["notes"], f"release: {text.strip()[:500] or 'no summary'}"]}

    def finish_item(self, state: TeamState) -> dict:
        item = state["items"][state["index"]]
        end = ITEM_ENDS.get(state["verdict"] or "changes", "done")
        note = f"item {state['index'] + 1} ({item.get('ticket') or 'the ticket'}): {end}"
        return {"index": state["index"] + 1, "pr": None, "verdict": None, "rounds": 0,
                "notes": [*state["notes"], note]}

    def monitor(self, state: TeamState) -> dict:
        self.ask("release", self.say("monitor", state))
        return {}


def _items(answer: Optional[dict]) -> Optional[list[dict]]:
    items = (answer or {}).get("items")
    ok = isinstance(items, list) and items and all(isinstance(i, dict) for i in items)
    return items if ok else None


def build_graph(run: Any, settings: Settings, bench: Bench, folder: Path = DESIGN) -> CompiledStateGraph:
    """The team's graph: ``START → plan → next_item``; each item ``code → review``, then release
    (approved), code again (changes, while rounds are left) or the next item; at the end
    ``monitor → END``."""
    team = Team(run, settings, bench, folder)
    g = StateGraph(TeamState)
    for node in ("plan", "code", "review", "release", "finish_item", "monitor"):
        g.add_node(node, getattr(team, node))
    g.add_edge(START, "plan")
    g.add_conditional_edges("plan", next_item, ["code", "monitor"])
    g.add_edge("code", "review")
    rounds = int(team.design["review_rounds"])
    g.add_conditional_edges("review", partial(after_review, rounds_max=rounds),
                            ["release", "code", "finish_item"])
    g.add_edge("release", "finish_item")
    g.add_conditional_edges("finish_item", next_item, ["code", "monitor"])
    g.add_edge("monitor", END)
    return g.compile()


def main(run: Any) -> None:
    settings = load_settings()
    steps = int(load_design()["max_graph_steps"])
    graph = build_graph(run, settings, benches(settings))
    graph.invoke(initial_state(run), {"recursion_limit": steps})


if __name__ == "__main__":
    run_design(main)
