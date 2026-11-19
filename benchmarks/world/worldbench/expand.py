"""Filling a template's values for one world: placeholders and includes (the template format, "Placeholders" and
"Includes")."""
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from .errors import TemplateError, key_path

WORLD_DIR = "${WORLD_DIR}"
_TOKEN = re.compile(r"\{\{|\}\}|\{([a-z_]+(?:\.[a-z0-9_-]+)*)\}")
_PARSED = {".yaml", ".yml", ".json"}
_TEXT = {".md", ".txt"}


@dataclass
class Context:
    """What placeholders read: the company (name, slug, key, domain), the size, each system's options, the
    people and an event's params."""
    company: dict
    size: int
    options: dict
    people: list
    params: dict


def expand(value, ctx: Context, base: Path, files: list, path: str = "") -> Any:
    """`value` with every string filled; folder includes are appended to `files` as `(source, "files/<path>")`."""
    if isinstance(value, dict):
        return {k: expand(v, ctx, base, files, key_path(path, k)) for k, v in value.items()}
    if isinstance(value, list):
        return [expand(v, ctx, base, files, key_path(path, i)) for i, v in enumerate(value)]
    if isinstance(value, str):
        return _include(value[1:], ctx, base, files, path) if is_include(value) else fill(value, ctx, path)
    return value


def is_include(s: str) -> bool:
    """A string naming a file or folder of the template: `@` and a path ending in a known suffix or `/`."""
    return s.startswith("@") and (s.endswith("/") or Path(s).suffix in _PARSED | _TEXT)


def fill(s: str, ctx: Context, path: str) -> Any:
    """`s` with its placeholders filled; a string that is exactly one placeholder takes the value as it is."""
    whole = _TOKEN.fullmatch(s)
    if whole and whole.group(1):
        return _lookup(whole.group(1), ctx, path)
    return _TOKEN.sub(lambda m: _as_text(m, ctx, path), s)


def _as_text(m: re.Match, ctx: Context, path: str) -> str:
    if m.group(1) is None:
        return m.group(0)[0]
    value = _lookup(m.group(1), ctx, path)
    if isinstance(value, (list, dict)):
        kind = "a list" if isinstance(value, list) else "an object"
        raise TemplateError(path, f"{{{m.group(1)}}} is {kind}: use it as the whole string")
    return str(value)


def _lookup(name: str, ctx: Context, path: str) -> Any:
    head, *rest = name.split(".")
    try:
        return _RESOLVERS[(head, len(rest))](ctx, *rest)
    except KeyError:
        raise TemplateError(path, f"unknown placeholder {{{name}}}") from None


_RESOLVERS = {
    ("company", 1): lambda ctx, k: ctx.company[k],
    ("size", 0): lambda ctx: ctx.size,
    ("options", 2): lambda ctx, system, name: ctx.options[system][name],
    ("people", 0): lambda ctx: ctx.people,
    ("people", 1): lambda ctx, role: [p for p in ctx.people if p.get("role") == role],
    ("param", 1): lambda ctx, k: ctx.params[k],
    ("world_dir", 0): lambda ctx: WORLD_DIR,
}


def inside(base: Path, rel: str, path: str) -> Path:
    """`base/rel`, refused when it leaves `base`."""
    target = (base / rel).resolve()
    if not target.is_relative_to(base.resolve()):
        raise TemplateError(path, f"@{rel} leaves the template folder")
    return target


def _include(rel: str, ctx: Context, base: Path, files: list, path: str) -> Any:
    target = inside(base, rel, path)
    if rel.endswith("/"):
        return _folder(rel, target, base, files, path)
    if not target.is_file():
        raise TemplateError(path, f"no file {rel}")
    text, inner = target.read_text(), f"{path}@{rel}"
    if target.suffix in _TEXT:
        return fill(text, ctx, inner)
    return expand(_parse(text, target.suffix, inner), ctx, base, files, inner)


def _folder(rel: str, target: Path, base: Path, files: list, path: str) -> str:
    if not target.is_dir():
        raise TemplateError(path, f"no folder {rel}")
    entry = (base / rel.rstrip("/"), f"files/{rel}")
    if entry not in files:
        files.append(entry)
    return f"{WORLD_DIR}/files/{rel}"


def _parse(text: str, suffix: str, path: str) -> Any:
    try:
        return json.loads(text) if suffix == ".json" else yaml.safe_load(text)
    except (ValueError, yaml.YAMLError) as e:
        raise TemplateError(path, f"can't parse: {e}") from None
