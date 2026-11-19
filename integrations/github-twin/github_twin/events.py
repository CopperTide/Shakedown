"""What follows refs moving, by a push or by the API: forced pushes counted, open pull requests on a
moved branch synchronised (head, `refs/pull/N/head`), and webhooks queued."""
from typing import Callable

from . import records, webhooks
from .rest import pull_shapes
from .repos import ZERO, Repos, full_name
from .view import View

HEADS = "refs/heads/"


def _forced(repos: Repos, repo, change) -> bool:
    if ZERO in (change.old, change.new):
        return False
    return not repos.is_ancestor(full_name(repo), change.old, change.new)


def _count_forced(db, repo, branch: str) -> None:
    db.execute("INSERT INTO branch_stats(repo_id, branch, forced_pushes) VALUES (?, ?, 1) "
               "ON CONFLICT(repo_id, branch) DO UPDATE SET forced_pushes = forced_pushes + 1", (repo["id"], branch))


def _synchronize(v: View, repo, branch: str, change, by: str) -> None:
    for pr in records.pulls(v.db, repo["id"], "i.state = 'open' AND p.head_ref = ?", (branch,)):
        records.set_pull(v.db, pr["id"], head_sha=change.new)
        records.touch(v.db, pr["id"], v.now())
        v.repos.set_pull_ref(full_name(repo), pr["number"], change.new)
        pr = records.pull(v.db, repo["id"], pr["number"])
        webhooks.fire(v, repo, "pull_request", by, "synchronize", number=pr["number"], before=change.old,
                      after=change.new, pull_request=pull_shapes.simple(v, repo, pr))


def _branch_moved(v: View, repo, change, by: str) -> None:
    branch, forced = change.ref[len(HEADS):], _forced(v.repos, repo, change)
    if forced:
        _count_forced(v.db, repo, branch)
    if change.new != ZERO:
        _synchronize(v, repo, branch, change, by)
    webhooks.push(v, repo, change, by, forced)


def after_push(ctx, repo, changes: list, by: str | None = None) -> None:
    if not changes:
        return
    v = View(ctx)
    for change in changes:
        if change.ref.startswith(HEADS):
            _branch_moved(v, repo, change, by or ctx.config.get("default_user", ""))
    ctx.store.conn.execute("UPDATE repos SET pushed_ms = ?, updated_ms = ? WHERE id = ?",
                           (ctx.now_ms(), ctx.now_ms(), repo["id"]))


def moved(ctx, repo, move: Callable[[], None], by: str | None = None) -> list:
    """Run `move` (git ref updates) and what follows the refs it moved; the changes."""
    repos, name = Repos(ctx.files), full_name(repo)
    before = repos.refs(name, HEADS)
    move()
    changes = repos.refs_changed(name, before, HEADS)
    after_push(ctx, repo, changes, by)
    return changes
