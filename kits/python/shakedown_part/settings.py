"""Shakedown's settings from the environment: ``SHAKEDOWN_<NAME>``, else the old name ``ARENA_<NAME>``
(the prefix until 2026-11-19), still read with one warning per name. A program shaked starts gets both
names, so code written for either reads its own."""

from __future__ import annotations

import os
import warnings
from typing import Mapping, Optional

PREFIX = "SHAKEDOWN_"
OLD_PREFIX = "ARENA_"
_warned: set[str] = set()


def old_name(name: str) -> str:
    """``SHAKEDOWN_X`` → ``ARENA_X`` (a name without the prefix is its own)."""
    return OLD_PREFIX + name[len(PREFIX):] if name.startswith(PREFIX) else name


def setting(name: str, default: Optional[str] = None, environ: Optional[Mapping[str, str]] = None) -> Optional[str]:
    """``os.environ.get(name, default)``, reading the old name when ``name`` is not set."""
    env = os.environ if environ is None else environ
    if name in env:
        return env[name]
    old = old_name(name)
    if old not in env:
        return default
    if old not in _warned:
        _warned.add(old)
        warnings.warn(f"{old} is an old name, read for now; set {name} instead", FutureWarning, stacklevel=2)
    return env[old]


def both(name: str, value: str) -> dict[str, str]:
    """``name`` and its old name set to ``value``: what a program started for Shakedown is given."""
    return {name: value, old_name(name): value}
