"""What the adapters share: a pydantic model for a tool's JSON schema, and a result as text."""

from __future__ import annotations

from typing import Any, Optional

from pydantic import BaseModel, ConfigDict, Field, create_model

from ..mcp import ToolResult

#: Placeholder key for clients pointed at the gateway, which holds the real one.
GATEWAY_KEY = "shakedown"

_TYPES: dict[str, Any] = {
    "string": str, "integer": int, "number": float, "boolean": bool, "array": list, "object": dict,
}


def _type_of(prop: dict[str, Any]) -> Any:
    t = prop.get("type")
    if isinstance(t, str) and t in _TYPES:
        return _TYPES[t]
    return Any


def model_from_schema(name: str, schema: dict[str, Any]) -> type[BaseModel]:
    """A pydantic model with one field per property of ``schema``. Simple types map to theirs,
    arrays to ``list``, objects to ``dict``, enums to their base type, anything else to ``Any``. A
    property not in ``required`` is ``Optional[...] = None``. A property whose name pydantic can't
    use as a field keeps it as the field's alias."""
    props = schema.get("properties") or {}
    required = set(schema.get("required") or [])
    fields: dict[str, Any] = {}
    for i, (prop, spec) in enumerate(props.items()):
        typ = _type_of(spec if isinstance(spec, dict) else {})
        desc = spec.get("description") if isinstance(spec, dict) else None
        usable = prop.isidentifier() and not prop.startswith("_") and not hasattr(BaseModel, prop)
        field_name = prop if usable else f"field_{i}"
        alias = None if usable else prop
        if prop in required:
            fields[field_name] = (typ, Field(..., description=desc, alias=alias))
        else:
            fields[field_name] = (Optional[typ], Field(None, description=desc, alias=alias))
    cls = "".join(p.capitalize() for p in name.replace("-", "_").split("_") if p) or "Tool"
    config = ConfigDict(populate_by_name=True, extra="allow")
    return create_model(f"{cls}Args", __config__=config, **fields)


def result_text(r: ToolResult) -> str:
    """The text an agent sees for a call: the result, or ``error: …`` when it was refused or failed
    (never raised, so the refusal is recorded and the agent can react)."""
    return r.text if not r.is_error else f"error: {r.text}"


def call_text(agent: Any, name: str, args: dict[str, Any]) -> str:
    """Calls ``name`` as ``agent`` with ``args`` (``None`` values dropped), as text. Arguments
    that aren't an object come back as an error the agent can read."""
    if not isinstance(args, dict):
        return f"error: the arguments must be a JSON object, not {type(args).__name__}"
    clean = {k: v for k, v in args.items() if v is not None}
    try:
        return result_text(agent.call(name, clean))
    except Exception as e:  # noqa: BLE001 - the agent sees the failure, the run goes on
        return f"error: {e}"
