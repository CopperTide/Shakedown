"""The core the integration tests use.

- ``SHAKEDOWN_SDK_URL``: a shaked that is already running (any benchmark); or
- a shaked the tests start (``Shakedown.local``) with the retail example benchmark, from
  ``SHAKEDOWN_SDK_BIN_DIR`` (default ``core/target/release``, then ``core/target/debug``; it needs
  ``shaked``, ``arena-retail`` and ``arena-postgres``) on the Postgres at ``SHAKEDOWN_SDK_PG_URL``
  (default ``deploy/compose.yml``'s, ``postgres://arena:arena@127.0.0.1:5544/postgres``).

The shaked the tests start cleans up unreferenced saves when it starts, like any shaked, so point
``SHAKEDOWN_SDK_PG_URL`` at a Postgres server no other test run is using at the same time.

Without either, the tests marked ``core`` are skipped. They drive the API directly (start and finish
external runs, call a tool endpoint, fork, read records, export) and never pose as an agent. The one
exception is the website sample (`test_sample.py`): its Claude Code agent is `fake_bin/claude`, a
stand-in that calls no model, first on the started shaked's PATH.
"""

from __future__ import annotations

import os
import shutil
import socket
import tempfile
from pathlib import Path
from urllib.parse import urlparse

import pytest

from shakedown import Shakedown
from shakedown.settings import setting

REPO = Path(__file__).resolve().parents[3]
#: First on the PATH of the shaked the tests start: a stand-in `claude` that calls no model.
FAKE_BIN = Path(__file__).resolve().parent / "fake_bin"
NEEDED = ("shaked", "arena-retail", "arena-postgres")


def _bin_dir() -> str | None:
    cands = [setting("SHAKEDOWN_SDK_BIN_DIR"), REPO / "core/target/release", REPO / "core/target/debug"]
    for c in cands:
        if c and all((Path(c) / n).is_file() for n in NEEDED):
            return str(c)
    return None


def _reachable(pg_url: str) -> bool:
    u = urlparse(pg_url)
    try:
        with socket.create_connection((u.hostname or "127.0.0.1", u.port or 5432), timeout=2):
            return True
    except OSError:
        return False


@pytest.fixture(scope="session")
def arena():
    url = setting("SHAKEDOWN_SDK_URL")
    if url:
        a = Shakedown(url)
        try:
            a.health()
        except Exception as e:  # noqa: BLE001
            pytest.skip(f"SHAKEDOWN_SDK_URL={url} does not answer: {e}")
        yield a
        a.close()
        return
    bin_dir = _bin_dir()
    pg = setting("SHAKEDOWN_SDK_PG_URL", "postgres://arena:arena@127.0.0.1:5544/postgres")
    if bin_dir is None:
        pytest.skip("no shaked build (set SHAKEDOWN_SDK_BIN_DIR or SHAKEDOWN_SDK_URL)")
    if not _reachable(pg):
        pytest.skip(f"no Postgres at {urlparse(pg).hostname}:{urlparse(pg).port} (set SHAKEDOWN_SDK_PG_URL)")
    saves = tempfile.mkdtemp(prefix="arena-sdk-test-saves-")
    a = Shakedown.local(
        benchmark="retail",
        packages=[str(REPO / "integrations"), str(REPO / "benchmarks")],
        bin_dir=bin_dir,
        pg_url=pg,
        saves=saves,
        cors_origin="http://127.0.0.1:5173",
        env={"PATH": f"{FAKE_BIN}{os.pathsep}{os.environ.get('PATH', '')}"},
    )
    yield a
    a.close()
    shutil.rmtree(saves, ignore_errors=True)


@pytest.fixture
def fake_claude(arena):
    """The shaked here runs Claude Code agents with `tests/fake_bin/claude` (only one the tests started)."""
    if not hasattr(arena, "process"):
        pytest.skip("only for a shaked the tests started (its Claude Code is the stand-in)")


@pytest.fixture
def cleanup(arena):
    """Runs to delete after the test, even when it fails."""
    runs: list[str] = []
    yield runs
    for r in runs:
        try:
            arena.delete_run(r)
        except Exception:  # noqa: BLE001
            pass
