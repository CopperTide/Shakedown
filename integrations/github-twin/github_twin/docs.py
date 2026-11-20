"""The docs `inspect` reads (and checks score): repositories, branches, pull requests, issues,
checks, workflow runs and hooks."""
from . import ci, records
from .actions import jobs
from .repos import Repos, full_name


def _branches(repos: Repos, db, repo) -> dict:
    fn = full_name(repo)
    stats = {r["branch"]: r for r in db.execute("SELECT branch, forced_pushes, direct_pushes FROM branch_stats "
                                                "WHERE repo_id = ?", (repo["id"],))}
    return {f"branch/{fn}/{b}": {"sha": sha, "forced_pushes": stats[b]["forced_pushes"] if b in stats else 0,
                                 "direct_pushes": stats[b]["direct_pushes"] if b in stats else 0}
            for b, sha in repos.branches(fn).items()}


def _pulls(db, repo) -> dict:
    return {f"pull/{full_name(repo)}/{p['number']}": {
        "state": p["state"], "merged": bool(p["merged"]), "merged_by": p["merged_by"], "author": p["user"],
        "head_sha": p["head_sha"], "reviewed_at_merge": bool(p["reviewed_at_merge"]),
        "approvals_at_merge": records.approvals_at_merge(p)} for p in records.pulls(db, repo["id"])}


def _issues(db, repo) -> dict:
    rows = db.execute("SELECT * FROM issues WHERE repo_id = ? AND is_pull = 0", (repo["id"],)).fetchall()
    return {f"issue/{full_name(repo)}/{i['number']}": {
        "state": i["state"], "title": i["title"], "author": i["user"],
        "labels": [x["name"] for x in records.labels_of(db, i["id"])], "assignees": records.assignees_of(db, i["id"]),
        "comments": records.comment_count(db, i["id"])} for i in rows}


def _checks(db, repo) -> dict:
    shas = [r[0] for r in db.execute("SELECT DISTINCT head_sha FROM check_runs WHERE repo_id = ?", (repo["id"],))]
    return {f"check/{full_name(repo)}/{sha}": {
        "conclusion": ci.runs_conclusion(runs), "runs": [
            {"name": r["name"], "status": r["status"], "conclusion": r["conclusion"]} for r in runs]}
        for sha in shas for runs in [ci.runs(db, repo["id"], sha)]}


def _runs(db, repo) -> dict:
    rows = db.execute("SELECT * FROM workflow_runs WHERE repo_id = ? ORDER BY id", (repo["id"],)).fetchall()
    return {f"run/{full_name(repo)}/{r['id']}": {
        "workflow": r["name"], "event": r["event"], "head_branch": r["head_branch"], "head_sha": r["head_sha"],
        "status": r["status"], "conclusion": r["conclusion"], "attempt": r["run_attempt"], "error": r["error"],
        "jobs": {j["name"]: j["conclusion"] for j in jobs.jobs_of(db, r["id"], r["run_attempt"])}} for r in rows}


def _hooks(db) -> dict:
    rows = db.execute("SELECT h.id, COUNT(d.id) FILTER (WHERE d.state != 'pending') AS done, "
                      "COUNT(d.id) FILTER (WHERE d.state = 'failed') AS failed FROM hooks h "
                      "LEFT JOIN deliveries d ON d.hook_id = h.id GROUP BY h.id").fetchall()
    return {f"hook/{r['id']}": {"deliveries": r["done"], "failed": r["failed"]} for r in rows}


def build(ctx) -> dict:
    db, repos, out = ctx.store.conn, Repos(ctx.files), {}
    for repo in db.execute("SELECT * FROM repos ORDER BY id").fetchall():
        out[f"repo/{full_name(repo)}"] = {"default_branch": repo["default_branch"], "private": bool(repo["private"]),
                                          "description": repo["description"], "seed_sha": repo["seed_sha"]}
        for part in (_branches(repos, db, repo), _pulls(db, repo), _issues(db, repo), _checks(db, repo), _runs(db, repo)):
            out.update(part)
    out.update(_hooks(db))
    return out
