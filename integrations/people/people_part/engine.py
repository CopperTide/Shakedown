"""What people mean to do and when they do it, kept in the part's state so a save holds it: a notice queues the
acts of the rules it sets off (after each one's seeded delay), schedules queue theirs at their slots, and running
the clock fires every act that comes due, in time order, as a `<system>_act` event."""
import copy

from . import schedule, values


def begin(world, state: dict, seed, epoch_ms: int, clock_ms: int) -> None:
    """A fresh state: nothing pending, each scheduled rule at its first slot."""
    state.update(seed=seed, epoch_ms=epoch_ms, seq=0, pending=[], slots={}, counts={}, draws={}, fired={},
                 acted={}, skipped={}, log=[])
    for role in world.roles():
        for rule in world.rules[role].rules:
            slot = schedule.first_slot(rule, epoch_ms, clock_ms)
            if slot is not None:
                state["slots"][f"{role}/{rule.id}"] = slot


def notice(world, state: dict, system: str, kind: str, data: dict) -> tuple:
    """A notice at the state's clock: it refreshes waiting acts that read it and queues the rules it sets off.
    The ids of the people it touched, in the world's order, and how many acts it queued."""
    touched = _refresh(world, state, system, kind, data)
    queued = []
    for role in world.roles():
        for rule in world.rules[role].rules:
            on = rule.notice
            if on and (on.system, on.kind) == (system, kind) and values.holds(on.where, data):
                queued += _queue(world, state, role, rule, state["clock_ms"], data)
    touched |= {item["id"] for item in queued}
    return [p["id"] for p in world.people if p["id"] in touched], len(queued)


def _refresh(world, state: dict, system: str, kind: str, data: dict) -> set:
    touched = set()
    for item in state["pending"]:
        rule = world.rule(item["role"], item["rule"])
        for r in rule.refresh if rule else ():
            if (r.system, r.kind) == (system, kind) and _matched(r.match, item["data"], data):
                for mine, theirs in r.set.items():
                    values.put(item["data"], mine, values.get(data, theirs))
                touched.add(item["id"])
    return touched


def _matched(match: dict, mine: dict, theirs: dict) -> bool:
    return all(values.get(mine, a) is not None and values.matches(values.get(mine, a), values.get(theirs, b))
               for a, b in match.items())


def _reached(world, role: str, rule, data: dict) -> list:
    people = world.of_role(role)
    if rule.notice and rule.notice.to:
        people = [p for p in people if any(values.matches(p["login"], values.get(data, ptr)) for ptr in rule.notice.to)]
    return people


def _queue(world, state: dict, role: str, rule, at_ms: int, data: dict) -> list:
    """The acts one trigger of `rule` queues: for each person it reaches (one of them, seeded, for `who: one`)."""
    people = _reached(world, role, rule, data)
    if rule.who == "one" and people:
        key = f"{role}/{rule.id}"
        k = state["draws"].get(key, 0)
        state["draws"][key] = k + 1
        people = [people[schedule.draw(schedule.rng(state["seed"], role, rule.id, k, "who"), len(people))]]
    out = []
    for p in people:
        key = f"{p['id']}/{rule.id}"
        n = state["counts"].get(key, 0)
        state["counts"][key] = n + 1
        item = {"seq": state["seq"], "id": p["id"], "role": role, "rule": rule.id, "n": n,
                "due_ms": schedule.due(state["seed"], p, rule, at_ms, n), "data": copy.deepcopy(data)}
        state["seq"] += 1
        state["pending"].append(item)
        out.append(item)
    return out


def run_to(world, state: dict, clock_ms: int) -> list:
    """Runs the clock to `clock_ms`: each slot and act that comes due, earliest first (a slot before an act at
    the same time, then in the order they were queued). The act events on the way."""
    events = []
    while (found := _next(state, clock_ms)) is not None:
        kind, key = found
        if kind == "slot":
            _slot(world, state, key)
        else:
            events += _fire(world, state, key)
    state["clock_ms"] = max(state["clock_ms"], clock_ms)
    return events


def _next(state: dict, clock_ms: int):
    slots = [((at, 0, key), ("slot", key)) for key, at in state["slots"].items() if at <= clock_ms]
    acts = [((p["due_ms"], 1, p["seq"]), ("act", p)) for p in state["pending"] if p["due_ms"] <= clock_ms]
    return min(slots + acts, key=lambda x: x[0])[1] if slots or acts else None


def _slot(world, state: dict, key: str) -> None:
    role, rule_id = key.split("/", 1)
    rule, at = world.rule(role, rule_id), state["slots"].pop(key)
    if rule is None:
        return
    _queue(world, state, role, rule, at, {})
    after = schedule.next_slot(rule, state["epoch_ms"], at)
    if after is not None:
        state["slots"][key] = after


def _fire(world, state: dict, item: dict) -> list:
    state["pending"].remove(item)
    person, rule = world.by_id(item["id"]), world.rule(item["role"], item["rule"])
    if person is None or rule is None:
        return []
    branch = next((b for b in rule.branches if b.when is None or values.holds(b.when, item["data"])), None)
    if branch is None:
        return []
    if not _within_limit(state, rule, item):
        _count(state, "skipped", person["id"], len(branch.acts))
        return []
    lines = world.rules[item["role"]].lines
    r = schedule.rng(state["seed"], person["id"], rule.id, item["n"], "lines")

    def draw(name: str) -> str:
        return lines[name][schedule.draw(r, len(lines[name]))]

    return [e for act in branch.acts if (e := _act(world, state, person, item, act, draw)) is not None]


def _within_limit(state: dict, rule, item: dict) -> bool:
    if rule.limit_per_day is None:
        return True
    key = f"{item['id']}/{rule.id}/{schedule.day_of(state['epoch_ms'], item['due_ms'])}"
    if state["fired"].get(key, 0) >= rule.limit_per_day:
        return False
    state["fired"][key] = state["fired"].get(key, 0) + 1
    return True


def _act(world, state: dict, person: dict, item: dict, act, draw) -> dict | None:
    if world.systems is not None and act.system not in world.systems:
        _count(state, "skipped", person["id"])
        return None
    at = state["epoch_ms"] + item["due_ms"]
    args = values.render(act.args, item["data"], person, draw)
    state["log"].append({"at_ms": at, "person": person["login"], "id": person["id"], "rule": item["rule"],
                         "system": act.system, "action": act.action})
    _count(state, "acted", person["id"])
    return {"kind": f"{act.system}_act", "text": f"{person['name']} ({person['role']}): {act.action} on {act.system}",
            "data": {"person": person["login"], "action": act.action, "args": args, "at_ms": at,
                     "id": person["id"], "rule": item["rule"]}}


def _count(state: dict, name: str, person_id: str, n: int = 1) -> None:
    state[name][person_id] = state[name].get(person_id, 0) + n
