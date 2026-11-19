"""Checks of a world's end state written as data (the expected end state format, end state only), evaluated
over every part's `inspect` docs into named checks for a benchmark's `score`.

Forms: a key `{part, key, field?, op, value | ref: {part, key, field}}`; a find `{part, find: "<glob>", where?,
count}`; `{all: [check]}` and `{any: [check]}`. Beside `check`, `safety: true` marks a safety check and
`measure: {label, unit?, better?, target?}` reports the field's number (or the count) as the check's measure.
Numbers are equal within 0.005, as gates compare them; `= null` matches a missing field.
"""
import json
import re
from fnmatch import fnmatchcase

from .benchmark import check

TOLERANCE = 0.005
_COUNT = re.compile(r"\s*(=|!=|<=|>=|<|>)?\s*(-?\d+)\s*")


class _Missing:
    def __repr__(self):
        return "missing"


MISSING = _Missing()


class _Bad(Exception):
    """A check that can't be evaluated as written."""


def evaluate(checks: list, docs: dict) -> list:
    """Each named check of `checks` over `docs` (`{part: {key: doc}}`), as `benchmark.check` results."""
    return [_named(item, docs) for item in checks]


def _named(item: dict, docs: dict) -> dict:
    try:
        passed, detail, value = _body(item.get("check") or {}, docs)
    except _Bad as e:
        passed, detail, value = False, f"can't evaluate: {e}", None
    return check(item["name"], passed, "" if passed else detail, bool(item.get("safety", False)),
                 _measure(item.get("measure"), value))


def _measure(spec, value):
    if not spec or not _number(value):
        return None
    return {**spec, "value": value}


def _body(body: dict, docs: dict) -> tuple:
    """(passed, detail, measured number or None)."""
    for form, run in (("all", _all), ("any", _any), ("find", _find), ("key", _key)):
        if form in body:
            return run(body, docs)
    raise _Bad("a check needs key, find, all or any")


def _all(body: dict, docs: dict) -> tuple:
    results = [_body(b, docs) for b in body["all"]]
    failed = [d for passed, d, _ in results if not passed]
    return not failed, "; ".join(failed), None


def _any(body: dict, docs: dict) -> tuple:
    results = [_body(b, docs) for b in body["any"]]
    failed = [d for passed, d, _ in results if not passed]
    return len(failed) < len(results), "; ".join(failed), None


def _key(body: dict, docs: dict) -> tuple:
    part, key, op = body["part"], body["key"], body.get("op", "exists")
    doc = docs.get(part, {}).get(key, MISSING)
    if doc is MISSING:
        return op == "absent", f"{part} has no {key}", None
    expected, missing_ref = _expected(body, docs)
    if missing_ref:
        return False, missing_ref, None
    actual = field_of(doc, body.get("field"))
    passed = compare(op, actual, expected)
    where = f"{part} {key}" + (f".{body['field']}" if body.get("field") else "")
    wanted = op if op in ("exists", "absent") else f"{op} {_show(expected)}"
    return passed, f"{where} is {_show(actual)}, expected {wanted}", actual if _number(actual) else None


def _expected(body: dict, docs: dict) -> tuple:
    """The value compared with, and a detail when the `ref` doc is missing."""
    ref = body.get("ref")
    if ref is None:
        return body.get("value"), None
    doc = docs.get(ref["part"], {}).get(ref["key"], MISSING)
    if doc is MISSING:
        return None, f"{ref['part']} has no {ref['key']}"
    return field_of(doc, ref.get("field")), None


def _find(body: dict, docs: dict) -> tuple:
    part, glob, count = body["part"], body["find"], body.get("count", ">= 1")
    where = body.get("where") or {}
    n = sum(1 for k, doc in docs.get(part, {}).items() if fnmatchcase(k, glob) and _matches(doc, where))
    op, target = _count(count)
    return compare(op, n, target), f"{n} of {part} {glob} match, expected {count}", n


def _count(count) -> tuple:
    if _number(count):
        return "=", count
    m = _COUNT.fullmatch(count) if isinstance(count, str) else None
    if m is None:
        raise _Bad(f"count {count} is not '<op> <n>'")
    return m.group(1) or "=", int(m.group(2))


def _matches(doc, where: dict) -> bool:
    for field, cond in where.items():
        op, value = "=", cond
        if isinstance(cond, dict) and len(cond) == 1 and next(iter(cond)) in _OPS:
            op, value = next(iter(cond.items()))
        if not compare(op, field_of(doc, field), value):
            return False
    return True


def field_of(doc, dotted):
    """The value at the dotted path in `doc` (list items by index), `MISSING` when there is none; the doc itself
    without a path."""
    if not dotted:
        return doc
    for k in str(dotted).split("."):
        if isinstance(doc, dict) and k in doc:
            doc = doc[k]
        elif isinstance(doc, list) and k.isdigit() and int(k) < len(doc):
            doc = doc[int(k)]
        else:
            return MISSING
    return doc


def _number(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _equal(a, b) -> bool:
    if b is None:
        return a is MISSING or a is None
    if _number(a) and _number(b):
        return abs(a - b) < TOLERANCE
    return a is not MISSING and a == b


def _ordered(test):
    return lambda a, b: _number(a) and _number(b) and test(a, b)


def _contains(a, b) -> bool:
    if isinstance(a, list):
        return any(_equal(x, b) for x in a)
    return isinstance(a, str) and isinstance(b, str) and b in a


_OPS = {
    "=": _equal,
    "!=": lambda a, b: not _equal(a, b),
    "<": _ordered(lambda a, b: a < b and not _equal(a, b)),
    "<=": _ordered(lambda a, b: a < b or _equal(a, b)),
    ">": _ordered(lambda a, b: a > b and not _equal(a, b)),
    ">=": _ordered(lambda a, b: a > b or _equal(a, b)),
    "in": lambda a, b: isinstance(b, list) and any(_equal(a, x) for x in b),
    "contains": _contains,
    "exists": lambda a, b: a is not MISSING and a is not None,
    "absent": lambda a, b: a is MISSING or a is None,
}


def compare(op: str, actual, expected) -> bool:
    """`actual op expected` with the check format's operators."""
    if op not in _OPS:
        raise _Bad(f"unknown op {op}")
    return _OPS[op](actual, expected)


def _show(v) -> str:
    if v is MISSING:
        return "missing"
    return v if isinstance(v, str) else json.dumps(v)
