"""Prompts are files: a role's prompt is its ``.md`` file with ``{name}`` placeholders filled in.
Only ``{identifier}`` is a placeholder, so a prompt can show JSON (``{"items": []}``) as it is."""

from __future__ import annotations

import re
from pathlib import Path

_PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")


def load_prompt(path: Path, **values: str) -> str:
    """The prompt in ``path`` with its placeholders filled from ``values``; a placeholder without
    a value is a ``KeyError`` naming it and the file."""

    def fill(m: re.Match) -> str:
        key = m.group(1)
        if key not in values:
            raise KeyError(f"{path.name} needs a value for {{{key}}}")
        return str(values[key])

    return _PLACEHOLDER.sub(fill, path.read_text())
