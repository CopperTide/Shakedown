"""Scheduled disruptions for a JSON-state part (docs/protocol.md, "Scheduled disruptions").

A task's setup arms them through the part's scenario tool; each then fires once, deterministically,
before the part answers the agent's call that crosses its count, so that call already sees the
changed world. The schedule is part of the state, so a save restores it exactly and a replay repeats
it.

A disruption is `{id, when: {after_calls, tool?}, do: {kind, ...}, text}`. `after_calls` counts the
agents' calls this part answered since setup (only calls of `tool` when it is given); one with
`after_calls` 0 and no `tool` fires as it is armed. A part says which `do.kind`s it knows with a
`Kind` each: the fields `do` needs and how it changes the state.

A timed disruption happens at a world time instead: `{id, at, known_at, at_ms, known_at_ms, do, text}`,
`at_ms` the branch clock it takes effect at and `known_at_ms` the clock the world learns of it (each
defaults to the other). `arm_timed` keeps them; `fire_timed` (inside the part's own fixed steps) gives an
`alert` at `known_at_ms` (with `data.known_only` true for a forecast, before it hits) and applies it at
`at_ms`, with a `disruption` event only when it was known by then: a part never reveals one before its
`known_at_ms` (docs/protocol.md, "Scheduled disruptions"). `next_wake` is the clock of the next reveal or
effect.
"""
from dataclasses import dataclass, field
from typing import Callable

from .params import TYPES, Refusal

EVENT = "disruption"  # the kind of the world event a disruption gives when it fires
ALERT = "alert"  # the kind of the world event a timed disruption gives when the world learns of it


def _no_check(config, do) -> None:
    return None


@dataclass(frozen=True)
class Kind:
    """One `do.kind`: `fields` maps each field `do` needs to its JSON Schema type; `apply(config,
    state, do)` changes the state and gives the keys it wrote, or a `Fired` with them and world
    events of the part's own that come after the disruption's (a notice a link can tell apart from
    the part's other disruptions); `check(config, do)` raises Refusal for a disruption that can't
    apply (an unknown lane, say), when it is armed."""
    fields: dict
    apply: Callable
    check: Callable = field(default=_no_check)


@dataclass
class Fired:
    """What the schedule did on one call: the keys it wrote and the world events it gives."""
    writes: list = field(default_factory=list)
    events: list = field(default_factory=list)

    def add(self, other: "Fired") -> None:
        self.writes += [k for k in other.writes if k not in self.writes]
        self.events += other.events


def key(d) -> str:
    """The disruption's key: internal (`_`), since it is the part's bookkeeping, not the world's state."""
    return f"_disruption/{d['id']}"


def _when(did, when, tools) -> dict:
    if not isinstance(when, dict):
        raise Refusal(f"disruption {did}: when must be an object")
    n, tool = when.get("after_calls"), when.get("tool")
    if not isinstance(n, int) or isinstance(n, bool) or n < 0:
        raise Refusal(f"disruption {did}: when.after_calls must be a whole number, 0 or more")
    if tool is not None and tool not in tools:
        raise Refusal(f"disruption {did}: when.tool {tool!r} is not a tool of this part")
    return {"after_calls": n, "tool": tool}


def _do(config, kinds, did, do) -> dict:
    if not isinstance(do, dict) or do.get("kind") not in kinds:
        raise Refusal(f"disruption {did}: do.kind must be one of {', '.join(sorted(kinds))}")
    kind = kinds[do["kind"]]
    for name, typ in kind.fields.items():
        value = do.get(name)
        if not isinstance(value, TYPES[typ]) or isinstance(value, bool):
            raise Refusal(f"disruption {did}: do.{name} must be of type {typ}")
    kind.check(config, do)
    return do


def _checked(config, kinds, tools, d) -> dict:
    """`d` checked, as the schedule keeps it (not seen, not fired)."""
    did = d.get("id") if isinstance(d, dict) else None
    if not isinstance(did, str) or not did:
        raise Refusal("each disruption needs an id (text)")
    if not isinstance(d.get("text"), str):
        raise Refusal(f"disruption {did}: text must be text")
    when = _when(did, d.get("when"), tools)
    do = _do(config, kinds, did, d.get("do"))
    return {"id": did, "when": when, "do": do, "text": d["text"], "seen": 0, "fired": False}


