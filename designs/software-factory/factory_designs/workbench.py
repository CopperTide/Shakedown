"""The workbench: where an agent's tools run. Each command runs in a fresh container with no
network, a read-only root and the agent's workspace mounted at ``/work``; its only way out is a
unix socket to Shakedown's recording proxy (see :mod:`factory_designs.bridge`), so every request
the agent's ``git``, ``gh``, ``curl`` or SDKs make is recorded, and nothing else is reachable.

Answers are text, never exceptions: ``exit <code>`` and the output's tail, or ``error: …``.
"""

from __future__ import annotations

import os
import posixpath
import re
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Mapping, Optional, Protocol
from urllib.parse import urlsplit, urlunsplit

from .bridge import bridge_socket
from .settings import Settings

#: Where the container's proxy listens (the image's ``bridge-then`` relays it to the socket).
BRIDGE_HOSTPORT = "127.0.0.1:3128"
#: Where the proxy's socket is mounted in the container.
SOCKET_IN = "/run/arena-proxy.sock"
PROXY_VARS = ("HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ARENA_PROXY")
CUT = "[…cut]"
_PUSH = re.compile(r"\bgit\b.*\bpush\b", re.S)
_CD = re.compile(r"(?:\bcd|\bgit\s+-C)\s+(\S+)")


@dataclass(frozen=True)
class Completed:
    code: int
    out: str


class Docker(Protocol):
    def run(self, argv: list[str], timeout_s: int) -> Completed: ...


class RealDocker:
    """Runs ``argv`` (a ``docker run …``), stdout and stderr together. The command itself is
    bounded inside the container; this timeout is the backstop for Docker hanging."""

    def run(self, argv: list[str], timeout_s: int) -> Completed:
        try:
            p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout_s + 30)
        except subprocess.TimeoutExpired as e:
            return Completed(124, f"{e.stdout or ''}docker did not answer in {timeout_s + 30} s")
        return Completed(p.returncode, p.stdout + p.stderr)


def container_env(env: Mapping[str, str]) -> dict[str, str]:
    """``env`` for the container: proxy URLs point at the bridge (``127.0.0.1:3128``) and keep
    their userinfo, the agent's ticket. Everything else passes unchanged."""
    return {k: (_at_bridge(v) if k in PROXY_VARS else v) for k, v in env.items()}


def _at_bridge(url: str) -> str:
    u = urlsplit(url)
    userinfo = u.netloc.rpartition("@")[0]
    netloc = f"{userinfo}@{BRIDGE_HOSTPORT}" if userinfo else BRIDGE_HOSTPORT
    return urlunsplit((u.scheme, netloc, u.path, u.query, u.fragment))


def cut_tail(text: str, limit: int) -> str:
    return text if len(text) <= limit else CUT + text[-limit:]


class PushGate:
    """A repository's pushes wait until its tests passed after its last ``write_file``."""

    def __init__(self) -> None:
        self._tick = 0
        self._written: dict[str, int] = {}
        self._passed: dict[str, int] = {}

    def _now(self) -> int:
        self._tick += 1
        return self._tick

    def wrote(self, repo: str) -> None:
        self._written[repo] = self._now()

    def passed(self, repo: str) -> None:
        self._passed[repo] = self._now()

    def allows(self, repo: str) -> bool:
        passed = self._passed.get(repo)
        return passed is not None and passed > self._written.get(repo, 0)


