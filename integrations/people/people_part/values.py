"""What rules read and write in a notice's data: JSON pointers, conditions, and the args of an act filled from
the notice, the person and the role's lines."""
import re

FILL = re.compile(r"\{(/[^{}]*|pick:[a-z][a-z0-9_-]*|person:[a-z_]+)\}")


def _tokens(ptr: str) -> list:
    return [t.replace("~1", "/").replace("~0", "~") for t in ptr[1:].split("/")] if ptr != "/" else []


def get(data, ptr: str, default=None):
    """The value at `ptr` (RFC 6901) in `data`, else `default`."""
    for t in _tokens(ptr):
        if isinstance(data, dict) and t in data:
            data = data[t]
        elif isinstance(data, list) and t.isdigit() and int(t) < len(data):
            data = data[int(t)]
        else:
            return default
    return data


def put(data: dict, ptr: str, value) -> None:
    """Sets `ptr` in `data`, making the objects on the way."""
    *head, last = _tokens(ptr)
    for t in head:
        data = data.setdefault(t, {})
    data[last] = value


def holds(cond: dict | None, data) -> bool:
    """Every pointer of `cond` is its value in `data` (or one of its values, when a list)."""
    for ptr, want in (cond or {}).items():
        have = get(data, ptr)
        if not (have in want if isinstance(want, list) else have == want):
            return False
    return True


def matches(mine, theirs) -> bool:
    """A waiting notice's value against a later one's: equal, or one of the later list."""
    return mine == theirs or (isinstance(theirs, list) and mine in theirs)


def strings(value, path: str):
    """(path, text) of every string in `value`."""
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for k, v in value.items():
            yield from strings(v, f"{path}.{k}")
    elif isinstance(value, list):
        for i, v in enumerate(value):
            yield from strings(v, f"{path}[{i}]")


def picks(text: str) -> list:
    """The lines a text draws from (`{pick:<name>}`)."""
    return [m[5:] for m in FILL.findall(text) if m.startswith("pick:")]


def render(value, data: dict, person: dict, draw):
    """`value` filled: a string that starts with `/` is the notice's value there; in other strings `{/pointer}`,
    `{person:<field>}` and `{pick:<lines>}` (`draw(name)`) are filled. Keys are filled in sorted order, so the
    draws don't depend on how a mapping was ordered."""
    if isinstance(value, dict):
        filled = {k: render(value[k], data, person, draw) for k in sorted(value)}
        return {k: filled[k] for k in value}
    if isinstance(value, list):
        return [render(v, data, person, draw) for v in value]
    if isinstance(value, str) and value.startswith("/"):
        return get(data, value)
    if isinstance(value, str):
        return FILL.sub(lambda m: _filled(m.group(1), data, person, draw), value)
    return value


def _filled(token: str, data: dict, person: dict, draw) -> str:
    if token.startswith("/"):
        found = get(data, token)
        return "" if found is None else found if isinstance(found, str) else str(found)
    if token.startswith("pick:"):
        return draw(token[5:])
    return str(person.get(token[7:], ""))