def _fire(config, state, kinds, d) -> Fired:
    applied = kinds[d["do"]["kind"]].apply(config, state, d["do"])
    own = applied if isinstance(applied, Fired) else Fired(list(applied))
    d["fired"] = True
    data = {"id": d["id"], **d["do"]}
    return Fired([key(d), *own.writes], [{"kind": EVENT, "text": d["text"], "data": data}, *own.events])


def _fire_all(config, state, kinds, due) -> Fired:
    out = Fired()
    for d in due:
        out.add(_fire(config, state, kinds, d))
    return out


def arm(config, state, kinds, tools, given) -> Fired:
    """Adds the disruptions `given` to the schedule in `state` (refusing the lot if one is bad or an
    id is taken) and fires at once those with `after_calls` 0 and no `tool`. `tools` are the
    part's tool names, which `when.tool` may name."""
    if not isinstance(given, list):
        raise Refusal("disruptions must be a list")
    armed = state.setdefault("disruptions", [])
    new = [_checked(config, kinds, tools, d) for d in given]
    ids = [d["id"] for d in armed + new]
    if len(set(ids)) != len(ids):
        raise Refusal("two disruptions have the same id")
    armed.extend(new)
    out = Fired([key(d) for d in new])
    at_once = [d for d in new if d["when"] == {"after_calls": 0, "tool": None}]
    out.add(_fire_all(config, state, kinds, at_once))
    return out


def remove(state, ids) -> list:
    """Drops the disruptions `ids` from the schedule in `state` (fired or not; what a fired one changed
    stays) and gives their keys; an id not armed is no change."""
    armed = state.get("disruptions", [])
    gone = [d for d in armed if d["id"] in ids]
    state["disruptions"] = [d for d in armed if d["id"] not in ids]
    return [key(d) for d in gone]


def pending(state, tool) -> list:
    """The disruptions not fired yet that a call of `tool` counts for."""
    return [d for d in state.get("disruptions", [])
            if not d["fired"] and d["when"]["tool"] in (None, tool)]


def before_call(config, state, kinds, tool) -> Fired:
    """For an agent's call of `tool`, before it is answered: fires the disruptions whose count it
    crosses, then counts it for the others (each counted disruption's key is written, so the step
    is saved and a restore keeps the count)."""
    counted = pending(state, tool)
    due = [d for d in counted if d["seen"] >= d["when"]["after_calls"]]
    out = _fire_all(config, state, kinds, due)
    for d in counted:
        if not d["fired"]:
            d["seen"] += 1
            out.add(Fired([key(d)]))
    return out


def on_call(config, state, kinds, tool, world_tools=frozenset()) -> Fired:
    """For a call of `tool` on `state` (changed in place): fires and counts the disruptions it sets off,
    unless it is one of the part's `world_tools` (a setup's or a link's call counts for nothing)."""
    if tool in world_tools or not pending(state, tool):
        return Fired()
    return before_call(config, state, kinds, tool)


def merged(out: dict, before: Fired) -> dict:
    """A call_tool answer `out` with what the disruptions that fired before it wrote and gave put first."""
    if not before.writes and not before.events:
        return out
    out["writes"] = before.writes + [k for k in out["writes"] if k not in before.writes]
    events = before.events + out.get("events", [])
    if events:
        out["events"] = events
    return out


def docs(state) -> dict:
    """`{_disruption/<id>: doc}`: each disruption with how many calls it has seen and whether it fired (a
    timed one: whether it is known and applied). The keys are internal (docs/protocol.md, "Ids and keys"):
    shaked saves them but no step writes them."""
    return {key(d): d for d in [*state.get("disruptions", []), *state.get("timed", [])]}


def fired(state) -> list:
    return [d for d in state.get("disruptions", []) if d["fired"]]


def is_timed(d) -> bool:
    """True for a disruption given at a world time (no `when`)."""
    return isinstance(d, dict) and "when" not in d and any(
        k in d for k in ("at_ms", "known_at_ms", "at", "known_at"))


