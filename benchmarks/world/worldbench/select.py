"""Which systems a world has (the required ones, the chosen or default ones, and what they require), and which
tasks, roles, events, links and gates stay with them."""
from dataclasses import dataclass, field


def default_systems(t) -> list[str]:
    return [s["id"] for s in t.systems if s.get("default")]


def closure(t, asked: list[str]) -> list[str]:
    """The required systems, `asked`, and every system they require, in template order."""
    by_id = {s["id"]: s for s in t.systems}
    want = {s["id"] for s in t.systems if s.get("required")} | set(asked)
    stack = list(want)
    while stack:
        for r in by_id[stack.pop()].get("requires", []):
            if r not in want:
                want.add(r)
                stack.append(r)
    return [s["id"] for s in t.systems if s["id"] in want]


@dataclass
class Kept:
    """What stays in a world of some systems, and `dropped`: `[{what, why}]` for the rest."""
    tasks: list = field(default_factory=list)
    roles: list = field(default_factory=list)
    events: list = field(default_factory=list)
    links: list = field(default_factory=list)
    gates: list = field(default_factory=list)
    dropped: list = field(default_factory=list)


def _systems_phrase(ids: list[str]) -> str:
    return f"the system {ids[0]}" if len(ids) == 1 else "the systems " + ", ".join(ids)


def _needing_all(items, field_: str, chosen: set, what, dropped: list) -> list:
    kept = []
    for item in items:
        missing = [s for s in item.get(field_, []) if s not in chosen]
        if missing:
            dropped.append({"what": what(item), "why": f"needs {_systems_phrase(missing)}, not chosen"})
        else:
            kept.append(item)
    return kept


def _roles(roles, chosen: set, dropped: list) -> list:
    kept = []
    for r in roles:
        if any(s in chosen for s in r["systems"]):
            kept.append(r)
        elif len(r["systems"]) == 1:
            dropped.append({"what": f"role {r['id']}", "why": f"needs the system {r['systems'][0]}, not chosen"})
        else:
            dropped.append({"what": f"role {r['id']}", "why": f"needs one of {_systems_phrase(r['systems'])}, "
                                                              "none chosen"})
    return kept


def keep(t, systems: list[str]) -> Kept:
    """What a world of `systems` keeps of the template."""
    chosen, out = set(systems), Kept()
    out.tasks = _needing_all(t.tasks, "needs", chosen, lambda x: f"task {x['id']}", out.dropped)
    out.roles = _roles(t.roles, chosen, out.dropped)
    out.events = _needing_all(t.events, "needs", chosen, lambda x: f"event {x['id']}", out.dropped)
    out.links = _needing_all(t.data.get("links", []), "needs", chosen,
                             lambda x: f"link {x['from']} {x['event']} → {x['to']}", out.dropped)
    out.gates = _needing_all(t.data.get("gates", []), "needs", chosen,
                             lambda x: f"gate {x['tool']} on {x['part']}", out.dropped)
    return out
