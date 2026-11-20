"""Start a shaked of your own as a subprocess (``Shakedown.local``)."""

from __future__ import annotations

import os
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from typing import Any, Mapping, Optional, Sequence

from .client import Shakedown
from .errors import ShakedownError, ConnectionFailed
from .settings import setting


class StartFailed(ShakedownError):
    """shaked exited or didn't answer while starting; the message ends with its log."""


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


#: The program's names, newest first: ``arenad`` is what shaked was called until 2026-11-19, so an
#: older build is found too.
NAMES = ("shaked", "arenad")


def find_shaked(shaked: Optional[str], bin_dir: Optional[str]) -> str:
    """``shaked`` as given, else ``shaked`` (or ``arenad``) in ``bin_dir``, else in ``$SHAKEDOWN_BIN_DIR``,
    else on PATH."""
    dirs = [d for d in (bin_dir, setting("SHAKEDOWN_BIN_DIR")) if d]
    cands = [shaked] + [os.path.join(d, n) for d in dirs for n in NAMES] + [shutil.which(n) for n in NAMES]
    for cand in cands:
        if cand and os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    raise StartFailed(
        "no shaked binary: pass shaked=<path> or bin_dir=<folder>, or set SHAKEDOWN_BIN_DIR "
        "(build it with `cd core && cargo build --release -p shaked`)"
    )


class LocalShakedown(Shakedown):
    """A :class:`Shakedown` connected to a shaked this object started. Use it in a ``with`` block (or
    call :meth:`close`) so the process stops.

    Args:
        benchmark: the benchmark package new runs use (``SHAKEDOWN_BENCHMARK``), e.g. ``"retail"``.
        packages: folders whose subfolders hold ``shakedown-package.toml`` (``SHAKEDOWN_PACKAGES``); default:
            shaked's own (``integrations`` and ``benchmarks`` of its checkout).
        pg_url: the Postgres server for shaked's records (``SHAKEDOWN_PG_URL``); default shaked's
            (``postgres://arena:arena@127.0.0.1:5544/postgres``, ``deploy/compose.yml``).
        saves: where saves go (``SHAKEDOWN_SAVES``); default a temporary folder removed on close.
        bin_dir: what ``${SHAKEDOWN_BIN_DIR}`` means in package manifests, and where to find shaked.
        shaked: the shaked binary (default: found in ``bin_dir``, ``$SHAKEDOWN_BIN_DIR`` or PATH).
            ``arenad=`` is its old name.
        port: the port (default: a free one).
        max_db_bytes: the saves' disk limit (``SHAKEDOWN_MAX_DB_BYTES``).
        cors_origin: web origins allowed to call the API from a browser (``SHAKEDOWN_CORS_ORIGIN``).
        env: more environment variables for shaked (model provider keys, ``SHAKEDOWN_LOCAL_MODELS``, ...);
            shaked otherwise inherits this process's environment.
        log: a file for shaked's output (default: a temporary file, ``self.log_path``, removed on
            close; a failed start quotes its end).
        startup_timeout: seconds to wait for shaked to answer.
    """

    def __init__(
        self,
        benchmark: Optional[str] = None,
        packages: Optional[Sequence[str]] = None,
        pg_url: Optional[str] = None,
        saves: Optional[str] = None,
        bin_dir: Optional[str] = None,
        shaked: Optional[str] = None,
        port: Optional[int] = None,
        max_db_bytes: Optional[int] = None,
        cors_origin: Optional[str] = None,
        env: Optional[Mapping[str, str]] = None,
        log: Optional[str] = None,
        startup_timeout: float = 120.0,
        arenad: Optional[str] = None,
        **client: Any,
    ):
        binary = find_shaked(shaked or arenad, bin_dir)
        self.port = port or free_port()
        self._tmp_saves = None
        if saves is None:
            self._tmp_saves = tempfile.mkdtemp(prefix="arena-saves-")
            saves = self._tmp_saves
        settings = {
            "SHAKEDOWN_PORT": str(self.port),
            "SHAKEDOWN_SAVES": os.path.abspath(saves),
            "SHAKEDOWN_BENCHMARK": benchmark,
            "SHAKEDOWN_PACKAGES": packages and os.pathsep.join(os.path.abspath(p) for p in packages),
            "SHAKEDOWN_PG_URL": pg_url,
            "SHAKEDOWN_BIN_DIR": bin_dir and os.path.abspath(bin_dir),
            "SHAKEDOWN_MAX_DB_BYTES": None if max_db_bytes is None else str(max_db_bytes),
            "SHAKEDOWN_CORS_ORIGIN": cors_origin,
        }
        e = {**os.environ, **{k: v for k, v in settings.items() if v}, **(env or {})}
        self._tmp_log = log is None
        if log is None:
            fd, log = tempfile.mkstemp(prefix="shaked-", suffix=".log")
            os.close(fd)
        self.log_path = log
        self._log = open(log, "ab")
        self.process = subprocess.Popen(
            [binary], env=e, stdout=self._log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL
        )
        super().__init__(f"http://127.0.0.1:{self.port}", **client)
        try:
            self._wait_ready(startup_timeout)
        except BaseException:
            self.close()
            raise

    def _wait_ready(self, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise StartFailed(f"shaked exited with {self.process.returncode}:\n{self.log_tail()}")
            try:
                self._http.get("/api/health", timeout=5.0).raise_for_status()
                return
            except Exception:
                time.sleep(0.3)
        raise StartFailed(f"shaked did not answer within {timeout} s:\n{self.log_tail()}")

    def log_tail(self, lines: int = 30) -> str:
        try:
            with open(self.log_path, encoding="utf-8", errors="replace") as f:
                return "".join(f.readlines()[-lines:])
        except OSError:
            return ""

    def close(self) -> None:
        """Stops shaked (ctrl-c, then kill after 15 s) and removes a temporary saves folder and log."""
        super().close()
        p = getattr(self, "process", None)
        if p is not None and p.poll() is None:
            p.send_signal(signal.SIGINT)
            try:
                p.wait(15)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait(5)
        if getattr(self, "_log", None):
            self._log.close()
        if self._tmp_saves:
            shutil.rmtree(self._tmp_saves, ignore_errors=True)
            self._tmp_saves = None
        if getattr(self, "_tmp_log", False):
            try:
                os.remove(self.log_path)
            except OSError:
                pass
            self._tmp_log = False

    def __repr__(self) -> str:
        return f"LocalShakedown({self.url!r}, pid={self.process.pid})"


#: The old names (the SDK was ``arena`` and shaked ``arenad`` until 2026-11-19).
LocalArena = LocalShakedown
find_arenad = find_shaked

__all__ = ["LocalShakedown", "StartFailed", "ConnectionFailed", "free_port", "find_shaked"]
