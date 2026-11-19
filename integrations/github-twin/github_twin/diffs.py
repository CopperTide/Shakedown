"""The files two commits differ in, as GitHub lists them (status, line counts, the patch)."""
from dataclasses import dataclass

from .gitcmd import EMPTY_TREE, run
from .repos import Repos

STATUS = {"A": "added", "D": "removed", "M": "modified", "R": "renamed", "C": "copied", "T": "changed"}


@dataclass(frozen=True)
class FileChange:
    filename: str
    status: str
    additions: int
    deletions: int
    sha: str
    patch: str | None
    previous_filename: str | None = None


def _name_status(repos: Repos, name: str, a: str, b: str) -> list:
    out = run(repos.path(name), "diff", "--no-color", "--name-status", "-M", "-z", "--no-abbrev", a, b).decode()
    fields, rows, i = out.split("\x00"), [], 0
    while i < len(fields) - 1:
        code = fields[i][0]
        if code in "RC":
            rows.append((code, fields[i + 2], fields[i + 1]))
            i += 3
        else:
            rows.append((code, fields[i + 1], None))
            i += 2
    return rows


def _numstat(repos: Repos, name: str, a: str, b: str, path: str) -> tuple[int, int]:
    out = repos.git(name, "diff", "--numstat", a, b, "--", path).split()
    return (0, 0) if not out or out[0] == "-" else (int(out[0]), int(out[1]))


def _patch(repos: Repos, name: str, a: str, b: str, paths: list) -> str | None:
    out = repos.git(name, "diff", "--no-color", "-M", a, b, "--", *paths)
    hunks = out.find("\n@@")
    return out[hunks + 1:].rstrip("\n") if hunks >= 0 else None


def _blob_sha(repos: Repos, name: str, commit: str, path: str) -> str:
    entries = repos.tree(name, commit, path)
    return entries[0].sha if entries else "0" * 40


def changed_files(repos: Repos, name: str, base: str | None, head: str) -> list:
    """`base` None: the files `head` adds to an empty tree."""
    a = base or EMPTY_TREE
    out = []
    for code, path, old in _name_status(repos, name, a, head):
        adds, dels = _numstat(repos, name, a, head, path)
        sha = _blob_sha(repos, name, base, path) if code == "D" and base else _blob_sha(repos, name, head, path)
        patch = _patch(repos, name, a, head, [old, path] if old else [path])
        out.append(FileChange(path, STATUS.get(code, "modified"), adds, dels, sha, patch, old))
    return out


def full_diff(repos: Repos, name: str, base: str, head: str) -> str:
    return repos.git(name, "diff", "--no-color", "-M", "--full-index", base, head)
