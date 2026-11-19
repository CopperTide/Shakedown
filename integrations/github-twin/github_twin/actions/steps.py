"""Running one job's steps outside the twin's lock: the commit checked out into a folder of its own,
each `run` step in its container, a build with `docker build`, a deploy handed to the world; each
step's result and log recorded through the worker as it ends."""
import io
import shutil
import tarfile
import uuid
from pathlib import Path

from ..gitcmd import run as git
from . import jobs
from .jobs import Ticket
from .runner import KILLED, Limits, WORKSPACE, build_argv, image_ref, job_argv
from .workflow import Step

CHECKOUT, BUILD, DEPLOY = "actions/checkout@v4", "shakedown/build@v1", "shakedown/deploy@v1"


def env_of(t: Ticket, step: Step) -> dict:
    """What a step's container is told: GitHub's variables, then the workflow's, job's and step's env."""
    branch = t.ref.rsplit("/", 1)[-1] if t.ref.startswith("refs/heads/") else t.ref
    base = {"CI": "true", "GITHUB_ACTIONS": "true", "HOME": "/tmp", "RUNNER_OS": "Linux",
            "GITHUB_WORKSPACE": WORKSPACE, "GITHUB_REPOSITORY": t.repo, "GITHUB_SHA": t.sha, "GITHUB_REF": t.ref,
            "GITHUB_REF_NAME": branch, "GITHUB_EVENT_NAME": t.event, "GITHUB_RUN_ID": str(t.run_id),
            "GITHUB_RUN_NUMBER": str(t.run_number), "GITHUB_RUN_ATTEMPT": str(t.attempt), "GITHUB_JOB": t.job.key,
            "GITHUB_ACTOR": t.actor}
    return {**base, **t.env, **t.job.env, **step.env}


def check_out(repo_dir: Path, sha: str, into: Path) -> None:
    """The files of commit `sha` (no `.git`) in a fresh folder."""
    shutil.rmtree(into, ignore_errors=True)
    into.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(git(repo_dir, "archive", "--format=tar", sha))) as tar:
        tar.extractall(into, filter="data")


class JobSteps:
    """One claimed job run by `worker` (its Docker, settings, lock and stop)."""

    def __init__(self, worker, t: Ticket):
        self.w, self.t = worker, t
        self.checkout = worker.work / str(t.job_id)

    def cancelled(self) -> bool:
        return self.w.stopping or self.w.read(jobs.is_cancelled, self.t.run_id)

    def run(self) -> str | None:
        """The job's conclusion; None when the instance stopped under it (it runs again on restore)."""
        try:
            check_out(self.w.repo_dir(self.t.repo), self.t.sha, self.checkout)
            return self._steps()
        finally:
            shutil.rmtree(self.checkout, ignore_errors=True)

    def _steps(self) -> str | None:
        outcome = "success"
        for number, step in enumerate(self.t.job.steps, 1):
            if outcome != "success":
                break
            self.w.apply(jobs.step_started, self.t.job_id, number)
            conclusion, log = self._step(number, step)
            if self.w.stopping:
                return None
            self.w.apply(jobs.step_finished, self.t.job_id, number, conclusion, log)
            outcome = conclusion
        return outcome

    def _step(self, number: int, step: Step) -> tuple[str, str]:
        if step.uses == CHECKOUT:
            return "success", f"Checked out {self.t.repo}@{self.t.sha} into {WORKSPACE}\n"
        if step.uses == BUILD:
            return self._build(step)
        if step.uses == DEPLOY:
            return self._deploy(step)
        name = f"ghtwin-{uuid.uuid4().hex[:12]}-{self.t.job_id}-{number}"
        argv = job_argv(self.t.job, step, self.checkout, env_of(self.t, step), Limits.from_config(self.w.settings), name)
        return self._docker(argv)

    def _docker(self, argv: list) -> tuple[str, str]:
        done = self.w.docker.run(argv, timeout_s=float(self.w.settings.get("step_timeout_s", 600)),
                                 cancelled=self.cancelled)
        if done.code == 0:
            return "success", done.output
        if done.code == KILLED and self.cancelled():
            return "cancelled", done.output
        return "failure", done.output + f"Error: Process completed with exit code {done.code}.\n"

    def _build(self, step: Step) -> tuple[str, str]:
        argv = build_argv(self.checkout, step.with_.get("context", "."), self.w.settings.get("image_prefix", ""),
                          step.with_["image"], self.t.sha)
        if argv is None:
            return "failure", f"the build context {step.with_.get('context')} leaves the checkout\n"
        return self._docker(argv)

    def _deploy(self, step: Step) -> tuple[str, str]:
        service = step.with_["service"]
        image = image_ref(self.w.settings.get("image_prefix", ""), step.with_["image"], self.t.sha)
        timeout_s = float(self.w.settings.get("deploy_timeout_s", 300))
        self.w.apply(jobs.deploy_waiting, self.t, service, image)
        while not self.w.wait():
            state = self.w.apply(jobs.deploy_poll, self.t.job_id, int(timeout_s * 1000))
            if state[0] == "done":
                ok, error = state[1], state[2]
                return ("success", f"deployed {image} to {service}\n") if ok else (
                    "failure", f"deploy failed: {error or 'no reason given'}\n")
            if state[0] == "timed_out":
                return "failure", f"deploy timed out after {timeout_s:g} s\n"
            if state[0] == "cancelled":
                return "cancelled", "the run was cancelled while it waited for the deploy\n"
        return "cancelled", ""
