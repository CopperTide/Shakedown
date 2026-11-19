"""Whether a pull request merges cleanly, and merging it by merge commit, squash or rebase."""
from dataclasses import dataclass

from .errors import GithubError
from .gitcmd import Person, attempt
from .repos import Repos

MERGEABLE, CONFLICTING = "MERGEABLE", "CONFLICTING"


@dataclass(frozen=True)
class Mergeable:
    state: str
    tree: str | None


def _merge_tree(repos: Repos, name: str, ours: str, theirs: str, base: str | None = None) -> str | None:
    """The merged tree of two commits (`git merge-tree --write-tree`); None on a conflict."""
    args = ["merge-tree", "--write-tree", "--no-messages", *([f"--merge-base={base}"] if base else []), ours, theirs]
    clean, out = attempt(repos.path(name), *args)
    return out.decode().split()[0] if clean else None


def mergeable(repos: Repos, name: str, base: str, head: str) -> Mergeable:
    tree = _merge_tree(repos, name, base, head)
    return Mergeable(MERGEABLE if tree else CONFLICTING, tree)


def _rebase(repos: Repos, name: str, base: str, head: str, by: Person) -> str:
    cur = base
    for sha in reversed(repos.rev_list(name, f"{base}..{head}")):
        info = repos.commit_info(name, sha)
        tree = _merge_tree(repos, name, cur, sha, info.parents[0] if info.parents else None)
        if tree is None:
            raise GithubError(405, "Pull Request is not mergeable")
        cur = repos.commit_tree(name, tree, [cur], info.message, info.author, by)
    return cur


def merge(repos: Repos, name: str, base: str, head: str, method: str, message: str, by: Person) -> str:
    """The commit that lands `head` on `base` by `method`; the caller moves the base branch."""
    m = mergeable(repos, name, base, head)
    if m.tree is None:
        raise GithubError(405, "Pull Request is not mergeable")
    if method == "rebase":
        return _rebase(repos, name, base, head, by)
    parents = [base, head] if method == "merge" else [base]
    return repos.commit_tree(name, m.tree, parents, message, by, by)
