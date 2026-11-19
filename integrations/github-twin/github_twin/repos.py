"""The twin's bare repositories under one folder (`<owner>/<name>.git`), and the git plumbing the
REST API and the pushes need."""
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path

from .gitcmd import EMPTY_TREE, Person, ok, run

ZERO = "0" * 40


@dataclass(frozen=True)
class RefChange:
    ref: str
    old: str
    new: str


@dataclass(frozen=True)
class CommitInfo:
    sha: str
    tree: str
    parents: list
    author: Person
    committer: Person
    message: str


@dataclass(frozen=True)
class TreeEntry:
    mode: str
    type: str
    sha: str
    size: int | None
    path: str


def full_name(repo) -> str:
    """`owner/name` of a repos row."""
    return f"{repo['owner']}/{repo['name']}"


def is_immutable(path: Path) -> bool:
    """Git objects never change in place (new ones are written beside them); `objects/info` does."""
    parts = Path(path).parts
    return "objects" in parts and "info" not in parts[parts.index("objects"):]


class Repos:
    def __init__(self, root: Path):
        self.root = Path(root)

    def path(self, name: str) -> Path:
        return self.root / f"{name}.git"

    def exists(self, name: str) -> bool:
        return (self.path(name) / "HEAD").is_file()

    def git(self, name: str, *args: str, **kw) -> str:
        return run(self.path(name), *args, **kw).decode()

    def init_from(self, source: Path, name: str, default_branch: str, author: Person) -> str:
        """A bare repo whose one commit holds the files of `source`, on `default_branch`; its sha."""
        p = self.path(name)
        p.parent.mkdir(parents=True, exist_ok=True)
        run(p, "init", "-q", "--bare", f"--initial-branch={default_branch}", str(p))
        run(p, "config", "http.receivepack", "true")
        with tempfile.TemporaryDirectory() as tmp:
            env = {"GIT_INDEX_FILE": os.path.join(tmp, "index"), "GIT_WORK_TREE": str(source)}
            run(p, "add", "-A", ".", env=env)
            tree = run(p, "write-tree", env=env).decode().strip()
        sha = self.commit_tree(name, tree, [], "Initial commit", author, author)
        self.update_ref(name, f"refs/heads/{default_branch}", sha)
        return sha

    # --- refs ---------------------------------------------------------------------------
    def ref(self, name: str, ref: str) -> str | None:
        """The commit a ref (or a branch name, or a sha) names; None when it names none."""
        out = run(self.path(name), "rev-parse", "--verify", "--quiet", "--end-of-options",
                  f"{ref}^{{commit}}", check=False)
        return out.decode().strip() or None

    def refs(self, name: str, prefix: str = "refs/") -> dict:
        out = self.git(name, "for-each-ref", "--format=%(refname) %(objectname)", prefix)
        return dict(line.split(" ", 1) for line in out.splitlines())

    def branches(self, name: str) -> dict:
        return {r[len("refs/heads/"):]: sha for r, sha in self.refs(name, "refs/heads/").items()}

    def update_ref(self, name: str, ref: str, new: str, old: str | None = None) -> None:
        self.git(name, "update-ref", ref, new, *([old] if old is not None else []))

    def delete_ref(self, name: str, ref: str) -> None:
        self.git(name, "update-ref", "-d", ref)

    def set_pull_ref(self, name: str, n: int, sha: str) -> None:
        self.update_ref(name, f"refs/pull/{n}/head", sha)

    def refs_changed(self, name: str, before: dict) -> list:
        after = self.refs(name)
        return [RefChange(r, before.get(r, ZERO), after.get(r, ZERO))
                for r in sorted(before.keys() | after.keys()) if before.get(r) != after.get(r)]

    def is_ancestor(self, name: str, a: str, b: str) -> bool:
        return ok(self.path(name), "merge-base", "--is-ancestor", a, b)

    # --- objects ------------------------------------------------------------------------
    def object_type(self, name: str, sha: str) -> str | None:
        return run(self.path(name), "cat-file", "-t", sha, check=False).decode().strip() or None

    def blob(self, name: str, sha: str) -> bytes:
        return run(self.path(name), "cat-file", "blob", sha)

    def write_blob(self, name: str, data: bytes) -> str:
        return run(self.path(name), "hash-object", "-w", "--stdin", input=data).decode().strip()

    def commit_info(self, name: str, sha: str) -> CommitInfo:
        fmt = "%H%x00%T%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%B"
        f = self.git(name, "show", "-s", f"--format={fmt}", sha, "--").split("\x00")
        return CommitInfo(f[0], f[1], f[2].split(), Person(f[3], f[4], int(f[5]) * 1000),
                          Person(f[6], f[7], int(f[8]) * 1000), f[9].rstrip("\n"))

    def commit_tree(self, name: str, tree: str, parents: list, message: str, author: Person,
                    committer: Person) -> str:
        args = [a for p in parents for a in ("-p", p)]
        env = {**author.env("AUTHOR"), **committer.env("COMMITTER")}
        return run(self.path(name), "commit-tree", tree, *args, input=message.encode(), env=env).decode().strip()

    def tree(self, name: str, treeish: str, path: str = "", recursive: bool = False) -> list:
        """The entries of a tree (`path` inside it); every level with `recursive`."""
        args = ["ls-tree", "-l", "-z", *(["-r", "-t"] if recursive else []), treeish]
        if path:
            args += ["--", path]
        out = run(self.path(name), *args).decode()
        return [_entry(e) for e in out.split("\x00") if e]

    def edit_tree(self, name: str, base: str | None, edits: list) -> str:
        """A new tree: `base` (a tree, or None for empty) with `edits` [(mode, sha or None, path)]."""
        lines = "".join(f"{mode} {sha}\t{path}\n" if sha else f"0 {ZERO}\t{path}\n" for mode, sha, path in edits)
        with tempfile.TemporaryDirectory() as tmp:
            env = {"GIT_INDEX_FILE": os.path.join(tmp, "index")}
            run(self.path(name), "read-tree", base or EMPTY_TREE, env=env)
            run(self.path(name), "update-index", "--index-info", input=lines.encode(), env=env)
            return run(self.path(name), "write-tree", env=env).decode().strip()

    def rev_list(self, name: str, rev: str, path: str = "") -> list:
        return self.git(name, "rev-list", rev, "--", *([path] if path else [])).split()

    def merge_base(self, name: str, a: str, b: str) -> str | None:
        return run(self.path(name), "merge-base", a, b, check=False).decode().strip() or None

    def ahead_behind(self, name: str, base: str, head: str) -> tuple[int, int]:
        behind, ahead = self.git(name, "rev-list", "--left-right", "--count", f"{base}...{head}").split()
        return int(ahead), int(behind)


def _entry(line: str) -> TreeEntry:
    meta, path = line.split("\t", 1)
    mode, typ, sha, size = meta.split()
    return TreeEntry(mode, typ, sha, None if size == "-" else int(size), path)
