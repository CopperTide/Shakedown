"""A template's data checked: against its JSON Schema, then across its lists (ids, the systems they name, files
and ranges)."""
import json
from functools import cache
from pathlib import Path

import jsonschema

from .errors import TemplateError, key_path
from .expand import read_include

SCHEMA = Path(__file__).resolve().parent / "schema" / "template.schema.json"
RESERVED_FIELDS = {"id", "name", "login", "role", "added"}


@cache
def _validator(definition: str | None = None) -> jsonschema.Draft202012Validator:
    """The template's schema, or one of its `$defs` (`checks`) on its own."""
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    if definition is not None:
        schema = {"$schema": schema["$schema"], "$defs": schema["$defs"], "$ref": f"#/$defs/{definition}"}
    return jsonschema.Draft202012Validator(schema)


def with_event_files(data, folder: Path):
    """`data` with each event written in its own file (`"@events/x.yaml"`) read in its place, held to the event's
    schema."""
    events = data.get("events") if isinstance(data, dict) else None
    if not isinstance(events, list):
        return data
    out = []
    for i, e in enumerate(events):
        if isinstance(e, str) and e.startswith("@"):
            e = _included_file(e, folder, f"events[{i}]", "event")
        out.append(e)
    return {**data, "events": out}


def check(data, folder: Path) -> None:
    """Raises the first TemplateError of `data`, the parsed template.yaml of `folder` (its event files read in)."""
    _schema(data)
    _included(data, folder)
    _unique_ids(data)
    _known_systems(data)
    _ranges(data)
    _files(data, folder)
    _names_and_fields(data)


def _schema(data, definition: str | None = None, path: str = "") -> None:
    error = jsonschema.exceptions.best_match(_validator(definition).iter_errors(data))
    if error is not None:
        for k in error.absolute_path:
            path = key_path(path, k)
        raise TemplateError(path, error.message)


def _included(data, folder: Path) -> None:
    """Name pools and a task's checks in their own file are held to the same schema as when written in place, and
    a check names only the template's systems."""
    if isinstance(data.get("names"), str):
        _included_file(data["names"], folder, "names", "namePools")
    known = {s["id"] for s in data["systems"]}
    for i, task in enumerate(data["tasks"]):
        path, checks = f"tasks[{i}].checks", task["checks"]
        if isinstance(checks, str):
            path, checks = f"{path}@{checks[1:]}", _included_file(checks, folder, path, "checks")
        _check_needs(checks, known, path)


def _included_file(include: str, folder: Path, path: str, definition: str):
    rel = include[1:]
    value = read_include(rel, folder, path)
    _schema(value, definition, f"{path}@{rel}")
    return value


def _check_needs(checks: list, known: set, path: str) -> None:
    for i, item in enumerate(checks):
        for j, sid in enumerate(item.get("needs", [])):
            if sid not in known:
                raise TemplateError(f"{path}[{i}].needs[{j}]", f"no system {sid}")


def _first_repeat(items, key: str, path: str) -> None:
    seen = set()
    for i, item in enumerate(items):
        if item[key] in seen:
            raise TemplateError(key_path(key_path(path, i), key), f"{item[key]} is used twice")
        seen.add(item[key])


def _unique_ids(data) -> None:
    for name in ("systems", "roles", "events", "tasks"):
        _first_repeat(data.get(name, []), "id", name)
    for i, s in enumerate(data["systems"]):
        _first_repeat(s.get("options", []), "name", f"systems[{i}].options")
    for i, e in enumerate(data.get("events", [])):
        _first_repeat(e.get("params", []), "name", f"events[{i}].params")
    seen = set()
    for i, s in enumerate(data["systems"]):
        for j, p in enumerate(s["parts"]):
            if p["part"] in seen:
                raise TemplateError(f"systems[{i}].parts[{j}].part", f"{p['part']} is used twice")
            seen.add(p["part"])


_SYSTEM_LISTS = (("systems", "requires"), ("roles", "systems"), ("events", "needs"), ("tasks", "needs"),
                 ("links", "needs"), ("gates", "needs"))


def _known_systems(data) -> None:
    known = {s["id"] for s in data["systems"]}
    for name, field in _SYSTEM_LISTS:
        for i, item in enumerate(data.get(name, [])):
            for j, sid in enumerate(item.get(field, [])):
                if sid not in known:
                    raise TemplateError(f"{name}[{i}].{field}[{j}]", f"no system {sid}")


def _within(value, low, high, path: str) -> None:
    if not low <= value <= high:
        raise TemplateError(path, f"{value} is outside {low}..{high}")


def _ranges(data) -> None:
    size = data["size"]
    if size["min"] > size["max"]:
        raise TemplateError("size.min", f"{size['min']} is above max {size['max']}")
    _within(size["default"], size["min"], size["max"], "size.default")
    for i, n in enumerate(size.get("presets", [])):
        _within(n, size["min"], size["max"], f"size.presets[{i}]")
    for i, s in enumerate(data["systems"]):
        for j, o in enumerate(s.get("options", [])):
            if o["default"] not in o["choices"]:
                raise TemplateError(f"systems[{i}].options[{j}].default", f"{o['default']} is not a choice")
    for i, e in enumerate(data.get("events", [])):
        for j, p in enumerate(e.get("params", [])):
            _param_default(p, f"events[{i}].params[{j}].default")
        if not e["apply"] and not e.get("fixed"):
            raise TemplateError(f"events[{i}].apply", "an event that isn't fixed applies something")


def _param_default(p: dict, path: str) -> None:
    if p["kind"] == "choice":
        if p["default"] not in p.get("choices", []):
            raise TemplateError(path, f"{p['default']} is not a choice")
    elif not isinstance(p["default"], int):
        raise TemplateError(path, f"{p['default']} is not a number")
    else:
        _within(p["default"], p.get("min", p["default"]), p.get("max", p["default"]), path)


def _files(data, folder: Path) -> None:
    for i, r in enumerate(data.get("roles", [])):
        if "rules" in r:
            path = f"roles[{i}].rules"
            target = (folder / r["rules"]).resolve()
            if not target.is_relative_to(folder.resolve()):
                raise TemplateError(path, f"{r['rules']} leaves the template folder")
            if not target.is_file():
                raise TemplateError(path, f"no file {r['rules']}")


def _names_and_fields(data) -> None:
    if data.get("roles") and "names" not in data:
        raise TemplateError("names", "roles need name pools (first and last names)")
    for i, r in enumerate(data.get("roles", [])):
        for f in r.get("fields", {}):
            if f in RESERVED_FIELDS:
                raise TemplateError(f"roles[{i}].fields.{f}", f"{f} is set by the builder")