def _clock(did, d, name):
    value = d.get(name)
    if value is not None and (not isinstance(value, int) or isinstance(value, bool)):
        raise Refusal(f"disruption {did}: {name} must be a whole number of ms")
    return value


def _checked_timed(config, kinds, d) -> dict:
    """`d` checked, as the schedule keeps it (not known, not applied)."""
    did = d.get("id") if isinstance(d, dict) else None
    if not isinstance(did, str) or not did:
        raise Refusal("each disruption needs an id (text)")
    if not isinstance(d.get("text"), str):
        raise Refusal(f"disruption {did}: text must be text")
    at, known = _clock(did, d, "at_ms"), _clock(did, d, "known_at_ms")
    if at is None and known is None:
        raise Refusal(f"disruption {did}: give at_ms or known_at_ms, the branch clock it happens or is known at")
    at, known = (known if at is None else at), (at if known is None else known)
    do = _do(config, kinds, did, d.get("do"))
    return {"id": did, "at": d.get("at"), "known_at": d.get("known_at"), "at_ms": at, "known_ms": known,
            "do": do, "text": d["text"], "known": False, "applied": False}


def arm_timed(config, state, kinds, given) -> Fired:
    """Adds the timed disruptions `given` to the schedule in `state` (refusing the lot if one is bad or an
    id is taken, among the counted ones too). Nothing fires here: `fire_timed` does, at their clocks."""
    if not isinstance(given, list):
        raise Refusal("disruptions must be a list")
    timed = state.setdefault("timed", [])
    new = [_checked_timed(config, kinds, d) for d in given]
    ids = [d["id"] for d in [*state.get("disruptions", []), *timed, *new]]
    if len(set(ids)) != len(ids):
        raise Refusal("two disruptions have the same id")
    timed.extend(new)
    return Fired([key(d) for d in new])


def due(state, clock_ms) -> tuple:
    """Takes the timed disruptions due by `clock_ms`: `(reveals, effects)`, those the world learns of and
    those that take effect, each marked so it is taken once, in clock order."""
    timed = state.get("timed", [])
    reveals = sorted((d for d in timed if not d["known"] and d["known_ms"] <= clock_ms),
                     key=lambda d: (d["known_ms"], d["id"]))
    effects = sorted((d for d in timed if not d["applied"] and d["at_ms"] <= clock_ms),
                     key=lambda d: (d["at_ms"], d["id"]))
    for d in reveals:
        d["known"] = True
    for d in effects:
        d["applied"] = True
    return reveals, effects


def _alert(d) -> dict:
    data = {"id": d["id"], "at_ms": d["at_ms"], "known_at_ms": d["known_ms"],
            "known_only": d["known_ms"] < d["at_ms"], **d["do"]}
    return {"kind": ALERT, "text": d["text"], "data": data}


def fire_timed(config, state, kinds, clock_ms) -> Fired:
    """Reveals and applies the timed disruptions due by `clock_ms` (see the module's doc), in clock order, a
    reveal before an effect at the same clock: the keys written and the events given."""
    reveals, effects = due(state, clock_ms)
    steps = sorted([(d["known_ms"], 0, d["id"], d) for d in reveals]
                   + [(d["at_ms"], 1, d["id"], d) for d in effects], key=lambda s: s[:3])
    out = Fired()
    for _, effect, _, d in steps:
        if not effect:
            out.add(Fired([key(d)], [_alert(d)]))
            continue
        written = kinds[d["do"]["kind"]].apply(config, state, d["do"])
        told = d["known_ms"] <= d["at_ms"]
        events = [{"kind": EVENT, "text": d["text"], "data": {"id": d["id"], **d["do"]}}] if told else []
        out.add(Fired([key(d), *written], events))
    return out


def next_wake(state):
    """The branch clock of the next reveal or effect of a timed disruption, or None."""
    times = [d["known_ms"] for d in state.get("timed", []) if not d["known"]]
    times += [d["at_ms"] for d in state.get("timed", []) if not d["applied"]]
    return min(times, default=None)
