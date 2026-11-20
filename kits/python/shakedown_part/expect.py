"""Checks of a world's end state written as data (the expected end state format, end state only), evaluated
over every part's `inspect` docs into named checks for a benchmark's `score`.

Forms: a key `{part, key, field?, op, value | ref: {part, key, field}}`; a find `{part, find: "<glob>", where?,
count}`; `{all: [check]}` and `{any: [check]}`. Beside `check`, `safety: true` marks a safety check and
`measure: {label, unit?, better?, target?}` reports the field's number (or the count) as the check's measure.
Numbers are equal within 0.005, as gates compare them; `= null` matches a missing field. An object in `where` is
`{op: value}` (compare with an object as `{"=": {…}}`). A check that can't be evaluated as written (a part the world
doesn't have, an empty `all`/`any`, a `ref` field that is missing, an unknown op or field) fails with
`can't evaluate: …`, alone: the other checks are still scored.
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
    """Each named check of `checks` over `docs` (`{part: {key: doc}}`), as `benchmark.check` results; a check that
    can't be evaluated as written fails alone, with `can't evaluate: …`."""
    return [_named(item, docs) for item in checks]


def _named(item, docs: dict) -> dict:
    if not isinstance(item, dict):
        return check("?", False, "can't evaluate: a check is an object with a name and a check")
    try:
        passed, detail, value = _body(item.get("check") or {}, docs)
        measure = _measure(item.get("measure"), value)
    except _Bad as e:
        passed, detail, measure = False, f"can't evaluate: {e}", None
    except (KeyError, TypeError, AttributeError, ValueError) as e:
        passed, detail, measure = False, f"can't evaluate: {type(e).__name__} {e}", None
    return check(str(item.get("name", "?")), passed, "" if passed else detail, bool(item.get("safety", False)),
                 measure)


def _measure(spec, value):
    if not spec or not _number(value):
        return None
    if not isinstance(spec, dict):
        raise _Bad("a measure is an object {label, unit?, better?, target?}")
    return {**spec, "value": value}


_FIELDS = {"key": {"part", "key", "field", "op", "value", "ref"}, "find": {"part", "find", "where", "count"},
           "all": {"all"}, "any": {"any"}}


def _body(body, docs: dict) -> tuple:
    """(passed, detail, measured number or None)."""
    if not isinstance(body, dict):
        raise _Bad("a check is an object")
    for form, run in (("all", _all), ("any", _any), ("find", _find), ("key", _key)):
        if form in body:
            extra = sorted(set(body) - _FIELDS[form])
            if extra:
                raise _Bad(f"a {form} check has no field {extra[0]}")
            return run(body, docs)
    raise _Bad("a check needs key, find, all or any")


def _each(body: dict, form: str, docs: dict) -> list:
    items = body[form]
    if not isinstance(items, list) or not items:
        raise _Bad(f"{form} needs a list of checks")
    return [_body(b, docs) for b in items]


def _all(body: dict, docs: dict) -> tuple:
    failed = [d for passed, d, _ in _each(body, "all", docs) if not passed]
    return not failed, "; ".join(failed), None


def _any(body: dict, docs: dict) -> tuple:
    results = _each(body, "any", docs)
    failed = [d for passed, d, _ in results if not passed]
    return len(failed) < len(results), "; ".join(failed), None


def _text(body: dict, name: str, form: str) -> str:
    v = body.get(name)
    if not isinstance(v, str):
        raise _Bad(f"a {form} check needs {name} as text")
    return v


def _part(docs: dict, part: str) -> dict:
    """The docs of `part`; a part the world doesn't have can't be checked (a typo, or a system not chosen)."""
    if part not in docs:
        raise _Bad(f"no part {part} in this world")
    return docs[part] or {}


def _op(name) -> str:
    if not isinstance(name, str) or name not in _OPS:
        raise _Bad(f"unknown op {name}")
    return name


def _key(body: dict, docs: dict) -> tuple:
    part, key, op = _text(body, "part", "key"), _text(body, "key", "key"), _op(body.get("op", "exists"))
    if op not in ("exists", "absent") and "value" not in body and "ref" not in body:
        raise _Bad(f"{op} needs a value or a ref")
    expected, missing_ref = _expected(body, docs)
    doc = _part(docs, part).get(key, MISSING)
    if doc is MISSING:
        return op == "absent", f"{part} has no {key}", None
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
    if not isinstance(ref, dict):
        raise _Bad("ref is an object {part, key, field?}")
    part, key = _text(ref, "part", "ref"), _text(ref, "key", "ref")
    doc = _part(docs, part).get(key, MISSING)
    if doc is MISSING:
        return None, f"{part} has no {key}"
    value = field_of(doc, ref.get("field"))
    if value is MISSING:
        raise _Bad(f"the ref {part} {key}.{ref['field']} is missing")
    return value, None


def _find(body: dict, docs: dict) -> tuple:
    part, glob, count = _text(body, "part", "find"), _text(body, "find", "find"), body.get("count", ">= 1")
    where = _conditions(body.get("where") or {})
    op, target = _count(count)
    n = sum(1 for k, doc in _part(docs, part).items() if fnmatchcase(k, glob) and _matches(doc, where))
    return compare(op, n, target), f"{n} of {part} {glob} match, expected {count}", n


def _count(count) -> tuple:
    if _number(count):
        return "=", count
    m = _COUNT.fullmatch(count) if isinstance(count, str) else None
    if m is None:
        raise _Bad(f"count {count} is not '<op> <n>'")
    return m.group(1) or "=", int(m.group(2))


def _conditions(where) -> list:
    """`where` as `[(field, op, value)]`: a plain value is `=`, an object is `{op: value}` (compare with an object
    as `{"=": {…}}`)."""
    if not isinstance(where, dict):
        raise _Bad("where is an object {field: value | {op: value}}")
    out = []
    for field, cond in where.items():
        op, value = "=", cond
        if isinstance(cond, dict):
            if len(cond) != 1:
                raise _Bad(f"where {field} needs one op, as {{op: value}}")
            op, value = next(iter(cond.items()))
        out.append((field, _op(op), value))
    return out


def _matches(doc, where: list) -> bool:
    return all(compare(op, field_of(doc, field), value) for field, op, value in where)


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
