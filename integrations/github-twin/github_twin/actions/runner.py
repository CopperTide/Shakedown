"""How a job's steps reach Docker: the bounded `docker run` of a `run` step (no network, no pull,
memory, CPU and process caps, the twin's own user so the checkout stays removable), the `docker build`
of a build step, and a Docker that kills a step past its time or when its run is cancelled."""
import os
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Protocol

from .workflow import Job, Step

WORKSPACE = "/github/workspace"
LABEL = "shakedown.github-twin=job"
MAX_OUTPUT = 256 * 1024
TIMED_OUT, KILLED = 124, 137


@dataclass(frozen=True)
class Completed:
    code: int
    output: str


class Docker(Protocol):
    def run(self, argv: list[str], timeout_s: float | None = None,
            cancelled: Callable[[], bool] = lambda: False) -> Completed: ...


@dataclass(frozen=True)
class Limits:
    memory: str = "384m"
    cpus: str = "1"
    pids: int = 256
    user: str = ""

    @staticmethod
    def from_config(actions: dict) -> "Limits":
        return Limits(str(actions.get("job_memory", "384m")), str(actions.get("job_cpus", "1")),
                      int(actions.get("job_pids", 256)), f"{os.getuid()}:{os.getgid()}")


def job_argv(job: Job, step: Step, checkout: Path, env: dict, limits: Limits, name: str) -> list[str]:
    """`docker run` of one `run` step of `job` (whose `image` is resolved) in `checkout`."""
    caps = ["--memory", limits.memory, "--memory-swap", limits.memory, "--cpus", limits.cpus,
            "--pids-limit", str(limits.pids), *(["--user", limits.user] if limits.user else [])]
    envs = [a for k, v in env.items() for a in ("-e", f"{k}={v}")]
    return ["docker", "run", "--rm", "--pull", "never", "--network", "none", *caps, "--name", name,
            "--label", LABEL, "-v", f"{checkout}:{WORKSPACE}", "-w", WORKSPACE, *envs,
            job.image, "sh", "-euc", step.run]


def build_argv(checkout: Path, context: str, prefix: str, image: str, sha: str) -> list[str] | None:
    """`docker build` of `context` (inside `checkout`) as `<prefix>/<image>:<sha>`; None when the
    context leaves the checkout."""
    root = Path(checkout).resolve()
    ctx = (root / (context or ".")).resolve()
    if ctx != root and root not in ctx.parents:
        return None
    return ["docker", "build", "--pull=false", "--network", "none", "-t", image_ref(prefix, image, sha), str(ctx)]


def image_ref(prefix: str, image: str, sha: str) -> str:
    return f"{prefix}/{image}:{sha}" if prefix else f"{image}:{sha}"


def _tail(raw: bytes, limit: int) -> str:
    if len(raw) <= limit:
        return raw.decode(errors="replace")
    return f"[output cut to its last {limit} bytes]\n" + raw[-limit:].decode(errors="replace")


def _name(argv: list[str]) -> str | None:
    return argv[argv.index("--name") + 1] if "--name" in argv else None


class RealDocker:
    """The `docker` CLI. A step past `timeout_s` (wall seconds) or whose run is cancelled is
    removed (`docker rm -f` of its container) and its client killed."""

    def __init__(self, binary: str = "docker", poll_s: float = 0.25, max_output: int = MAX_OUTPUT):
        self.binary, self.poll_s, self.max_output = binary, poll_s, max_output

    def run(self, argv, timeout_s=None, cancelled=lambda: False) -> Completed:
        with tempfile.TemporaryFile() as out:
            proc = subprocess.Popen([self.binary, *argv[1:]], stdout=out, stderr=subprocess.STDOUT,
                                    stdin=subprocess.DEVNULL)
            code, note = self._wait(proc, argv, timeout_s, cancelled)
            out.seek(0)
            return Completed(code, _tail(out.read(), self.max_output) + note)

    def _wait(self, proc, argv, timeout_s, cancelled) -> tuple[int, str]:
        deadline = None if timeout_s is None else time.monotonic() + timeout_s
        while True:
            try:
                return proc.wait(self.poll_s), ""
            except subprocess.TimeoutExpired:
                pass
            if deadline is not None and time.monotonic() >= deadline:
                self._kill(proc, argv)
                return TIMED_OUT, f"the step timed out after {timeout_s} s\n"
            if cancelled():
                self._kill(proc, argv)
                return KILLED, "the run was cancelled\n"

    def _kill(self, proc, argv) -> None:
        name = _name(argv)
        if name is not None:
            subprocess.run([self.binary, "rm", "-f", name], capture_output=True, timeout=60)
        proc.kill()
        proc.wait(30)
