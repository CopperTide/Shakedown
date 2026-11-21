"""Reading the store: issues by key or id, their labels, comments and links, projects, boards and
sprints; and an issue as the flat record JQL evaluates."""
from . import adf
from .errors import JiraError, no_issue, not_found


def issue(db, id_or_key: str):
    """The issue a path names (id or key, any case); 404 in Jira's words when there is none."""
    value = str(id_or_key)
    row = db.execute("SELECT * FROM issues WHERE id = ? OR key = upper(?)",
                     (int(value) if value.isdigit() else -1, value)).fetchone()
    if row is None:
        raise no_issue()
    return row


def issues(db, where: str = "1", args: tuple = ()) -> list:
    return db.execute(f"SELECT * FROM issues WHERE {where} ORDER BY id", args).fetchall()


def labels(db, issue_id: int) -> list[str]:
    return [r[0] for r in db.execute("SELECT label FROM labels WHERE issue_id = ? ORDER BY label", (issue_id,))]


def comments(db, issue_id: int) -> list:
    return db.execute("SELECT * FROM comments WHERE issue_id = ? ORDER BY id", (issue_id,)).fetchall()


def comment(db, issue_row, comment_id: str):
    row = db.execute("SELECT * FROM comments WHERE id = ? AND issue_id = ?",
                     (int(comment_id) if str(comment_id).isdigit() else -1, issue_row["id"])).fetchone()
    if row is None:
        raise not_found("Can not find a comment for the id: " + str(comment_id) + ".")
    return row


def links(db, issue_id: int) -> list:
    return db.execute("SELECT * FROM links WHERE inward_id = ? OR outward_id = ? ORDER BY id", (issue_id, issue_id)).fetchall()


def link(db, link_id: str):
    row = db.execute("SELECT * FROM links WHERE id = ?", (int(link_id) if str(link_id).isdigit() else -1,)).fetchone()
    if row is None:
        raise not_found(f"No issue link with id '{link_id}' exists.")
    return row


def projects(db) -> list:
    return db.execute("SELECT * FROM projects ORDER BY key").fetchall()


def project(db, id_or_key: str):
    value = str(id_or_key)
    return db.execute("SELECT * FROM projects WHERE id = ? OR key = upper(?)",
                      (int(value) if value.isdigit() else -1, value)).fetchone()


def project_or_404(db, id_or_key: str):
    row = project(db, id_or_key)
    if row is None:
        raise not_found(f"No project could be found with key '{id_or_key}'.")
    return row


def project_by_id(db, project_id: int):
    return db.execute("SELECT * FROM projects WHERE id = ?", (project_id,)).fetchone()


def board(db, board_id: str):
    row = db.execute("SELECT * FROM boards WHERE id = ?", (int(board_id) if str(board_id).isdigit() else -1,)).fetchone()
    if row is None:
        raise not_found("Board does not exist or you do not have permission to see it.")
    return row


def sprint(db, sprint_id) -> object:
    row = db.execute("SELECT * FROM sprints WHERE id = ?",
                     (int(sprint_id) if str(sprint_id).isdigit() else -1,)).fetchone()
    if row is None:
        raise JiraError(404, ["Sprint does not exist or you do not have permission to see it."])
    return row


def sprint_or_none(db, sprint_id):
    return None if sprint_id is None else db.execute("SELECT * FROM sprints WHERE id = ?", (sprint_id,)).fetchone()


def record(db, row, cache: dict) -> dict:
    """An issue as JQL sees it: every field it filters or sorts on, by name."""
    project_row = cache.setdefault(("p", row["project_id"]), project_by_id(db, row["project_id"]))
    text = [c["body"] for c in comments(db, row["id"])]
    return {"id": row["id"], "key": row["key"], "num": int(row["key"].rsplit("-", 1)[1]),
            "project": project_row["key"], "project_id": project_row["id"], "project_name": project_row["name"],
            "summary": row["summary"], "description": row["description"] or "", "comment": "\n".join(text),
            "status": row["status"], "type": row["type"], "priority": row["priority"], "assignee": row["assignee"],
            "reporter": row["reporter"], "creator": row["creator"], "resolution": row["resolution"],
            "labels": labels(db, row["id"]), "sprint": row["sprint_id"], "parent": row["parent_key"],
            "rank": row["rank"], "created": row["created_ms"], "updated": row["updated_ms"],
            "resolved": row["resolved_ms"], "duedate": row["duedate"]}


def plain(value) -> str:
    """Rich text as the store keeps it."""
    return adf.to_text(value) if isinstance(value, dict) else ("" if value is None else str(value))
