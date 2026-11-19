"""How large a template may be (its files, how deep and how many values they hold, a folder it copies, what one
value expands to), and reading its files within those limits, so that no template can stall the builder, exhaust
its memory or copy what lies outside its folder."""
import json
import os
from pathlib import Path
from typing import Any

import yaml

from .errors import TemplateError

MAX_FILE_BYTES = 1_000_000
MAX_VALUES = 100_000
MAX_DEPTH = 64
MAX_EXPANDED = 200_000
MAX_INCLUDE_DEPTH = 8
MAX_FOLDER_FILES = 2_000
MAX_FOLDER_BYTES = 50_000_000


def read_text(file: Path, name: str, path: str) -> str:
    """The UTF-8 text of `file` (`name` in messages); a TemplateError at `path` when it is too large, can't be
    read or isn't UTF-8."""
    try:
        if file.stat().st_size > MAX_FILE_BYTES:
            raise TemplateError(path, f"{name} is larger than {MAX_FILE_BYTES} bytes")
        return file.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        raise TemplateError(path, f"{name} is not UTF-8 text") from None
    except OSError as e:
        raise TemplateError(path, f"{name} can't be read: {e.strerror or e}") from None


def parse(text: str, suffix: str, name: str, path: str) -> Any:
    """`text` parsed as JSON (`.json`) or YAML, refused when it is nested too deep or holds too many values (a YAML
    alias bomb holds billions)."""
    try:
        value = json.loads(text) if suffix == ".json" else yaml.safe_load(text)
    except (ValueError, yaml.YAMLError) as e:
        raise TemplateError(path, f"{name} can't be parsed: {e}") from None
    except RecursionError:
        raise TemplateError(path, f"{name} is nested deeper than {MAX_DEPTH} levels") from None
    _bounded(value, name, path)
    return value


def _bounded(value, name: str, path: str) -> None:
    stack, seen = [(value, 0)], 0
    while stack:
        v, depth = stack.pop()
        seen += 1
        if seen > MAX_VALUES:
            raise TemplateError(path, f"{name} holds more than {MAX_VALUES} values")
        if depth > MAX_DEPTH:
            raise TemplateError(path, f"{name} is nested deeper than {MAX_DEPTH} levels")
        if isinstance(v, (dict, list)):
            stack.extend((x, depth + 1) for x in (v.values() if isinstance(v, dict) else v))


def check_folder(folder: Path, rel: str, path: str) -> None:
    """A folder a template copies holds only files and folders (no links out), and not too many or too much."""
    count = size = 0
    for root, dirs, files in os.walk(folder):
        for n in dirs + files:
            p = Path(root, n)
            if p.is_symlink() or not (p.is_dir() or p.is_file()):
                kind = "a link" if p.is_symlink() else "not a file"
                raise TemplateError(path, f"@{rel}{p.relative_to(folder).as_posix()} is {kind}")
        count += len(files)
        size += sum(Path(root, f).stat().st_size for f in files)
        if count > MAX_FOLDER_FILES:
            raise TemplateError(path, f"@{rel} holds more than {MAX_FOLDER_FILES} files")
        if size > MAX_FOLDER_BYTES:
            raise TemplateError(path, f"@{rel} holds more than {MAX_FOLDER_BYTES} bytes")
