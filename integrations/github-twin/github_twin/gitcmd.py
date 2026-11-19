"""Running git for the twin: a clean environment (no user or system config), dates and people given."""
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path

EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"


class GitError(Exception):
    pass


@dataclass(frozen=True)
class Person:
    """Who authored or committed, and when (world ms)."""
    name: str
    email: str
    at_ms: int

    def env(self, role: str) -> dict:
        return {f"GIT_{role}_NAME": self.name, f"GIT_{role}_EMAIL": self.email,
                f"GIT_{role}_DATE": f"@{self.at_ms // 1000} +0000"}


def clean_env(extra: dict | None = None) -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull, "GIT_TERMINAL_PROMPT": "0",
                "LC_ALL": "C"})
    env.update(extra or {})
    return env


def run(git_dir: Path, *args: str, input: bytes | None = None, env: dict | None = None,
        check: bool = True) -> bytes:
    """`git --git-dir <git_dir> <args>`; its stdout. Raises GitError when it fails and `check`."""
    r = subprocess.run(["git", f"--git-dir={git_dir}", *args], input=input, env=clean_env(env),
                       capture_output=True, timeout=120)
    if check and r.returncode != 0:
        raise GitError(f"git {' '.join(args)}: {r.stderr.decode(errors='replace').strip()}")
    return r.stdout if r.returncode == 0 else b""


def ok(git_dir: Path, *args: str) -> bool:
    """True when `git <args>` exits 0 (a yes/no question such as `merge-base --is-ancestor`)."""
    return subprocess.run(["git", f"--git-dir={git_dir}", *args], env=clean_env(), capture_output=True,
                          timeout=120).returncode == 0
