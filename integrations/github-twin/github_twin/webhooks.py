"""Webhook payloads: every one has the repository and the sender; queued for the repo's hooks."""
from . import hooks_delivery, users
from .repos import ZERO, full_name
from .rest import shapes


def _sender(v, login: str) -> dict:
    return users.simple_user(v.urls, users.ensure(v.db, login, v.now()))


def fire(v, repo, event: str, sender: str, action: str | None = None, **fields) -> None:
    """Queue `event` for the repo's hooks that want it."""
    if not hooks_delivery.hooks_for(v.db, repo["id"], event):
        return
    payload = {**({"action": action} if action else {}), **fields,
               "repository": shapes.repository(v, repo), "sender": _sender(v, sender)}
    hooks_delivery.queue(v.ctx, repo["id"], event, payload)


def _push_commit(v, repo, sha: str) -> dict:
    info = v.repos.commit_info(full_name(repo), sha)
    return {"id": sha, "tree_id": info.tree, "message": info.message, "distinct": True,
            "timestamp": shapes.person(info.author)["date"], "url": v.urls.repo_html(full_name(repo), f"/commit/{sha}"),
            "author": {"name": info.author.name, "email": info.author.email},
            "committer": {"name": info.committer.name, "email": info.committer.email},
            "added": [], "removed": [], "modified": []}


def push(v, repo, change, pusher: str, forced: bool) -> None:
    fn = full_name(repo)
    news = [] if change.new == ZERO else v.repos.rev_list(fn, change.new if change.old == ZERO else f"{change.old}..{change.new}")
    commits = [_push_commit(v, repo, s) for s in reversed(news[:20])]
    fire(v, repo, "push", pusher, ref=change.ref, before=change.old, after=change.new,
         created=change.old == ZERO, deleted=change.new == ZERO, forced=forced, base_ref=None,
         compare=v.urls.repo_html(fn, f"/compare/{change.old[:12]}...{change.new[:12]}"), commits=commits,
         head_commit=commits[-1] if commits else None,
         pusher={"name": pusher, "email": users.email(v.config, pusher)})