class Workbench:
    """The tools of agent ``agent_id``, run in containers on its workspace ``root / agent_id``
    with its proxy variables ``env``. With ``push_gate``, ``git push`` waits for passing tests."""

    def __init__(self, agent_id: str, env: Mapping[str, str], root: Path, socket: Path,
                 settings: Settings, docker: Docker, push_gate: bool = False):
        self.agent_id = agent_id
        self.env = dict(env)
        self.workspace = (root / agent_id).resolve()
        self.workspace.mkdir(parents=True, exist_ok=True)
        self.socket = socket
        self.settings = settings
        self.docker = docker
        self.gate: Optional[PushGate] = PushGate() if push_gate else None

    # ------------------------------------------------------------------ tools

    def shell(self, cmd: str, cwd: str = ".") -> str:
        """Runs ``cmd`` with ``sh -euc`` in ``cwd`` (relative to the workspace)."""
        if self._inside(cwd) is None:
            return self._outside(cwd)
        if self.gate and _PUSH.search(cmd):
            repo = self._repo(cwd, cmd)
            if not self.gate.allows(repo):
                return f"error: run the tests (run_tests) in {repo} and see them pass before pushing"
        return self._text(self._exec(cmd, cwd))

    def run_tests(self, cwd: str = ".", args: str = "") -> str:
        """Runs ``python -m pytest -q <args>`` in ``cwd``; a pass opens the push gate there."""
        if self._inside(cwd) is None:
            return self._outside(cwd)
        done = self._exec(f"python -m pytest -q {args}".strip(), cwd)
        if self.gate and done.code == 0:
            self.gate.passed(self._repo(cwd))
        return self._text(done)

    def read_file(self, path: str) -> str:
        p = self._inside(path)
        if p is None:
            return self._outside(path)
        try:
            return cut_tail(p.read_text(errors="replace"), self.settings.tool_output_chars)
        except OSError as e:
            return f"error: {path}: {e.strerror or e}"

    def write_file(self, path: str, text: str) -> str:
        p = self._inside(path)
        if p is None or p == self.workspace:
            return self._outside(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
        if self.gate:
            self.gate.wrote(self._repo(path))
        return f"wrote {len(text)} characters to {path}"

    # ------------------------------------------------------------------ the container

    def argv(self, cmd: str, cwd: str) -> list[str]:
        """The ``docker run`` command line for ``cmd`` in ``cwd``."""
        s = self.settings
        workdir = posixpath.normpath(posixpath.join("/work", self._relative(cwd)))
        head = ["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                "--tmpfs", "/tmp:size=256m", "--memory", "1g", "--pids-limit", "256",
                "--user", "1000:1000", "-v", f"{self.workspace}:/work",
                "-v", f"{self.socket}:{SOCKET_IN}", "-w", workdir]
        mounts = [x for f in self._files() for x in ("-v", f"{f}:{f}:ro")]
        envs = [x for k, v in container_env(self.env).items() for x in ("-e", f"{k}={v}")]
        command = ["timeout", str(s.command_timeout_s), "sh", "-euc", cmd]
        return head + mounts + envs + [s.workbench_image] + command

    def _files(self) -> list[str]:
        """Env values that name an existing file (the proxy's CA certificate), once each."""
        seen = [v for v in self.env.values() if v.startswith("/") and Path(v).is_file()]
        return list(dict.fromkeys(seen))

    def _exec(self, cmd: str, cwd: str) -> Completed:
        return self.docker.run(self.argv(cmd, cwd), self.settings.command_timeout_s)

    def _text(self, done: Completed) -> str:
        return f"exit {done.code}\n{cut_tail(done.out, self.settings.tool_output_chars)}"

    # ------------------------------------------------------------------ paths

    def _relative(self, path: str) -> str:
        """``path`` relative to the workspace; ``/work/…`` (the container's view) maps to it."""
        if path == "/work" or path.startswith("/work/"):
            return path[len("/work"):].lstrip("/") or "."
        return path

    def _inside(self, path: str) -> Optional[Path]:
        """The resolved path, or None when it is outside the workspace (symlinks followed)."""
        rel = self._relative(path)
        if rel.startswith("/"):
            return None
        p = (self.workspace / rel).resolve()
        return p if p == self.workspace or self.workspace in p.parents else None

    def _outside(self, path: str) -> str:
        return f"error: {path} is outside your workspace"

    def _repo(self, cwd: str, cmd: str = "") -> str:
        """The repository a command works in: the top-level folder of ``cwd``, else of the first
        ``cd <dir>`` / ``git -C <dir>`` in ``cmd``."""
        top = Path(posixpath.normpath(self._relative(cwd))).parts[:1]
        if top and top[0] != ".":
            return top[0]
        m = _CD.search(cmd)
        if m:
            return Path(posixpath.normpath(self._relative(m.group(1)))).parts[0]
        return "."


def benches(settings: Settings, push_gate: bool = False) -> Callable[[str, Any], "Workbench"]:
    """A design's workbench maker for a real run: one workspace per agent under
    ``FACTORY_WORKSPACES`` (default a new temporary folder), on Docker, bridged to the proxy."""
    root = Path(os.environ.get("FACTORY_WORKSPACES") or tempfile.mkdtemp(prefix="shakedown-ws-"))
    socket, docker = bridge_socket(), RealDocker()

    def make(agent_id: str, endpoint: Any) -> Workbench:
        return Workbench(agent_id, endpoint.env, root, socket, settings, docker, push_gate)

    return make
