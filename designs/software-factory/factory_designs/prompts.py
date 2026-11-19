"""Prompts are files: a role's prompt is its ``.md`` file with ``{name}`` placeholders filled in.
Only ``{identifier}`` is a placeholder, so a prompt can show JSON (``{"items": []}``) as it is."""

from __future__ import annotations

import re
from pathlib import Path

_PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")


def fill_in(template: str, values: dict[str, str], where: str) -> str:
    """``template`` with its placeholders filled from ``values``; a placeholder without a value
    is a ``KeyError`` naming it and ``where`` the template came from."""

    def fill(m: re.Match) -> str:
        key = m.group(1)
        if key not in values:
            raise KeyError(f"{where} needs a value for {{{key}}}")
        return str(values[key])

    return _PLACEHOLDER.sub(fill, template)


def load_prompt(path: Path, **values: str) -> str:
    """The prompt in ``path`` with its placeholders filled from ``values``."""
    return fill_in(path.read_text(), values, path.name)
