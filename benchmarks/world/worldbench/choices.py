"""What the user chose (the Choices JSON), checked against a template and filled with its defaults."""
from dataclasses import dataclass

from . import select


def problem(path: str, message: str) -> dict:
    return {"path": path, "message": message}


def _whole(v) -> bool:
    return isinstance(v, int) and not isinstance(v, bool)


@dataclass
class Choices:
    """Choices with the template's defaults filled in. `systems` is closed under `requires`; `options` holds
    every system's; `events` every event's `{on, params}`."""
    seed: int
    company: str
    size: int
    systems: list
    options: dict
    counts: dict
    edits: dict
    added: list
    removed: list
    events: dict

    def to_json(self) -> dict:
        return {"seed": self.seed, "company": {"name": self.company}, "size": self.size, "systems": self.systems,
                "options": self.options, "events": self.events,
                "people": {"counts": self.counts, "edits": self.edits, "added": self.added,
                           "removed": self.removed}}

    @staticmethod
    def parse(t, raw) -> tuple["Choices", list]:
        """The choices `raw` with defaults filled, and the problems found (`[{path, message}]`)."""
        out: list = []
        if not isinstance(raw, dict):
            out.append(problem("", "choices must be an object"))
            raw = {}
        people = _people(t, _object(raw, "people", out), out)
        c = Choices(seed=_seed(raw, out), company=_company(t, raw, out), size=_size(t, raw, out),
                    systems=_systems(t, raw, out), options=_options(t, raw, out), **people,
                    events=_events(t, raw, out))
        return c, out


def _object(raw: dict, key: str, out: list) -> dict:
    v = raw.get(key) or {}
    if not isinstance(v, dict):
        out.append(problem(key, f"{key} must be an object"))
        return {}
    return v


def _seed(raw: dict, out: list) -> int:
    seed = raw.get("seed", 1)
    if not _whole(seed):
        out.append(problem("seed", "seed must be a whole number"))
        return 1
    return seed


def _company(t, raw: dict, out: list) -> str:
    name = _object(raw, "company", out).get("name", t.data["company"]["name"])
    if not isinstance(name, str) or not name.strip():
        out.append(problem("company.name", "Give the company a name"))
        return t.data["company"]["name"]
    return name.strip()


def _size(t, raw: dict, out: list) -> int:
    size = t.data["size"]
    v = raw.get("size", size["default"])
    if not _whole(v) or not size["min"] <= v <= size["max"]:
        out.append(problem("size", f"Size is between {size['min']} and {size['max']}"))
        return size["default"]
    return v


def _systems(t, raw: dict, out: list) -> list:
    asked = raw.get("systems")
    if asked is None:
        return select.closure(t, select.default_systems(t))
    known = {s["id"] for s in t.systems}
    if not isinstance(asked, list):
        out.append(problem("systems", "systems must be a list of system ids"))
        asked = []
    for i, sid in enumerate(asked):
        if sid not in known:
            out.append(problem(f"systems[{i}]", f"no system {sid}"))
    return select.closure(t, [s for s in asked if s in known])


def _options(t, raw: dict, out: list) -> dict:
    filled = {s["id"]: {o["name"]: o["default"] for o in s.get("options", [])} for s in t.systems}
    specs = {s["id"]: {o["name"]: o for o in s.get("options", [])} for s in t.systems}
    for sid, values in _object(raw, "options", out).items():
        if sid not in specs:
            out.append(problem(f"options.{sid}", f"no system {sid}"))
            continue
        for name, v in (values or {}).items():
            _option(specs[sid], sid, name, v, filled, out)
    return filled


def _option(specs: dict, sid: str, name: str, v, filled: dict, out: list) -> None:
    path = f"options.{sid}.{name}"
    if name not in specs:
        out.append(problem(path, f"{sid} has no option {name}"))
    elif v not in specs[name]["choices"]:
        out.append(problem(path, f"{v} is not one of {', '.join(specs[name]['choices'])}"))
    else:
        filled[sid][name] = v


def _people(t, raw: dict, out: list) -> dict:
    roles = {r["id"]: r for r in t.roles}
    counts = {}
    for rid, n in (raw.get("counts") or {}).items():
        if rid not in roles:
            out.append(problem(f"people.counts.{rid}", f"no role {rid}"))
        elif not _whole(n) or n < roles[rid].get("min", 0):
            out.append(problem(f"people.counts.{rid}", f"{roles[rid]['label']} need at least {roles[rid].get('min', 0)}"))
        else:
            counts[rid] = n
    added = [a for i, a in enumerate(raw.get("added") or []) if _added_ok(a, i, roles, out)]
    return {"counts": counts, "edits": dict(raw.get("edits") or {}), "added": added,
            "removed": list(raw.get("removed") or [])}


def _added_ok(a, i: int, roles: dict, out: list) -> bool:
    path = f"people.added[{i}]"
    if not isinstance(a, dict) or not isinstance(a.get("name"), str) or not a["name"].strip():
        out.append(problem(f"{path}.name", "a person needs a name"))
        return False
    if a.get("role") not in roles:
        out.append(problem(f"{path}.role", f"no role {a.get('role')}"))
        return False
    return True


def _events(t, raw: dict, out: list) -> dict:
    asked = _object(raw, "events", out)
    known = {e["id"] for e in t.events}
    for eid in asked:
        if eid not in known:
            out.append(problem(f"events.{eid}", f"no event {eid}"))
    return {e["id"]: _event(e, asked.get(e["id"]) or {}, out) for e in t.events}


def _event(e: dict, asked: dict, out: list) -> dict:
    on = asked.get("on", e.get("default", False))
    params = {p["name"]: p["default"] for p in e.get("params", [])}
    specs = {p["name"]: p for p in e.get("params", [])}
    for name, v in (asked.get("params") or {}).items():
        path = f"events.{e['id']}.params.{name}"
        if name not in specs:
            out.append(problem(path, f"{e['id']} has no param {name}"))
        elif (msg := param_problem(specs[name], v)) is not None:
            out.append(problem(path, msg))
        else:
            params[name] = v
    return {"on": bool(on), "params": params}


def param_problem(p: dict, v) -> str | None:
    """Why `v` does not fit the param `p` (as a scenario's params are checked), or None."""
    if p["kind"] == "choice":
        return None if v in p["choices"] else f"{v} is not one of {', '.join(map(str, p['choices']))}"
    if not _whole(v):
        return f"{p['name']} must be a whole number"
    low, high = p.get("min"), p.get("max")
    if (low is None or v >= low) and (high is None or v <= high):
        return None
    if low is not None and high is not None:
        return f"{p['name']} is between {low} and {high}"
    return f"{p['name']} is at least {low}" if low is not None else f"{p['name']} is at most {high}"
