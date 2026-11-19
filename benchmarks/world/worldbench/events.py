"""Events baked into a world at build: each `apply` op of an event that is on, filled with its params, appended
or set into a part's config, or added as a setup call of every task or as a link."""
import copy
from dataclasses import replace

from .errors import TemplateError
from .expand import expand


def apply_events(t, kept_events: list, c, ctx, world: dict, files: list) -> None:
    """Applies the kept events that are on to `world` (its `parts`, `tasks` and `links`), in template order."""
    parts = {p["part"]: p for p in world["parts"]}
    for e in kept_events:
        if not c.events[e["id"]]["on"]:
            continue
        ectx = replace(ctx, params=c.events[e["id"]]["params"])
        base = f"events[{t.events.index(e)}].apply"
        for i, op in enumerate(e["apply"]):
            op = expand(op, ectx, t.dir, files, f"{base}[{i}]")
            _apply(op, parts, world, f"{base}[{i}]")


def _part(parts: dict, name: str, path: str) -> dict:
    if name not in parts:
        raise TemplateError(path, f"no part {name} in this world")
    return parts[name]


def _apply(op: dict, parts: dict, world: dict, path: str) -> None:
    if "setup" in op:
        _part(parts, op["setup"]["part"], f"{path}.setup.part")
        for task in world["tasks"]:
            task["setup"].append(copy.deepcopy(op["setup"]))
    elif "link" in op:
        for end in ("from", "to"):
            _part(parts, op["link"][end], f"{path}.link.{end}")
        world["links"].append(op["link"])
    else:
        part = _part(parts, op["part"], f"{path}.part")
        if not isinstance(part.get("config"), dict):
            part["config"] = {}
        _into(part["config"], op, path)


def _into(config: dict, op: dict, path: str) -> None:
    dotted = op.get("append") or op["set"]
    *head, last = dotted.split(".")
    node = config
    for k in head:
        node = node.setdefault(k, {})
        if not isinstance(node, dict):
            raise TemplateError(path, f"{dotted}: {k} is not an object")
    if "set" in op:
        node[last] = op["value"]
        return
    target = node.setdefault(last, [])
    if not isinstance(target, list):
        raise TemplateError(path, f"{dotted} is not a list")
    target.append(op["value"])
