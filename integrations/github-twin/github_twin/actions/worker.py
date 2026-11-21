"""One Actions worker per instance: it takes the next job (oldest run first, one at a time), runs
its steps outside the twin's lock, and applies each result under the lock in its own transaction."""
import shutil
import sys
import threading
import traceback

from ..repos import Repos
from ..view import View
from . import jobs
from .runs import settings
from .steps import JobSteps


class Worker:
    def __init__(self, ctx, docker, poll_s: float = 0.1):
        self.ctx, self.docker, self.poll_s = ctx, docker, poll_s
        self.work = ctx.files.parent / "actions"
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._loop, name="github-actions", daemon=True)

    @property
    def settings(self) -> dict:
        return settings(self.ctx.config)

    @property
    def stopping(self) -> bool:
        return self._stop.is_set()

    def wait(self) -> bool:
        """Sleep one poll; True when the instance is stopping."""
        return self._stop.wait(self.poll_s)

    def start(self) -> "Worker":
        self._thread.start()
        return self

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=60)
        shutil.rmtree(self.work, ignore_errors=True)

    def repo_dir(self, full_name: str):
        return Repos(self.ctx.files).path(full_name)

    def apply(self, fn, *args):
        """`fn(view, *args)` under the lock in one transaction; the world changed if it wrote."""
        with self.ctx.lock, self.ctx.atomic() as db:
            before, queued = db.total_changes, len(self.ctx.notices)
            out = fn(View(self.ctx), *args)
            if db.total_changes != before or len(self.ctx.notices) != queued:
                self.ctx.changed = True
            return out

    def read(self, fn, *args):
        with self.ctx.lock:
            return fn(View(self.ctx), *args)

    def _loop(self) -> None:
        while not self.wait():
            try:
                self._round()
            except Exception:   # one bad round is reported, not the end of the runs
                traceback.print_exc(file=sys.stderr)

    def _round(self) -> None:
        while not self.stopping:
            ticket = self.apply(jobs.claim)
            if ticket is None:
                return
            conclusion = self._run(ticket)
            if conclusion is None:
                return
            self.apply(jobs.job_done, ticket.job_id, conclusion)

    def _run(self, ticket) -> str | None:
        try:
            return JobSteps(self, ticket).run()
        except Exception as e:   # the job fails with why; the worker goes on
            if self.stopping:
                return None
            self.apply(jobs.note, ticket.job_id, f"the job could not run: {e}\n")
            return "failure"
