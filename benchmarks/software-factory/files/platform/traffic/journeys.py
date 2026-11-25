"""A journey is what one user does, written as data (`journeys.json`): values drawn at the start (`draw`), then
steps run in order, each a request to a service whose path and JSON body name earlier values as `{name}`; an
answer can `save` values at JSON pointers and `pick` one item of a list (those whose `where` field is above 0).
A journey ends `completed`, `failed` at the first answer that is not a success (or no answer), or `abandoned`
when there is nothing to pick. Every choice comes from the journey's own `random.Random` (`random()` only)."""
import re

WHOLE = re.compile(r"^\{(\w+)\}$")


class Missing(Exception):
    pass


def draw(spec: dict, rng) -> object:
    low, high = spec["int"]
    n = low + int(rng.random() * (high - low + 1))
    return spec["format"].format(n) if "format" in spec else n


def fill(value, values: dict):
    """`{name}` in strings from `values`; a string that is exactly `{name}` takes the value as it is."""
    if isinstance(value, dict):
        return {k: fill(v, values) for k, v in value.items()}
    if not isinstance(value, str):
        return value
    whole = WHOLE.match(value)
    try:
        return values[whole.group(1)] if whole else value.format_map(values)
    except KeyError as e:
        raise Missing(str(e)) from None


def pointer(doc, path: str):
    for token in filter(None, path.split("/")):
        doc = doc[int(token)] if isinstance(doc, list) else doc[token]
    return doc


def pick(spec: dict, body, rng):
    items = [i for i in pointer(body, spec["from"]) if (i.get(spec["where"]) or 0) > 0]
    if not items:
        raise Missing(spec["from"])
    return pointer(items[int(rng.random() * len(items))], spec["take"])


def take(step: dict, body, values: dict, rng) -> None:
    for name, path in step.get("save", {}).items():
        values[name] = pointer(body, path)
    for name, spec in step.get("pick", {}).items():
        values[name] = pick(spec, body, rng)


def run(journey: dict, services: dict, rng, send) -> str:
    """Run `journey`; `send(step, method, url, json_body) -> client.Outcome` makes each request."""
    values = {name: draw(spec, rng) for name, spec in sorted(journey.get("draw", {}).items())}
    try:
        for step in journey["steps"]:
            body = fill(step["json"], values) if "json" in step else None
            url = services[step["service"]] + fill(step["path"], values)
            out = send(step, step["method"], url, body)
            if out.status is None or not 200 <= out.status < 300:
                return "failed"
            take(step, out.body, values, rng)
    except (Missing, KeyError, IndexError, TypeError, AttributeError):
        return "abandoned"
    return "completed"
