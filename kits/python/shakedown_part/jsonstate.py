"""A part whose instance is a config plus one JSON document of state, with no clock: exact saves as one
`state.json`, keyed documents for `inspect` and `diff`, and a table of tools that each either change the
state or refuse and change nothing.

Subclass `JsonStatePart` and give it `NAME` (the package name, which names its save folders), `TOOLS`
(`{name: Tool}`), `describe_fields()` (`version`, `fingerprint`, optionally `system`), `new_state(config)`
(a fresh state; validate the config there) and `docs(config, state)` (`{key: doc}`).

A tool that a hard limit refuses raises `Denied`: the part records it in `state["denials"]` (which `new_state`
starts empty) and `denials_doc(state)` is its `_audit/denials` document.

A part with scheduled disruptions (`disruptions`) also gives `DISRUPTIONS` (`{do.kind: Kind}`) and
`WORLD_TOOLS` (the tools a task's setup or a link calls: they are not agents' calls, so they count
for no disruption). Its scenario tool arms them with `disruptions.arm`.
"""
import copy
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from . import disruptions, errors, saves
from .docspart import DocsPart, pick
from .params import Refusal, input_schema, normalize, refused

FILE = "state.json"
DENIALS = "_audit/denials"


@dataclass
class Done:
    """What a tool did: its result, the keys it read and wrote, and the events it gives the world."""
    result: object
    reads: list = field(default_factory=list)
    writes: list = field(default_factory=list)
    events: list = field(default_factory=list)


@dataclass(frozen=True)
class Tool:
    name: str
    description: str
    effect: str  # "read" or "write"
    params: tuple  # of params.Param
    run: Callable  # (config, state, args) -> Done; raises Refusal to refuse

    def schema(self) -> dict:
        return {"name": self.name, "description": self.description, "effect": self.effect,
                "inputSchema": input_schema(self.params)}


@dataclass
class Instance:
    config: dict
    state: dict


class Denied(Refusal):
    """A call a hard limit refuses: the agent gets `body` (structured, as JSON) as the tool's error, and the
    world gets a `denial` event with `text`. The part keeps a record of it in `state["denials"]`."""

    def __init__(self, body: dict, text: str):
        super().__init__(json.dumps(body))
        self.body = body
        self.text = text


def denial(state: dict, fields: dict) -> dict:
    """The record of the next denial on `state["denials"]`: numbered from 1, then `fields`."""
    return {"n": len(state["denials"]) + 1, **fields}


def denials_doc(state: dict) -> dict:
    """The `_audit/denials` document: every denial recorded."""
    return {DENIALS: {"denials": state["denials"]}}


def event(kind: str, text: str, data: dict) -> dict:
    """A world event for `Done.events`."""
    return {"kind": kind, "text": text, "data": data}


class JsonStatePart(DocsPart):
    NAME = ""
    TOOLS: dict = {}
    DISRUPTIONS: dict = {}
    WORLD_TOOLS: frozenset = frozenset()

    def __init__(self):
        super().__init__()  # self.instances: {id: Instance}

    def describe_fields(self) -> dict:
        raise NotImplementedError("describe_fields")

    def new_state(self, config) -> dict:
        raise NotImplementedError("new_state")

    def docs(self, config, state) -> dict:
        raise NotImplementedError("docs")

    def describe(self):
        fixed = {"name": self.NAME, "protocol": 1, "restore": {"exact": "save", "approximate": False},
                 "uses_clock": False}
        return {**fixed, **self.describe_fields(), "tools": [t.schema() for t in self.TOOLS.values()]}

    def start(self, instance, config, seed, clock_ms, from_=None):
        config = config or {}
        state = self.new_state(config)
        if from_ is not None:
            if not isinstance(from_, dict) or "save" not in from_:
                raise errors.invalid("invalid params: from needs a save")
            state = _read(saves.root(self.NAME) / from_["save"])["state"]
        self.instances[instance] = Instance(config, state)

    def call_tool(self, instance, name, args, clock_ms):
        """Runs the tool on a copy of the state and keeps the copy only when the tool did not refuse.
        The disruptions an agent's call sets off fire first and stay, even when the tool refuses."""
        inst = self._instance(instance)
        tool = self.TOOLS.get(name)
        if tool is None:
            raise errors.invalid(f"unknown tool {name}")
        before = self._scheduled(inst, name)
        work = copy.deepcopy(inst.state)
        try:
            done = tool.run(inst.config, work, normalize(name, tool.params, args))
        except (Refusal, ValueError) as e:
            return disruptions.merged(self.on_refused(inst, name, args, e), before)
        inst.state = work
        out = {"result": done.result, "reads": done.reads, "writes": done.writes}
        if done.events:
            out["events"] = done.events
        return disruptions.merged(out, before)

    def on_refused(self, inst, name, args, error) -> dict:
        """The answer to a call the tool refused. A `Denied` is recorded on the state (its `denials`) and
        reported as a `denial` event; other refusals change nothing. A part may override it: it may change
        `inst.state`, name the keys in `writes` and give `events`."""
        if not isinstance(error, Denied):
            return refused(str(error))
        record = denial(inst.state, {"tool": name, "args": args, **error.body})
        inst.state = {**inst.state, "denials": [*inst.state["denials"], record]}
        return {"error": str(error), "reads": [], "writes": [DENIALS],
                "events": [event("denial", error.text, record)]}

    def _scheduled(self, inst, name) -> disruptions.Fired:
        """Fires and counts the disruptions an agent's call of `name` sets off, on the state itself."""
        if name in self.WORLD_TOOLS or not disruptions.pending(inst.state, name):
            return disruptions.Fired()
        state = copy.deepcopy(inst.state)
        fired = disruptions.on_call(inst.config, state, self.DISRUPTIONS, name)
        inst.state = state
        return fired

    def save(self, instance, save, dir):
        inst = self._instance(instance)
        body = json.dumps({"config": inst.config, "state": inst.state}, separators=(",", ":"), sort_keys=True)
        Path(dir).mkdir(parents=True, exist_ok=True)
        (Path(dir) / FILE).write_text(body, encoding="utf-8")
        return {"bytes": len(body.encode()), "exact": True}

    def live_docs(self, instance) -> dict:
        inst = self._instance(instance)
        return self.docs(inst.config, inst.state)

    def saved_docs(self, save_dir) -> dict:
        saved = _read(save_dir)
        return self.docs(saved["config"], saved["state"])

    def inspect(self, target, keys):
        return {"docs": pick(json.loads(json.dumps(self.docs_of(target))), keys)}


