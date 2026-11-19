"""What a commit's checks add up to: check runs and commit statuses, as GitHub combines them."""
FAILING = {"failure", "timed_out", "cancelled", "action_required", "startup_failure", "stale", "error"}


def runs(db, repo_id: int, sha: str) -> list:
    return db.execute("SELECT * FROM check_runs WHERE repo_id = ? AND head_sha = ? ORDER BY id", (repo_id, sha)).fetchall()


def runs_conclusion(rows: list) -> str | None:
    """`failure` when one completed run failed, `success` when all completed and none did; else None."""
    if not rows or any(r["status"] != "completed" for r in rows):
        return None
    return "failure" if any(r["conclusion"] in FAILING for r in rows) else "success"


def latest_statuses(db, repo_id: int, sha: str) -> list:
    """The latest status of each context, newest first."""
    rows = db.execute("SELECT * FROM statuses WHERE repo_id = ? AND sha = ? ORDER BY id DESC", (repo_id, sha)).fetchall()
    seen, out = set(), []
    for r in rows:
        if r["context"] not in seen:
            seen.add(r["context"])
            out.append(r)
    return out


def combined_state(latest: list) -> str:
    states = {r["state"] for r in latest}
    if states & {"failure", "error"}:
        return "failure"
    return "success" if latest and states == {"success"} else "pending"


def head_checks(db, repo_id: int, sha: str) -> str:
    """`none` (nothing reported), `pending`, `success` or `failure` over runs and statuses."""
    rs, ss = runs(db, repo_id, sha), latest_statuses(db, repo_id, sha)
    if not rs and not ss:
        return "none"
    parts = ([runs_conclusion(rs) or "pending"] if rs else []) + ([combined_state(ss)] if ss else [])
    return "failure" if "failure" in parts else "pending" if "pending" in parts else "success"
