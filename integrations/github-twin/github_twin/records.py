"""Reading and writing issues, pull requests, labels and reviews in the store."""
import json

from .errors import not_found

PULL = "SELECT i.*, p.* FROM issues i JOIN pulls p ON p.issue_id = i.id"


def repo_named(db, full: str):
    owner, _, name = full.partition("/")
    return db.execute("SELECT * FROM repos WHERE owner = ? AND name = ?", (owner, name)).fetchone()


def issue(db, repo_id: int, number: int):
    row = db.execute("SELECT * FROM issues WHERE repo_id = ? AND number = ?", (repo_id, number)).fetchone()
    if row is None:
        raise not_found()
    return row


def pull(db, repo_id: int, number: int):
    row = db.execute(f"{PULL} WHERE i.repo_id = ? AND i.number = ?", (repo_id, number)).fetchone()
    if row is None:
        raise not_found()
    return row


def pulls(db, repo_id: int, where: str = "1", args: tuple = ()) -> list:
    return db.execute(f"{PULL} WHERE i.repo_id = ? AND ({where}) ORDER BY i.number DESC", (repo_id, *args)).fetchall()


def insert_issue(db, repo_id: int, title: str, body: str | None, user: str, now: int, is_pull: bool = False):
    n = db.execute("SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repo_id = ?", (repo_id,)).fetchone()[0]
    db.execute("INSERT INTO issues(repo_id, number, title, body, user, is_pull, created_ms, updated_ms) "
               "VALUES (?, ?, ?, ?, ?, ?, ?, ?)", (repo_id, n, title, body, user, int(is_pull), now, now))
    return issue(db, repo_id, n)


def touch(db, issue_id: int, now: int, **fields) -> None:
    sets = "".join(f", {k} = ?" for k in fields)
    db.execute(f"UPDATE issues SET updated_ms = ?{sets} WHERE id = ?", (now, *fields.values(), issue_id))


def set_pull(db, issue_id: int, **fields) -> None:
    sets = ", ".join(f"{k} = ?" for k in fields)
    db.execute(f"UPDATE pulls SET {sets} WHERE issue_id = ?", (*fields.values(), issue_id))


def close(db, issue_id: int, by: str, now: int) -> None:
    touch(db, issue_id, now, state="closed", closed_ms=now, closed_by=by)


def reopen(db, issue_id: int, now: int) -> None:
    touch(db, issue_id, now, state="open", closed_ms=None, closed_by=None)


# --- labels and assignees ---------------------------------------------------------------
def label(db, repo_id: int, name: str):
    return db.execute("SELECT * FROM labels WHERE repo_id = ? AND name = ?", (repo_id, name)).fetchone()


def ensure_label(db, repo_id: int, name: str):
    if label(db, repo_id, name) is None:
        db.execute("INSERT INTO labels(repo_id, name) VALUES (?, ?)", (repo_id, name))
    return label(db, repo_id, name)


def labels_of(db, issue_id: int) -> list:
    return db.execute("SELECT l.* FROM labels l JOIN issue_labels il ON il.label_id = l.id "
                      "WHERE il.issue_id = ? ORDER BY l.name", (issue_id,)).fetchall()


def add_labels(db, repo_id: int, issue_id: int, names: list) -> None:
    for name in names:
        db.execute("INSERT OR IGNORE INTO issue_labels VALUES (?, ?)", (issue_id, ensure_label(db, repo_id, name)["id"]))


def set_labels(db, repo_id: int, issue_id: int, names: list) -> None:
    db.execute("DELETE FROM issue_labels WHERE issue_id = ?", (issue_id,))
    add_labels(db, repo_id, issue_id, names)


def assignees_of(db, issue_id: int) -> list:
    return [r[0] for r in db.execute("SELECT login FROM assignees WHERE issue_id = ? ORDER BY login", (issue_id,))]


def set_assignees(db, issue_id: int, logins: list) -> None:
    db.execute("DELETE FROM assignees WHERE issue_id = ?", (issue_id,))
    db.executemany("INSERT OR IGNORE INTO assignees VALUES (?, ?)", [(issue_id, x) for x in logins])


# --- comments, reviews, requests --------------------------------------------------------
def comment_count(db, issue_id: int) -> int:
    return db.execute("SELECT COUNT(*) FROM comments WHERE issue_id = ?", (issue_id,)).fetchone()[0]


def review_comment_count(db, issue_id: int) -> int:
    return db.execute("SELECT COUNT(*) FROM review_comments WHERE issue_id = ?", (issue_id,)).fetchone()[0]


def reviews_of(db, issue_id: int) -> list:
    return db.execute("SELECT * FROM reviews WHERE issue_id = ? ORDER BY id", (issue_id,)).fetchall()


def requested(db, issue_id: int) -> list:
    return [r[0] for r in db.execute("SELECT login FROM review_requests WHERE issue_id = ? ORDER BY login", (issue_id,))]


def approvals_on(db, issue_id: int, head_sha: str) -> list:
    """Users whose latest deciding review (approve or request changes) approves `head_sha`."""
    latest = {}
    for r in reviews_of(db, issue_id):
        if r["state"] in ("APPROVED", "CHANGES_REQUESTED", "DISMISSED"):
            latest[r["user"]] = r
    return sorted(u for u, r in latest.items() if r["state"] == "APPROVED" and r["commit_id"] == head_sha)


def approvals_at_merge(row) -> list:
    return json.loads(row["approvals_at_merge"])
