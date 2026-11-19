"""The docs `inspect` reads (and checks score): repositories and branches."""
from .repos import Repos, full_name


def _branches(repos: Repos, db, repo) -> dict:
    fn = full_name(repo)
    forced = {r["branch"]: r["forced_pushes"] for r in
              db.execute("SELECT branch, forced_pushes FROM branch_stats WHERE repo_id = ?", (repo["id"],))}
    return {f"branch/{fn}/{b}": {"sha": sha, "forced_pushes": forced.get(b, 0)}
            for b, sha in repos.branches(fn).items()}


def build(ctx) -> dict:
    db, repos, out = ctx.store.conn, Repos(ctx.files), {}
    for repo in db.execute("SELECT * FROM repos ORDER BY id"):
        out[f"repo/{full_name(repo)}"] = {"default_branch": repo["default_branch"], "private": bool(repo["private"]),
                                          "description": repo["description"]}
        out.update(_branches(repos, db, repo))
    return out
