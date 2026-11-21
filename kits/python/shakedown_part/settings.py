"""Shakedown's settings from the environment: ``SHAKEDOWN_<NAME>``."""

from __future__ import annotations

import os
from typing import Mapping, Optional

PREFIX = "SHAKEDOWN_"


def setting(name: str, default: Optional[str] = None, environ: Optional[Mapping[str, str]] = None) -> Optional[str]:
    """``os.environ.get(name, default)`` (or ``environ``'s)."""
    env = os.environ if environ is None else environ
    return env.get(name, default)
