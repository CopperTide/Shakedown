"""Shared by the examples: connect to shaked.

- ``SHAKEDOWN_URL`` set: connect to that shaked (any benchmark).
- otherwise: start one with ``Shakedown.local`` from ``SHAKEDOWN_BIN_DIR`` (a ``cargo build --release`` of this
  repository: ``core/target/release``), on the Postgres at ``SHAKEDOWN_PG_URL`` (default
  ``deploy/compose.yml``'s), with the benchmark ``SHAKEDOWN_BENCHMARK`` (default ``retail``).
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

from shakedown import Shakedown
from shakedown.settings import setting

REPO = Path(__file__).resolve().parents[3]


def connect() -> Shakedown:
    url = setting("SHAKEDOWN_URL")
    if url:
        return Shakedown(url)
    bin_dir = setting("SHAKEDOWN_BIN_DIR", str(REPO / "core/target/release"))
    return Shakedown.local(
        benchmark=setting("SHAKEDOWN_BENCHMARK", "retail"),
        packages=[str(REPO / "integrations"), str(REPO / "benchmarks")],
        bin_dir=bin_dir,
        pg_url=setting("SHAKEDOWN_PG_URL"),
        saves=setting("SHAKEDOWN_SAVES"),
    )


def need_claude() -> None:
    if not shutil.which("claude"):
        sys.exit("skipped: this example runs Claude Code agents; install `claude` and sign in")


def task_arg(arena: Shakedown, default_index: int = 0) -> str:
    """The task id from argv[1], else the benchmark's first (or ``default_index``th) task."""
    if len(sys.argv) > 1 and sys.argv[1] != "-":
        return sys.argv[1]
    return sd.tasks()[default_index].id
