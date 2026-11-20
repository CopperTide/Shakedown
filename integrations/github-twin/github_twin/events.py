"""What follows refs moving, by a push or by the API: forced pushes counted, moves no pull request's merge
made counted (direct pushes), open pull requests on a moved branch synchronised (head, `refs/pull/N/head`), and
webhooks queued."""
from typing import Callable

from . import records, webhooks
from .actions import runs
from .rest import pull_shapes
from .repos import ZERO, Repos, full_name
from .view import View

HEADS = "refs/heads/"


def _forced(repos: Repos, repo, change) -> bool:
    if ZERO in (change.old, change.new):
        return False
    return not repos.is_ancestor(full_name(repo), change.old, change.new)


def _count(db, repo, branch: str, stat: str) -> None:
    """One more `stat` (forced_pushes, direct_pushes) on `branch`."""
    db.execute(f"INSERT INTO branch_stats(repo_id, branch, {stat}) VALUES (?, ?, 1) "
               f"ON CONFLICT(repo_id, branch) DO UPDATE SET {stat} = {stat} + 1", (repo["id"], branch))


def _synchronize(v: View, repo, branch: str, change, by: str) -> None:
    for pr in records.pulls(v.db, repo["id"], "i.state = 'open' AND p.head_ref = ?", (branch,)):
        records.set_pull(v.db, pr["id"], head_sha=change.new)
        records.touch(v.db, pr["id"], v.now())
        v.repos.set_pull_ref(full_name(repo), pr["number"], change.new)
        pr = records.pull(v.db, repo["id"], pr["number"])
        webhooks.fire(v, repo, "pull_request", by, "synchronize", number=pr["number"], before=change.old,
                      after=change.new, pull_request=pull_shapes.simple(v, repo, pr))
        runs.on_pull_request(v, repo, pr, by)


def _branch_moved(v: View, repo, change, by: str, merge: bool) -> None:
    branch, forced = change.ref[len(HEADS):], _forced(v.repos, repo, change)
    if forced:
        _count(v.db, repo, branch, "forced_pushes")
    if change.new != ZERO and not merge:
        _count(v.db, repo, branch, "direct_pushes")
    if change.new != ZERO:
        _synchronize(v, repo, branch, change, by)
    webhooks.push(v, repo, change, by, forced)
    if change.new != ZERO:
        runs.on_push(v, repo, branch, change.new, by)


def after_push(ctx, repo, changes: list, by: str | None = None, merge: bool = False) -> None:
    """What follows `changes`; `merge`: they are a pull request's merge, not a push."""
    if not changes:
        return
    v = View(ctx)
    for change in changes:
        if change.ref.startswith(HEADS):
            _branch_moved(v, repo, change, by or ctx.config.get("default_user", ""), merge)
    ctx.store.conn.execute("UPDATE repos SET pushed_ms = ?, updated_ms = ? WHERE id = ?",
                           (ctx.now_ms(), ctx.now_ms(), repo["id"]))


def moved(ctx, repo, move: Callable[[], None], by: str | None = None, merge: bool = False) -> list:
    """Run `move` (git ref updates) and what follows the refs it moved; the changes."""
    repos, name = Repos(ctx.files), full_name(repo)
    before = repos.refs(name, HEADS)
    move()
    changes = repos.refs_changed(name, before, HEADS)
    after_push(ctx, repo, changes, by, merge)
    return changes
