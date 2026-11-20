"""Shared by the examples: connect to shaked.

- ``ARENA_URL`` set: connect to that shaked (any benchmark).
- otherwise: start one with ``Shakedown.local`` from ``ARENA_BIN_DIR`` (a ``cargo build --release`` of this
  repository: ``core/target/release``), on the Postgres at ``ARENA_PG_URL`` (default
  ``deploy/compose.yml``'s), with the benchmark ``ARENA_BENCHMARK`` (default ``retail``).
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

from shakedown import Shakedown

REPO = Path(__file__).resolve().parents[3]


def connect() -> Shakedown:
    url = os.environ.get("ARENA_URL")
    if url:
        return Shakedown(url)
    bin_dir = os.environ.get("ARENA_BIN_DIR", str(REPO / "core/target/release"))
    return Shakedown.local(
        benchmark=os.environ.get("ARENA_BENCHMARK", "retail"),
        packages=[str(REPO / "integrations"), str(REPO / "benchmarks")],
        bin_dir=bin_dir,
        pg_url=os.environ.get("ARENA_PG_URL"),
        saves=os.environ.get("ARENA_SAVES"),
    )


def need_claude() -> None:
    if not shutil.which("claude"):
        sys.exit("skipped: this example runs Claude Code agents; install `claude` and sign in")


def task_arg(arena: Shakedown, default_index: int = 0) -> str:
    """The task id from argv[1], else the benchmark's first (or ``default_index``th) task."""
    if len(sys.argv) > 1 and sys.argv[1] != "-":
        return sys.argv[1]
    return sd.tasks()[default_index].id
