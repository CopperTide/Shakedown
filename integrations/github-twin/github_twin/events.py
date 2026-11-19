"""What follows a ref moving, by a push or by the API: forced pushes counted, the push time kept."""
from typing import Callable

from .repos import ZERO, Repos, full_name

HEADS = "refs/heads/"


def _count_forced(ctx, repos: Repos, repo, change) -> None:
    if not change.ref.startswith(HEADS) or ZERO in (change.old, change.new):
        return
    if repos.is_ancestor(full_name(repo), change.old, change.new):
        return
    ctx.store.conn.execute(
        "INSERT INTO branch_stats(repo_id, branch, forced_pushes) VALUES (?, ?, 1) "
        "ON CONFLICT(repo_id, branch) DO UPDATE SET forced_pushes = forced_pushes + 1",
        (repo["id"], change.ref[len(HEADS):]))


def after_push(ctx, repo, changes: list) -> None:
    if not changes:
        return
    repos = Repos(ctx.files)
    for change in changes:
        _count_forced(ctx, repos, repo, change)
    ctx.store.conn.execute("UPDATE repos SET pushed_ms = ?, updated_ms = ? WHERE id = ?",
                           (ctx.now_ms(), ctx.now_ms(), repo["id"]))


def moved(ctx, repo, move: Callable[[], None]) -> list:
    """Run `move` (git ref updates) and what follows the refs it moved; the changes."""
    repos, name = Repos(ctx.files), full_name(repo)
    before = repos.refs(name)
    move()
    changes = repos.refs_changed(name, before)
    after_push(ctx, repo, changes)
    return changes
