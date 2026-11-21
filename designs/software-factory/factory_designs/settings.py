"""The model settings both designs share, read from ``shared.yaml``: the same provider, model,
reasoning effort, output cap, temperature and tool limits for every design."""

from __future__ import annotations

import dataclasses
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping

import yaml

#: The one settings file of both designs.
SHARED = Path(__file__).resolve().parent.parent / "shared.yaml"

#: What the environment may override (a run on another provider); nothing else is.
OVERRIDES = {"FACTORY_PROVIDER": "provider", "FACTORY_MODEL": "model"}


@dataclass(frozen=True)
class Settings:
    provider: str
    model: str
    reasoning_effort: str
    max_tokens: int
    temperature: float
    tool_output_chars: int
    command_timeout_s: int
    workbench_image: str


def load_settings(path: Path = SHARED, env: Mapping[str, str] = os.environ) -> Settings:
    """The settings in ``path``, with ``FACTORY_PROVIDER`` and ``FACTORY_MODEL`` from ``env``
    when set. A missing field is a ``ValueError`` naming it."""
    data = yaml.safe_load(path.read_text()) or {}
    data.update({field: env[var] for var, field in OVERRIDES.items() if env.get(var)})
    fields = {f.name: f.type for f in dataclasses.fields(Settings)}
    missing = [name for name in fields if name not in data]
    if missing:
        raise ValueError(f"{path.name} is missing {', '.join(missing)}")
    casts = {"int": int, "float": float, "str": str}
    return Settings(**{name: casts[typ](data[name]) for name, typ in fields.items()})
