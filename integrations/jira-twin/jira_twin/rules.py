"""The writes, shared by REST and `jira_act`: issues made, edited, moved through the workflow,
commented, assigned, linked, put in sprints and ranked; each one stamped with world time, and the
notices the world's people react to."""
from . import adf, fields_in, records, users
from .errors import JiraError, invalid
from .meta import DEFAULT_PRIORITY, SPRINT_FIELD
from .workflow import Workflow

COLUMNS = {"summary": "summary", "description": "description", "issuetype": "type", "priority": "priority",
           "assignee": "assignee", "reporter": "reporter", "duedate": "duedate", "parent": "parent_key",
           SPRINT_FIELD: "sprint_id"}


def _touch(view, issue_id: int) -> None:
    view.db.execute("UPDATE issues SET updated_ms = ? WHERE id = ?", (view.now(), issue_id))


def _next_rank(db) -> int:
    return db.execute("SELECT COALESCE(MAX(rank), 0) + 1 FROM issues").fetchone()[0]


def _next_key(db, project) -> str:
    db.execute("UPDATE projects SET next_num = next_num + 1 WHERE id = ?", (project["id"],))
    return f"{project['key']}-{project['next_num']}"


def _resolution_for(workflow: Workflow, status: str, current):
    done = workflow.status(status).category == "done"
    return (current or "Done") if done else None


def insert_issue(view, actor: str, project, values: dict, at: dict | None = None):
    """A new issue in `project`; `at` may give seed times (`created_ms`, `updated_ms`), `status`, `rank`."""
    at, db, now = at or {}, view.db, view.now()
    status = at.get("status") or view.workflow.initial().name
    resolution = _resolution_for(view.workflow, status, None)
    issue_id = db.execute("SELECT COALESCE(MAX(id), 10000) + 1 FROM issues").fetchone()[0]
    db.execute("INSERT INTO issues(id, key, project_id, summary, description, type, status, priority, assignee, "
               "reporter, creator, resolution, sprint_id, parent_key, duedate, rank, created_ms, updated_ms, "
               "resolved_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
               (issue_id, _next_key(db, project), project["id"], values["summary"], values.get("description") or None,
                values["issuetype"], status, values.get("priority") or DEFAULT_PRIORITY, values.get("assignee"),
                values.get("reporter") or actor, actor, resolution, values.get(SPRINT_FIELD), values.get("parent"),
                values.get("duedate"), at.get("rank") or _next_rank(db), at.get("created_ms", now),
                at.get("updated_ms", now), at.get("updated_ms", now) if resolution else None))
    _set_labels(db, issue_id, values.get("labels") or [])
    row = records.issue(db, str(issue_id))
    if row["assignee"]:
        view.notify("issue_assigned", {"key": row["key"], "assignee": row["assignee"]})
    return row


def create_issue(view, actor: str, fields: dict):
    project = fields_in.project_of(view, fields.get("project"))
    values = fields_in.read(view, fields, allow_project=True)
    if not values.get("summary"):
        raise invalid("summary", "You must specify a summary of the issue.")
    if "issuetype" not in values:
        raise invalid("issuetype", "Specify an issue type")
    return insert_issue(view, actor, project, values)


def _set_labels(db, issue_id: int, labels: list) -> None:
    db.execute("DELETE FROM labels WHERE issue_id = ?", (issue_id,))
    db.executemany("INSERT OR IGNORE INTO labels(issue_id, label) VALUES (?, ?)", [(issue_id, x) for x in labels])


def _label_ops(db, row, ops: list) -> None:
    current = records.labels(db, row["id"])
    for op in ops:
        if "set" in op:
            current = [str(x) for x in op["set"] or []]
        elif "add" in op and op["add"] not in current:
            current.append(str(op["add"]))
        elif "remove" in op:
            current = [x for x in current if x != op["remove"]]
    _set_labels(db, row["id"], current)


def _apply(view, row, values: dict) -> None:
    for name, value in values.items():
        if name == "labels":
            _set_labels(view.db, row["id"], value)
        else:
            view.db.execute(f"UPDATE issues SET {COLUMNS[name]} = ? WHERE id = ?", (value, row["id"]))
    if values.get("assignee") and values["assignee"] != row["assignee"]:
        view.notify("issue_assigned", {"key": row["key"], "assignee": values["assignee"]})