class ClockedJsonStatePart(JsonStatePart):
    """A JsonStatePart on the world clock. Its state keeps `clock_ms` (how far the world has run; a fresh
    instance starts at the start's clock) and `queued` (the events found on the way, which wait for the next
    `advance`). Give `step(config, state, clock_ms)`: it runs the state from `state["clock_ms"]` to
    `clock_ms`, moving `state["clock_ms"]`, and gives the events on the way. Every call runs the world to its
    clock first; a clock behind the world's changes nothing."""

    def describe(self):
        return {**super().describe(), "uses_clock": True}

    def step(self, config, state, clock_ms) -> list:
        raise NotImplementedError("step")

    def start(self, instance, config, seed, clock_ms, from_=None):
        super().start(instance, config, seed, clock_ms, from_)
        if from_ is None:
            state = self._instance(instance).state
            state["clock_ms"] = clock_ms
            state.setdefault("queued", [])

    def _run_to(self, inst, clock_ms) -> None:
        inst.state["queued"] += self.step(inst.config, inst.state, clock_ms)

    def advance(self, instance, clock_ms):
        inst = self._instance(instance)
        before = inst.state["clock_ms"]
        self._run_to(inst, clock_ms)
        events, inst.state["queued"] = inst.state["queued"], []
        return {"changed": bool(events) or inst.state["clock_ms"] != before, "events": events}

    def call_tool(self, instance, name, args, clock_ms):
        self._run_to(self._instance(instance), clock_ms)
        return super().call_tool(instance, name, args, clock_ms)


def set_scenario(config, state, args, known, overlay, kinds, tools) -> Done:
    """A scenario tool's work: sets `state["scenario"]` to `args["name"]` (one of `known`) and arms
    the disruptions the scenario's data lists (`overlay(config, state)["disruptions"]`), then those
    in `args["disruptions"]`, as `disruptions.arm` does with `kinds` and the part's `tools`."""
    if args["name"] not in known:
        raise Refusal(f"unknown scenario {args['name']} (known: {', '.join(known)})")
    state["scenario"] = args["name"]
    given = [*overlay(config, state).get("disruptions", []), *(args.get("disruptions") or [])]
    if not given:
        return Done({"scenario": args["name"]}, writes=["scenario/current"])
    fired = disruptions.arm(config, state, kinds, list(tools), given)
    result = {"scenario": args["name"], "disruptions": [d["id"] for d in given]}
    return Done(result, writes=["scenario/current", *fired.writes], events=fired.events)



@dataclass(frozen=True)
class Scenarios:
    """A part's scenarios: `scenarios` in its config at `path` (`("lims",)` reads `config["lims"]["scenarios"]`),
    each `{name: overlay}`, beside the base scenario, which changes nothing. `sort` lists them by name rather
    than in the config's order."""
    path: tuple = ()
    sort: bool = False
    base: str = "base"

    def _all(self, config) -> dict:
        for key in self.path:
            config = config[key]
        return config.get("scenarios", {})

    def names(self, config) -> list:
        named = self._all(config)
        return [self.base, *(sorted(named) if self.sort else named)]

    def overlay(self, config, state) -> dict:
        """What the instance's scenario changes (empty for the base)."""
        return self._all(config).get(state["scenario"], {})

    def set(self, config, state, args, kinds, tools) -> Done:
        """The scenario tool's work (`set_scenario`), with `kinds` and `tools` the part's own."""
        return set_scenario(config, state, args, self.names(config), self.overlay, kinds, tools)


def _read(save_dir) -> dict:
    try:
        return json.loads((Path(save_dir) / FILE).read_text(encoding="utf-8"))
    except OSError:
        raise errors.unknown(f"unknown save {Path(save_dir).name}") from None
