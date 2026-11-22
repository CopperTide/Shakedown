"""Checking what callers pass: tool arguments against their parameters, and config values."""
from dataclasses import dataclass
from datetime import datetime

from . import errors


class Refusal(Exception):
    """A tool refused the call (bad argument, unknown id): answered as a tool-level `error`."""


REQUIRED = object()
TYPES = {"string": (str,), "integer": (int,), "number": (int, float), "boolean": (bool,), "array": (list,),
         "object": (dict,)}


def _is(value, kind: str) -> bool:
    return isinstance(value, TYPES[kind]) and (kind == "boolean" or not isinstance(value, bool))


@dataclass(frozen=True)
class Param:
    name: str
    type: str  # a JSON Schema type, a key of TYPES
    default: object = REQUIRED
    # An array's items: a JSON Schema type name (`"string"`), or a whole JSON Schema (an object's `properties`,
    # `required`). Models fill an array whose item type they are told; told nothing, some send `[]`.
    items: object = None

    def check(self, value):
        if not _is(value, self.type):
            if value is None and self.default is None:
                return
            raise Refusal(f"{self.name} must be of type {self.type}")
        kind = self.items if isinstance(self.items, str) else (self.items or {}).get("type")
        if self.type == "array" and kind in TYPES:
            for i, v in enumerate(value):
                if not _is(v, kind):
                    raise Refusal(f"{self.name}[{i}] must be of type {kind}")

    def schema(self) -> dict:
        out = {"type": self.type}
        if self.items is not None:
            out["items"] = {"type": self.items} if isinstance(self.items, str) else dict(self.items)
        return out


def input_schema(params) -> dict:
    """The MCP `inputSchema` of a tool with these params."""
    props = {p.name: p.schema() for p in params}
    return {"type": "object", "properties": props, "required": [p.name for p in params if p.default is REQUIRED]}


def normalize(tool: str, params, args) -> dict:
    """`args` checked against `params`, with defaults filled in; Refusal when they don't fit."""
    if not isinstance(args, dict):
        raise Refusal("args must be an object")
    out = {}
    for p in params:
        if p.name in args:
            p.check(args[p.name])
            out[p.name] = args[p.name]
        elif p.default is REQUIRED:
            raise Refusal(f"{tool}: missing {p.name}")
        else:
            out[p.name] = p.default
    return out


def refused(message: str) -> dict:
    """A call_tool result for a refusal: the error and no keys touched."""
    return {"error": message, "reads": [], "writes": []}


def config_datetime(config: dict, key: str, default: str) -> datetime:
    """`config[key]` (or `default`) as an ISO datetime; `invalid` otherwise."""
    raw = config.get(key) or default
    try:
        return datetime.fromisoformat(raw)
    except (TypeError, ValueError):
        raise errors.invalid(f"{key} must be an ISO datetime, got {raw!r}") from None