def _update_ops(view, actor: str, row, update: dict) -> dict:
    """The `update` verbs: labels add/remove/set, comment add, and `set` on any other field."""
    values = {}
    for name, ops in (update or {}).items():
        if name == "labels":
            _label_ops(view.db, row, ops)
        elif name == "comment":
            for op in ops:
                add_comment(view, actor, row, fields_in.rich(view, op.get("add", {}).get("body"), "comment",
                                                             "Comment body is not valid!"))
        else:
            values.update(fields_in.read(view, {name: op["set"] for op in ops if "set" in op}, allow_project=False))
    return values


def edit_issue(view, actor: str, row, fields: dict, update: dict) -> None:
    values = fields_in.read(view, fields or {}, allow_project=False)
    values.update(_update_ops(view, actor, row, update))
    _apply(view, row, values)
    _touch(view, row["id"])


def move(view, actor: str, row, to_status: str, resolution: str | None = None) -> None:
    """Into status `to_status`: a done status sets a resolution, any other clears it."""
    res = _resolution_for(view.workflow, to_status, resolution)
    resolved = view.now() if res else None
    view.db.execute("UPDATE issues SET status = ?, resolution = ?, resolved_ms = ?, updated_ms = ? WHERE id = ?",
                    (to_status, res, resolved, view.now(), row["id"]))
    view.notify("issue_transitioned", {"key": row["key"], "from": row["status"], "to": to_status, "by": actor})


def transition(view, actor: str, row, transition_id: str, fields: dict, update: dict) -> None:
    t = view.workflow.transition(row["status"], transition_id)
    resolution = (fields or {}).pop("resolution", None)
    edit_issue(view, actor, row, fields or {}, update or {})
    name = resolution.get("name") or resolution.get("id") if isinstance(resolution, dict) else resolution
    found = view.meta.resolution(name) if name else None
    move(view, actor, records.issue(view.db, str(row["id"])), t.to, found.name if found else None)


def _mentioned(view, text: str, author: str) -> list[str]:
    out = []
    for ref in adf.MENTION.findall(text or ""):
        row = users.find(view.db, ref)
        name = row["name"] if row else (None if users.ACCOUNT_ID.fullmatch(ref) else ref)
        if name and name != author and name not in out:
            out.append(name)
    return out


def add_comment(view, actor: str, row, text: str):
    now = view.now()
    cur = view.db.execute("INSERT INTO comments(id, issue_id, author, update_author, body, created_ms, updated_ms) "
                          "VALUES ((SELECT COALESCE(MAX(id), 10000) + 1 FROM comments), ?, ?, ?, ?, ?, ?)",
                          (row["id"], actor, actor, text, now, now))
    _touch(view, row["id"])
    for name in _mentioned(view, text, actor):
        view.notify("mentioned", {"key": row["key"], "user": name, "body": text})
    return view.db.execute("SELECT * FROM comments WHERE rowid = ?", (cur.lastrowid,)).fetchone()


def edit_comment(view, actor: str, issue_row, comment_row, text: str):
    view.db.execute("UPDATE comments SET body = ?, update_author = ?, updated_ms = ? WHERE id = ?",
                    (text, actor, view.now(), comment_row["id"]))
    _touch(view, issue_row["id"])
    return records.comment(view.db, issue_row, comment_row["id"])


def assign(view, row, user_ref) -> None:
    given, user = users.find_in(view.db, user_ref)
    if given and user is None:
        raise invalid("assignee", f"User '{user_ref}' does not exist.")
    _apply(view, row, {"assignee": user["name"] if user else None})
    _touch(view, row["id"])


def delete_issue(view, row) -> None:
    view.db.execute("DELETE FROM issues WHERE id = ?", (row["id"],))


def link(view, type_ref, inward, outward) -> None:
    t = view.meta.link_type((type_ref or {}).get("id") or (type_ref or {}).get("name") or "")
    if t is None:
        raise JiraError(404, ["No issue link type with name '%s' found." % (type_ref or {}).get("name")])
    view.db.execute("INSERT INTO links(type, inward_id, outward_id) VALUES (?, ?, ?)", (t[1], inward["id"], outward["id"]))
