"""Filling a template's values for one world: placeholders and includes (the template format, "Placeholders" and
"Includes")."""
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from . import limits
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


@dataclass
class _Budget:
    """What one expansion may still produce (`limits.MAX_EXPANDED` values), and the includes it is inside."""
    left: int
    chain: list = field(default_factory=list)


def expand(value, ctx: Context, base: Path, files: list, path: str = "", budget: _Budget | None = None) -> Any:
    """`value` with every string filled; folder includes are appended to `files` as `(source, "files/<path>")`."""
    budget = budget or _Budget(limits.MAX_EXPANDED)
    budget.left -= 1
    if budget.left < 0:
        raise TemplateError(path, f"expands to more than {limits.MAX_EXPANDED} values")
    if isinstance(value, dict):
        return {k: expand(v, ctx, base, files, key_path(path, k), budget) for k, v in value.items()}
    if isinstance(value, list):
        return [expand(v, ctx, base, files, key_path(path, i), budget) for i, v in enumerate(value)]
    if isinstance(value, str):
        return _include(value[1:], ctx, base, files, path, budget) if is_include(value) else fill(value, ctx, path)
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


def read_include(rel: str, base: Path, path: str) -> Any:
    """The file `@rel` of the template in `base`, parsed (.yaml, .yml, .json) or as text, not yet filled."""
    target = inside(base, rel, path)
    if not target.is_file():
        raise TemplateError(path, f"no file {rel}")
    text = limits.read_text(target, rel, path)
    return text if target.suffix in _TEXT else limits.parse(text, target.suffix, rel, f"{path}@{rel}")


def _include(rel: str, ctx: Context, base: Path, files: list, path: str, budget: _Budget) -> Any:
    target = inside(base, rel, path)
    if rel.endswith("/"):
        return _folder(rel, target, base, files, path)
    if target in budget.chain:
        raise TemplateError(path, f"@{rel} includes itself")
    if len(budget.chain) >= limits.MAX_INCLUDE_DEPTH:
        raise TemplateError(path, f"includes nest deeper than {limits.MAX_INCLUDE_DEPTH}")
    value, inner = read_include(rel, base, path), f"{path}@{rel}"
    if target.suffix in _TEXT:
        return fill(value, ctx, inner)
    budget.chain.append(target)
    try:
        return expand(value, ctx, base, files, inner, budget)
    finally:
        budget.chain.pop()


def _folder(rel: str, target: Path, base: Path, files: list, path: str) -> str:
    """`${WORLD_DIR}/files/<the folder's place in the template>/`; the folder is copied there, so its place is the
    resolved one (`a/../b/` is `b/`), and no two folders copied may hold one another."""
    if not target.is_dir():
        raise TemplateError(path, f"no folder {rel}")
    place = target.relative_to(base.resolve()).as_posix()
    if place == ".":
        raise TemplateError(path, f"@{rel} is the whole template folder")
    dest = f"files/{place}/"
    for _, other in files:
        if other != dest and (other.startswith(dest) or dest.startswith(other)):
            raise TemplateError(path, f"@{place}/ overlaps @{other.removeprefix('files/')}")
    limits.check_folder(target, f"{place}/", path)
    if (base / place, dest) not in files:
        files.append((base / place, dest))
    return f"{WORLD_DIR}/{dest}"
