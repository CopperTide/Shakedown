"""The only way this package talks to RunPod: ``runpodctl`` 3.0.1, JSON on stdout. Commands used:
``gpu list --include-unavailable``, ``pod create|get|start|stop|delete``, ``billing pods``, ``user``.
Tests use a fake runner; nothing in the tests runs runpodctl."""

from __future__ import annotations

import json
import subprocess
from typing import Any, Protocol


class RunpodError(Exception):
    """runpodctl failed; the message is its ``error`` (never a key: runpodctl prints none)."""


class Runner(Protocol):
    def run(self, argv: list[str]) -> str: ...


class RealRunner:
    def run(self, argv: list[str]) -> str:
        p = subprocess.run(["runpodctl", *argv], capture_output=True, text=True, timeout=180)
        if p.returncode != 0:
            raise RunpodError(_error(p.stderr) or f"runpodctl {argv[0]} exited {p.returncode}")
        return p.stdout


def _error(stderr: str) -> str:
    for line in stderr.splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if isinstance(d, dict) and d.get("error"):
            return f"{d['error']} ({d.get('code', 'error')})"
    return stderr.strip()[-300:]


def call(runner: Runner, *argv: str) -> Any:
    """``runpodctl <argv>`` as JSON."""
    return json.loads(runner.run(list(argv)))
