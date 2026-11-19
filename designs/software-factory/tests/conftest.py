"""Fixtures: a Docker that never runs anything, and a workbench on it."""

from __future__ import annotations

from pathlib import Path
from typing import Callable, Union

import pytest

from factory_designs.settings import load_settings
from factory_designs.workbench import Completed, Workbench

Answer = Union[Completed, Callable[[list[str]], Completed]]


class FakeDocker:
    """Records each ``docker run`` argv and answers from ``script`` (in order, then exit 0 with
    no output), or with ``answer(argv)`` when given a function."""

    def __init__(self, script: Union[list[Completed], Callable[[list[str]], Completed], None] = None):
        self.script = script if script is not None else []
        self.argvs: list[list[str]] = []

    def run(self, argv: list[str], timeout_s: int) -> Completed:
        self.argvs.append(argv)
        if callable(self.script):
            return self.script(argv)
        return self.script.pop(0) if self.script else Completed(0, "")

    @property
    def commands(self) -> list[str]:
        """The shell command of each run (the argv's last item)."""
        return [a[-1] for a in self.argvs]


@pytest.fixture
def settings():
    return load_settings(env={})


@pytest.fixture
def docker():
    return FakeDocker()


@pytest.fixture
def bench(tmp_path: Path, settings, docker) -> Workbench:
    (tmp_path / "ws" / "a").mkdir(parents=True)
    return Workbench("a", {}, tmp_path / "ws", tmp_path / "proxy.sock", settings, docker)
