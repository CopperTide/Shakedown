"""The rule format (`shakedown-people/1`): one role's rules as data, checked by `rules.schema.json` and the
cross-checks the schema cannot make, and loaded into the rules the engine runs. The format is described in
`rules.schema.json` and the package README."""
import json
from dataclasses import dataclass
from functools import cache
from pathlib import Path

import jsonschema

from . import values

FORMAT = "shakedown-people/1"
SCHEMA = Path(__file__).resolve().parent / "rules.schema.json"
MIN_MS = 60_000
ME_FIELDS = ("/reviewer", "/assignee", "/user")


class RulesError(Exception):
    """A rules doc that doesn't fit: where (`rules[0].after`) and why."""

    def __init__(self, path: str, message: str):
        super().__init__(f"{path or '(the rules)'}: {message}")
        self.path, self.message = path, message


@dataclass(frozen=True)
class Notice:
    system: str
    kind: str
    to: tuple = ()  # pointers, one of which must be (or list) the person's login; empty: everyone of the role
    where: dict | None = None


@dataclass(frozen=True)
class Every:
    at: tuple = ()  # minutes of the world's day (UTC)
    minutes: int = 0


@dataclass(frozen=True)
class Act:
    system: str
    action: str
    args: dict


@dataclass(frozen=True)
class Branch:
    when: dict | None  # None: else
    acts: tuple


@dataclass(frozen=True)
class Refresh:
    system: str
    kind: str
    match: dict
    set: dict


@dataclass(frozen=True)
class Rule:
    id: str
    notice: Notice | None
    every: Every | None
    once: int | None  # world ms of the clock
    who: str
    after: tuple  # (lo, hi) world ms
    limit_per_day: int | None
    refresh: tuple
    branches: tuple


@dataclass(frozen=True)
class RoleRules:
    role: str
    lines: dict
    rules: tuple


@cache
def _validator() -> jsonschema.Draft202012Validator:
    return jsonschema.Draft202012Validator(json.loads(SCHEMA.read_text()))


def path_of(parts) -> str:
    out = ""
    for k in parts:
        out += f"[{k}]" if isinstance(k, int) else (f".{k}" if out else str(k))
    return out


def load(doc) -> RoleRules | RulesError:
    """`doc` as a role's rules, or why it doesn't fit."""
    error = jsonschema.exceptions.best_match(_validator().iter_errors(doc))
    if error is not None:
        return RulesError(path_of(error.absolute_path), error.message)
    try:
        _cross_check(doc)
    except RulesError as e:
        return e
    return RoleRules(doc["role"], {k: tuple(v) for k, v in doc.get("lines", {}).items()},
                     tuple(_rule(r) for r in doc["rules"]))


def _cross_check(doc: dict) -> None:
    seen = set()
    for i, r in enumerate(doc["rules"]):
        at = f"rules[{i}]"
        if r["id"] in seen:
            raise RulesError(f"{at}.id", f"{r['id']} is used twice")
        seen.add(r["id"])
        lo, hi = _minutes(r)
        if lo > hi:
            raise RulesError(f"{at}.after.minutes", f"{lo} is after {hi}")
        if "refresh" in r and "notice" not in r:
            raise RulesError(f"{at}.refresh", "only a rule that answers a notice can be refreshed")
        _check_branches(r, at, set(doc.get("lines", {})))


def _check_branches(r: dict, at: str, lines: set) -> None:
    branches = [(f"{at}.do[{j}]", b) for j, b in enumerate(r["do"])] if "do" in r else [(at, r)]
    for j, (where, b) in enumerate(branches):
        if "else" in b and j != len(branches) - 1:
            raise RulesError(where, "else must be the last branch")
        acts = b["act"] if isinstance(b["act"], list) else [b["act"]]
        for k, act in enumerate(acts):
            name = f"{where}.act[{k}]" if isinstance(b["act"], list) else f"{where}.act"
            for path, text in values.strings(act.get("args", {}), f"{name}.args"):
                missing = [p for p in values.picks(text) if p not in lines]
                if missing:
                    raise RulesError(path, f"no lines named {missing[0]}")


def _minutes(r: dict) -> tuple:
    m = r.get("after", {}).get("minutes", 0)
    return (m[0], m[1]) if isinstance(m, list) else (m, m)


def _rule(r: dict) -> Rule:
    lo, hi = _minutes(r)
    return Rule(r["id"], _notice(r.get("notice")), _every(r.get("every")),
                r["once"]["minute"] * MIN_MS if "once" in r else None, r.get("who", "all"),
                (round(lo * MIN_MS), round(hi * MIN_MS)), r.get("limit", {}).get("per_day"),
                tuple(_refresh(x) for x in r.get("refresh", [])), _branches(r))


def _refresh(x: dict) -> Refresh:
    return Refresh(x["notice"]["system"], x["notice"]["kind"], x["match"], x["set"])


def _notice(on: dict | None) -> Notice | None:
    if on is None:
        return None
    to = on.get("to")
    return Notice(on["system"], on["kind"], ME_FIELDS if to == "me" else (to,) if to else (), on.get("where"))


def _every(every: dict | None) -> Every | None:
    if every is None:
        return None
    if "minutes" in every:
        return Every(minutes=every["minutes"])
    at = every["at"] if isinstance(every["at"], list) else [every["at"]]
    return Every(at=tuple(sorted({int(t[:2]) * 60 + int(t[3:]) for t in at})))


def _branches(r: dict) -> tuple:
    given = r["do"] if "do" in r else [{"else": True, "act": r["act"]}]
    out = []
    for b in given:
        acts = b["act"] if isinstance(b["act"], list) else [b["act"]]
        out.append(Branch(b.get("if"), tuple(Act(a["system"], a["action"], a.get("args", {})) for a in acts)))
    return tuple(out)
