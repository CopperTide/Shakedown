"""A built world written as a benchmark package: world.json, its copied folders, and shakedown-package.toml."""
import hashlib
import json
import os
import shutil
from pathlib import Path

from .errors import TemplateError
from .generate import Result, world_json

_PROJECT = "${SHAKEDOWN_DIR}/benchmarks/world"


def _toml(s: str) -> str:
    """`s` as a TOML basic string: JSON's escapes are TOML's except the surrogate pairs it writes for an emoji, so
    characters beyond ASCII stay as they are; DEL is the one control character JSON leaves bare and TOML refuses."""
    return json.dumps(s, ensure_ascii=False).replace("\x7f", "\\u007f")


def manifest(world: dict, t, version: str) -> str:
    command = ["uv", "run", "--project", _PROJECT, "python", "-m", "worldbench", "--world", "world.json",
               "--version", version]
    return "\n".join([
        'kind = "benchmark"',
        f"name = {_toml(world['name'])}",
        f"version = {_toml(version)}",
        "protocol = 1",
        "opt_in = true",
        f"run = {{ command = [{', '.join(_toml(a) for a in command)}] }}",
        "needs = { gpu_vram_gb = 0, services = [] }",
        "",
        "[world]",
        f"title = {_toml(world['title'])}",
        f"template = {_toml(t.id)}",
        f"template_name = {_toml(t.name)}",
        "",
    ])


def _no_links(folder: str, names: list) -> list:
    """copytree's `ignore`: a link in a copied folder (made after the preview checked it) is refused, not followed."""
    for n in names:
        if os.path.islink(os.path.join(folder, n)):
            raise TemplateError("", f"{os.path.join(folder, n)} is a link")
    return []


def write_package(result: Result, t, dir: Path) -> dict:
    """Writes the package into `dir` (which must not exist; nothing is left behind on failure) and answers
    `{name, title, version}`; the version is the template's plus the first 8 hex of world.json's sha256."""
    text = world_json(result.world)
    version = f"{t.version}+{hashlib.sha256(text.encode()).hexdigest()[:8]}"
    dir.mkdir(parents=True)
    try:
        (dir / "world.json").write_text(text, encoding="utf-8")
        for src, dst in result.files:
            shutil.copytree(src, dir / dst, ignore=_no_links)
        (dir / "shakedown-package.toml").write_text(manifest(result.world, t, version), encoding="utf-8")
    except BaseException:
        shutil.rmtree(dir, ignore_errors=True)
        raise
    return {"name": result.world["name"], "title": result.world["title"], "version": version}
