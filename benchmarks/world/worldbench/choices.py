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


def _object(raw: dict, key: str, out: list, path: str | None = None) -> dict:
    """`raw[key]` when it is an object (`{}` when absent); a problem at `path` (default `key`) when it is not."""
    v = raw.get(key) or {}
    if not isinstance(v, dict):
        out.append(problem(path or key, f"{path or key} must be an object"))
        return {}
    return v


def _list(raw: dict, key: str, out: list, path: str) -> list:
    v = raw.get(key) or []
    if not isinstance(v, list):
        out.append(problem(path, f"{path} must be a list"))
        return []
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
    ok = [isinstance(sid, str) and sid in known for sid in asked]
    out += [problem(f"systems[{i}]", f"no system {sid}") for i, sid in enumerate(asked) if not ok[i]]
    return select.closure(t, [sid for sid, fine in zip(asked, ok) if fine])


def _options(t, raw: dict, out: list) -> dict:
    filled = {s["id"]: {o["name"]: o["default"] for o in s.get("options", [])} for s in t.systems}
    specs = {s["id"]: {o["name"]: o for o in s.get("options", [])} for s in t.systems}
    asked = _object(raw, "options", out)
    for sid in asked:
        if sid not in specs:
            out.append(problem(f"options.{sid}", f"no system {sid}"))
            continue
        for name, v in _object(asked, sid, out, f"options.{sid}").items():
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
    for rid, n in _object(raw, "counts", out, "people.counts").items():
        if rid not in roles:
            out.append(problem(f"people.counts.{rid}", f"no role {rid}"))
        elif not _whole(n) or n < roles[rid].get("min", 0):
            out.append(problem(f"people.counts.{rid}", f"{roles[rid]['label']} need at least {roles[rid].get('min', 0)}"))
        else:
            counts[rid] = n
    added = [a for i, a in enumerate(_list(raw, "added", out, "people.added")) if _added_ok(a, i, roles, out)]
    return {"counts": counts, "edits": dict(_object(raw, "edits", out, "people.edits")), "added": added,
            "removed": _removed(raw, out)}


def _removed(raw: dict, out: list) -> list:
    removed = _list(raw, "removed", out, "people.removed")
    out += [problem(f"people.removed[{i}]", "a person is removed by id") for i, pid in enumerate(removed)
            if not isinstance(pid, str)]
    return [pid for pid in removed if isinstance(pid, str)]


def _added_ok(a, i: int, roles: dict, out: list) -> bool:
    path = f"people.added[{i}]"
    if not isinstance(a, dict) or not isinstance(a.get("name"), str) or not a["name"].strip():
        out.append(problem(f"{path}.name", "a person needs a name"))
        return False
    if not isinstance(a.get("role"), str) or a["role"] not in roles:
        out.append(problem(f"{path}.role", f"no role {a.get('role')}"))
        return False
    return True


def _events(t, raw: dict, out: list) -> dict:
    asked = _object(raw, "events", out)
    known = {e["id"] for e in t.events}
    for eid in asked:
        if eid not in known:
            out.append(problem(f"events.{eid}", f"no event {eid}"))
    return {e["id"]: _event(e, _object(asked, e["id"], out, f"events.{e['id']}"), out) for e in t.events}


def _event(e: dict, asked: dict, out: list) -> dict:
    on = asked.get("on", e.get("default", False) or e.get("fixed", False))
    if e.get("fixed") and not on:
        out.append(problem(f"events.{e['id']}.on", f"{e['label']} is part of the template's data and can't be "
                                                   "turned off"))
        on = True
    params = {p["name"]: p["default"] for p in e.get("params", [])}
    specs = {p["name"]: p for p in e.get("params", [])}
    for name, v in _object(asked, "params", out, f"events.{e['id']}.params").items():
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
