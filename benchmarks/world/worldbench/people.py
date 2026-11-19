"""The world's simulated people: drawn from the template's name pools by a seeded generator, with the role
fields the template gives, then the user's removals, renames and additions, each with a unique login."""
import copy
import math
import re
from random import Random

from .choices import problem
from .errors import TemplateError
from .expand import Context, expand
from .slug import ascii_of

_NO_CONTEXT = Context(company={}, size=0, options={}, people=[], params={})


def role_count(role: dict, c) -> int:
    """How many people of `role`: the count set by hand, else its share of the size (rounded half up) or its
    fixed count, never under its `min`."""
    if role["id"] in c.counts:
        return c.counts[role["id"]]
    rule = role["count"]
    n = math.floor(rule["share"] * c.size + 0.5) if "share" in rule else rule["fixed"]
    return max(role.get("min", 0), n)


def generate_people(t, c, chosen_roles: list[dict]) -> list[dict]:
    """People of `chosen_roles` in role order (ids `p-001…`), then the people the user added."""
    plan = [(role, role_count(role, c)) for role in chosen_roles]
    total = sum(n for _, n in plan)
    rng = Random(f"{c.seed}:{t.id}:people")
    names = _draw_names(rng, *_pools(t), total) if total else []
    people = []
    for role, count in plan:
        for i in range(count):
            people.append({"id": _id(len(people) + 1), "name": names[len(people)], "role": role["id"],
                           **_fields(role, i, rng)})
    added = [{"id": _id(total + j + 1), "name": a["name"].strip(), "role": a["role"], "added": True}
             for j, a in enumerate(c.added)]
    return people + added


def _id(n: int) -> str:
    return f"p-{n:03d}"


def _pools(t) -> tuple[list, list]:
    names = expand(t.data.get("names", {"first": [], "last": []}), _NO_CONTEXT, t.dir, [], "names")
    ok = isinstance(names, dict) and all(isinstance(names.get(k), list) and names[k] for k in ("first", "last"))
    if not ok:
        raise TemplateError("names", "names need a first and a last list")
    return list(dict.fromkeys(names["first"])), list(dict.fromkeys(names["last"]))


def _index(rng: Random, n: int) -> int:
    """A draw in 0..n-1 from `Random.random()` alone, whose sequence Python keeps across versions."""
    return min(int(rng.random() * n), n - 1)


def _draw_names(rng: Random, first: list, last: list, total: int) -> list[str]:
    """`total` full names, none twice (a partial Fisher–Yates shuffle of every first-last pair)."""
    pairs = len(first) * len(last)
    if total > pairs:
        raise TemplateError("names", f"the name pools hold {pairs} full names; this world needs {total}")
    pool, out = list(range(pairs)), []
    for k in range(total):
        j = k + _index(rng, pairs - k)
        pool[k], pool[j] = pool[j], pool[k]
        out.append(f"{first[pool[k] // len(last)]} {last[pool[k] % len(last)]}")
    return out


def _fields(role: dict, i: int, rng: Random) -> dict:
    out = {}
    for name, rule in role.get("fields", {}).items():
        values = rule.get("cycle") or rule["pick"]
        out[name] = copy.deepcopy(values[i % len(values)] if "cycle" in rule else values[_index(rng, len(values))])
    return out


def apply_edits(people: list[dict], c) -> tuple[list[dict], list]:
    """`people` without the removed, with the renames, each given a unique login; problems for ids not in the
    world and for edits that don't fit."""
    ids = {p["id"] for p in people}
    out = [problem(f"people.removed.{pid}", f"no person {pid} in this world") for pid in c.removed if pid not in ids]
    names = {}
    for pid, edit in c.edits.items():
        name = _edited_name(pid, edit, ids, out)
        if name is not None:
            names[pid] = name
    kept = [{**p, "name": names.get(p["id"], p["name"])} for p in people if p["id"] not in set(c.removed)]
    return _with_logins(kept), out


def _edited_name(pid: str, edit, ids: set, out: list) -> str | None:
    path = f"people.edits.{pid}"
    if pid not in ids:
        out.append(problem(path, f"no person {pid} in this world"))
    elif not isinstance(edit, dict) or set(edit) - {"name"}:
        out.append(problem(path, "only a person's name can be edited"))
    elif not isinstance(edit.get("name"), str) or not edit["name"].strip():
        out.append(problem(f"{path}.name", "a person needs a name"))
    else:
        return edit["name"].strip()
    return None


def _login_base(name: str) -> str:
    words = re.findall(r"[a-z0-9]+", ascii_of(name).lower())
    if not words:
        return "user"
    return words[0] + (words[-1][0] if len(words) > 1 else "")


def _with_logins(people: list[dict]) -> list[dict]:
    seen, out = set(), []
    for p in people:
        base = login = _login_base(p["name"])
        n = 2
        while login in seen:
            login, n = f"{base}{n}", n + 1
        seen.add(login)
        out.append({**p, "login": login})
    return out
