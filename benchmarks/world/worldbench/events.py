"""Events and systems baked into a world at build: each `apply` op of a chosen system (filled like the rest of the
template), then of an event that is on (filled with its params), appended, set or merged into a part's config, or
added as a setup call of every task or as a link."""
import copy
from dataclasses import replace

from .errors import TemplateError
from .expand import expand


def apply_systems(t, c, ctx, world: dict, files: list) -> None:
    """Applies each chosen system's `apply` to `world`, in template order."""
    for i, s in enumerate(t.systems):
        if s["id"] in c.systems:
            _apply_all(s.get("apply", []), ctx, t.dir, world, files, f"systems[{i}].apply")


def apply_events(t, kept_events: list, c, ctx, world: dict, files: list) -> None:
    """Applies the kept events that are on to `world` (its `parts`, `tasks` and `links`), in template order."""
    for e in kept_events:
        if c.events[e["id"]]["on"]:
            _apply_all(e["apply"], replace(ctx, params=c.events[e["id"]]["params"]), t.dir, world, files,
                       f"events[{t.events.index(e)}].apply")


def _apply_all(ops: list, ctx, base_dir, world: dict, files: list, base: str) -> None:
    parts = {p["part"]: p for p in world["parts"]}
    for i, op in enumerate(ops):
        _apply(expand(op, ctx, base_dir, files, f"{base}[{i}]"), parts, world, f"{base}[{i}]")


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
    dotted = op.get("append") or op.get("merge") or op["set"]
    *head, last = dotted.split(".")
    node = config
    for k in head:
        node = node.setdefault(k, {})
        if not isinstance(node, dict):
            raise TemplateError(path, f"{dotted}: {k} is not an object")
    if "set" in op:
        node[last] = op["value"]
        return
    if "merge" in op:
        _merge(node, last, op["value"], dotted, path)
        return
    target = node.setdefault(last, [])
    if not isinstance(target, list):
        raise TemplateError(path, f"{dotted} is not a list")
    target.append(op["value"])


def _merge(node: dict, last: str, value, dotted: str, path: str) -> None:
    if not isinstance(value, dict):
        raise TemplateError(path, "merge needs an object")
    target = node.setdefault(last, {})
    if not isinstance(target, dict):
        raise TemplateError(path, f"{dotted} is not an object")
    target.update(value)
