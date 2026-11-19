"""A driver: the few lines that put your own agent code in a Shakedown run.

    from arena import driver

    with driver.run() as run:
        planner = run.agent("planner", provider="openai", model="gpt-4.1-mini")
        my_graph.invoke({"ticket": run.ticket}, tools=..., model=...)   # your code, unchanged

When Shakedown starts the driver itself (a Code connection), it sets ``ARENA_URL``, ``ARENA_RUN``,
``ARENA_BRANCH``, ``ARENA_TASK`` (and ``ARENA_FORK_OF`` on a fork) and :func:`run` joins that
branch. Run by hand, it starts a run of its own. When the block ends the agents it registered are
ended and the run is finished, so Shakedown scores the world. If the block raises, the run still
gets scored, ends ``failed`` with the error, and the exception goes on up.
"""

from __future__ import annotations

import os
from contextlib import contextmanager
from typing import Iterator, Optional

from .client import DEFAULT_URL, AgentEndpoint, Arena, Branch
from .models import Event, Task


class DriverRun:
    """The run a driver drives: its branch, its task and the agents it registered."""

    def __init__(self, arena: Arena, branch: Branch, task: Task, fork_of: Optional[str] = None):
        self.arena = arena
        self.branch = branch
        self.task = task
        #: The branch this one was forked from, when Shakedown relaunched the driver on a fork.
        self.fork_of = fork_of
        self._agents: dict[str, AgentEndpoint] = {}
        self._start_seq: Optional[int] = None

    @property
    def ticket(self) -> str:
        """What the team is asked to do: the task's ticket and the benchmark's instructions."""
        t = self.task
        return t.ticket + (f"\n\n{t.instructions}" if t.instructions else "")

    @property
    def is_fork(self) -> bool:
        return self.fork_of is not None

    def agent(
        self,
        id: str,
        *,
        provider: Optional[str] = None,
        model: Optional[str] = None,
        name: Optional[str] = None,
    ) -> AgentEndpoint:
        """The agent ``id`` on this run: registered the first time, the same endpoint after."""
        if id not in self._agents:
            kw = {k: v for k, v in {"provider": provider, "model": model, "name": name}.items() if v}
            self._agents[id] = self.branch.register(id, **kw)
        return self._agents[id]

    def history(self, agent_id: str) -> list[Event]:
        """On a fork, what ``agent_id`` did before the fork point; empty otherwise."""
        if not self.is_fork:
            return []
        if self._start_seq is None:
            self._start_seq = self.branch.record().start_seq or 0
        events = self.branch.events()
        return [e for e in events if e.agent == agent_id and e.seq < self._start_seq]

    def _end_agents(self, ok: bool, error: Optional[str] = None) -> None:
        for a in self._agents.values():
            if not a.ended:
                a.end(ok=ok, error=error)


@contextmanager
def run(task: Optional[str] = None, url: Optional[str] = None) -> Iterator[DriverRun]:
    """Joins the run Shakedown started this driver for (``ARENA_BRANCH``), or starts one of
    ``task`` (default ``ARENA_TASK``, else the benchmark's first task). Finishes it at the end."""
    arena = Arena(url or os.environ.get("ARENA_URL") or DEFAULT_URL)
    branch_id = os.environ.get("ARENA_BRANCH")
    task_id = task or os.environ.get("ARENA_TASK")
    if branch_id:
        t = arena.task(task_id) if task_id else None
        branch = Branch(arena, branch_id, os.environ.get("ARENA_RUN", ""), task_id=task_id)
        if t is None:
            t = arena.task(branch.record().run_start["task_id"])
    else:
        t = arena.task(task_id) if task_id else arena.tasks()[0]
        branch = arena.external_run(t)
    dr = DriverRun(arena, branch, t, fork_of=os.environ.get("ARENA_FORK_OF") or None)
    try:
        yield dr
    except BaseException as exc:
        # Best effort: the driver's own exception is the one that matters.
        try:
            dr._end_agents(ok=False, error=repr(exc))
            branch.finish(error=f"driver raised {type(exc).__name__}: {exc}")
        except Exception:  # noqa: BLE001
            pass
        raise
    else:
        dr._end_agents(ok=True)
        branch.finish()
    finally:
        arena.close()
