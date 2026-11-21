"""JQL over issue records (`records.record`): each clause a test of one field, then `ORDER BY`.
Names are resolved through the context (the instance's projects, statuses, users, sprints, …), so an
unknown one is Jira's "does not exist" error; relative dates count from the context's world now."""
from dataclasses import dataclass, field
from typing import Callable

from .errors import JqlError
from .fields import FIELDS, SORTS, Field
from .parser import Bool, Clause, Func, Query
from . import values as V


@dataclass
class Context:
    now_ms: int
    user: str | None
    resolve: dict = field(default_factory=dict)        # kind -> (value -> canonical or None)
    sprint_states: dict = field(default_factory=dict)  # sprint id -> state
    category_of: Callable[[str], str] = lambda s: "new"
    status_order: list = field(default_factory=list)
    priority_rank: Callable[[str], int] = lambda p: 0


def _field(name: str) -> Field:
    f = FIELDS.get(name.lower())
    if f is None:
        raise JqlError(f"Field '{name}' does not exist or you do not have permission to view it.")
    return f


def _one(f: Field, value, ctx: Context) -> set:
    """The stored values a single JQL value stands for."""
    if isinstance(value, Func):
        return _func(f, value, ctx)
    if f.kind == "date":
        return {V.date(value, ctx.now_ms, f.name)}
    if f.kind in ("text", "rank"):
        return {str(value)}
    if f.kind == "resolution" and str(value).lower() == "unresolved":
        return {None}
    if f.kind in ("key", "parent"):
        return {str(value).upper()}
    found = ctx.resolve[f.kind](str(value)) if f.kind in ctx.resolve else None
    if found is None:
        raise V.missing(value, f.name)
    return {found}


def _func(f: Field, func: Func, ctx: Context) -> set:
    low = func.name.lower()
    if low == "currentuser" and f.kind == "user":
        return {ctx.user}
    if low in V.SPRINT_FUNCS and f.kind == "sprint":
        return {i for i, s in ctx.sprint_states.items() if s == V.SPRINT_FUNCS[low]}
    if low in V.DATE_FUNCS and f.kind == "date":
        return {V.date_func(func.name, func.args, ctx.now_ms)}
    raise V.no_function(func.name)


def _wanted(f: Field, value, ctx: Context) -> set:
    items = value if isinstance(value, tuple) else (value,)
    out = set()
    for item in items:
        out |= _one(f, item, ctx)
    return out


def _matches_text(text: str, query: str) -> bool:
    words = [w for w in "".join(c if c.isalnum() else " " for c in (text or "").lower()).split()]
    terms = [t.strip("*") for t in "".join(c if c.isalnum() or c == "*" else " " for c in query.lower()).split()]
    return all(any(w.startswith(t) for w in words) for t in terms if t)


def _ordered(f: Field, have, want: int, op: str, ctx: Context) -> bool:
    if have is None:
        return False
    if f.kind == "priority":
        have, want = -ctx.priority_rank(have), -ctx.priority_rank(want)
    return {">": have > want, ">=": have >= want, "<": have < want, "<=": have <= want}[op]


def _test(c: Clause, rec: dict, ctx: Context) -> bool:
    f = _field(c.field)
    have = f.get(rec, ctx)
    many = have if isinstance(have, list) else ([] if have is None else [have])
    if c.op in ("is", "is not"):
        return (not many) == (c.op == "is")
    if c.op in ("~", "!~"):
        if f.kind != "text":
            raise JqlError(f"The operator '{c.op}' is not supported by the '{c.field}' field.")
        return _matches_text(have, str(c.value)) == (c.op == "~")
    if f.kind == "text":
        raise JqlError(f"The operator '{c.op}' is not supported by the '{c.field}' field.")
    want = _wanted(f, c.value, ctx)
    if c.op in ("=", "in"):
        return bool(set(many) & want) or (None in want and not many)
    if c.op in ("!=", "not in"):
        return bool(many) and not set(many) & want
    if f.kind not in ("date", "priority"):
        raise JqlError(f"The operator '{c.op}' is not supported by the '{c.field}' field.")
    return any(_ordered(f, have, w, c.op, ctx) for w in want)


def _holds(node, rec: dict, ctx: Context) -> bool:
    if node is None:
        return True
    if isinstance(node, Clause):
        return _test(node, rec, ctx)
    if node.op == "not":
        return not _holds(node.parts[0], rec, ctx)
    results = (_holds(p, rec, ctx) for p in node.parts)
    return all(results) if node.op == "and" else any(results)


def _nulls_last(value, desc: bool) -> tuple:
    """A sort key that puts empty values last in either direction (equal Nones never compare)."""
    return (value is None) != desc, value


def _sort(recs: list, order: list, ctx: Context) -> list:
    out = list(recs)
    for name, direction in reversed(order):
        key = SORTS.get(name)
        if key is None:
            raise JqlError(f"Not able to sort using field '{name}'.")
        desc = direction == "desc"
        out.sort(key=lambda r, key=key, desc=desc: _nulls_last(key(r, ctx), desc), reverse=desc)
    return out


def evaluate(q: Query, recs: list, ctx: Context) -> list:
    """The records `q` selects, in its order (by id when it gives none); raises JqlError."""
    _check(q.where, ctx)
    found = [r for r in sorted(recs, key=lambda r: r["id"]) if _holds(q.where, r, ctx)]
    return _sort(found, q.order, ctx)


def _check(node, ctx: Context) -> None:
    """Every field and value is checked even when no issue reaches the clause (Jira validates first)."""
    if isinstance(node, Bool):
        for p in node.parts:
            _check(p, ctx)
    elif isinstance(node, Clause):
        f = _field(node.field)
        if node.value is not None and node.op not in ("~", "!~") and f.kind != "text":
            _wanted(f, node.value, ctx)
