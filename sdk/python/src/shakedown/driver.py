"""A driver: the few lines that put your own agent code in a Shakedown run.

    from shakedown import driver

    with driver.run() as run:
        planner = run.agent("planner", provider="openai", model="gpt-4.1-mini")
        my_graph.invoke({"ticket": run.ticket}, tools=..., model=...)   # your code, unchanged

When Shakedown starts the driver itself (a Code connection), it sets ``SHAKEDOWN_URL``, ``SHAKEDOWN_RUN``,
``SHAKEDOWN_BRANCH``, ``SHAKEDOWN_TASK`` (and ``SHAKEDOWN_FORK_OF`` on a fork) and :func:`run` joins that
branch. Run by hand, it starts a run of its own. When the block ends the agents it registered are
ended and the run is finished, so Shakedown scores the world. If the block raises, the run still
gets scored, ends ``failed`` with the error, and the exception goes on up. If the run had already
ended when the block ends (its world time window or its spend limit ended it), that is said on
stderr and the block ends normally.
"""

from __future__ import annotations

import os
import sys
from contextlib import contextmanager
from typing import Iterator, Optional

from .client import DEFAULT_URL, AgentEndpoint, Branch, Shakedown
from .errors import Conflict
from .models import Event, Task
from .settings import setting


class DriverRun:
    """The run a driver drives: its branch, its task and the agents it registered."""

    def __init__(self, shakedown: Shakedown, branch: Branch, task: Task, fork_of: Optional[str] = None):
        self.shakedown = shakedown
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
        read_only: bool = False,
    ) -> AgentEndpoint:
        """The agent ``id`` on this run: registered the first time, the same endpoint after. A
        ``read_only`` agent's writes are refused (its tools and its requests through the proxy)."""
        if id not in self._agents:
            kw = {k: v for k, v in {"provider": provider, "model": model, "name": name,
                                    "read_only": read_only}.items() if v}
            self._agents[id] = self.branch.register(id, **kw)
        return self._agents[id]

    @property
    def agents(self) -> list[AgentEndpoint]:
        """The agents registered so far, in order."""
        return list(self._agents.values())

    def clock(self) -> dict:
        """The run's clock and the world's time (see :meth:`Branch.clock`)."""
        return self.branch.clock()

    def history(self, agent_id: str) -> list[Event]:
        """On a fork, what ``agent_id`` did before the fork point; empty otherwise."""
        if not self.is_fork:
            return []
        if self._start_seq is None:
            self._start_seq = self.branch.record().start_seq or 0
        events = self.branch.record().events
        return [e for e in events if e.agent == agent_id and e.seq < self._start_seq]

    def _end_agents(self, ok: bool, error: Optional[str] = None) -> None:
        for a in self._agents.values():
            if not a.ended:
                a.end(ok=ok, error=error)


@contextmanager
def run(task: Optional[str] = None, url: Optional[str] = None) -> Iterator[DriverRun]:
    """Joins the run Shakedown started this driver for (``SHAKEDOWN_BRANCH``), or starts one of
    ``task`` (default ``SHAKEDOWN_TASK``, else the benchmark's first task). Finishes it at the end."""
    sd = Shakedown(url or setting("SHAKEDOWN_URL") or DEFAULT_URL)
    branch_id = setting("SHAKEDOWN_BRANCH")
    task_id = task or setting("SHAKEDOWN_TASK")
    if branch_id:
        t = sd.task(task_id) if task_id else None
        branch = Branch(sd, branch_id, setting("SHAKEDOWN_RUN", ""), task_id=task_id)
        if t is None:
            t = sd.task(branch.record().run_start["task_id"])
    else:
        t = sd.task(task_id) if task_id else sd.tasks()[0]
        branch = sd.external_run(t)
    dr = DriverRun(sd, branch, t, fork_of=setting("SHAKEDOWN_FORK_OF") or None)
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
        _finish(dr)
    finally:
        sd.close()


def _finish(dr: DriverRun) -> None:
    """Ends the agents and finishes the run; a run that had already ended is said on stderr."""
    try:
        dr._end_agents(ok=True)
    except Conflict:
        pass  # the run ended meanwhile: finishing says so
    try:
        dr.branch.finish()
    except Conflict as e:
        print(f"shakedown: the run had already ended ({e.message})", file=sys.stderr)
