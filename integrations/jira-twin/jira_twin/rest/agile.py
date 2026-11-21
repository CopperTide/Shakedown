"""The agile API (`/rest/agile/1.0`): boards, their sprints, backlog and issues; sprints made,
started, closed and filled; the backlog; ranking."""
from .. import agile_shapes, finder, issue_shapes, records, sprint_rules
from .api import created, no_content, offset_page

STATE_ORDER = {"closed": 0, "active": 1, "future": 2}


def boards(call):
    rows = call.db.execute("SELECT b.* FROM boards b JOIN projects p ON p.id = b.project_id ORDER BY b.id").fetchall()
    want_type, name, project = call.arg("type"), call.arg("name"), call.arg("projectKeyOrId")
    if project:
        row = records.project(call.db, project)
        rows = [r for r in rows if row is not None and r["project_id"] == row["id"]]
    rows = [r for r in rows if (not want_type or r["type"] == want_type) and (not name or name.lower() in r["name"].lower())]
    return offset_page(call, [agile_shapes.board(call, r) for r in rows], "values", 50, 50)


def board_sprints(call):
    board = records.board(call.db, call.req.params["boardId"])
    states = [s.strip() for s in (call.arg("state") or "").split(",") if s.strip()]
    rows = call.db.execute("SELECT * FROM sprints WHERE board_id = ? ORDER BY id", (board["id"],)).fetchall()
    rows = sorted([r for r in rows if not states or r["state"] in states], key=lambda r: STATE_ORDER[r["state"]])
    return offset_page(call, [agile_shapes.sprint(call, r) for r in rows], "values", 50, 50)


def _issue_page(call, rows: list) -> dict:
    rows = finder.find(call, call.login, call.arg("jql") or "ORDER BY rank", rows)
    page = offset_page(call, rows, "issues", 50, 100)
    page["issues"] = [issue_shapes.issue(call, r, call.arg("fields"), call.arg("expand")) for r in page["issues"]]
    page.pop("isLast")
    return {"expand": "schema,names", **page}


def board_issues(call):
    board = records.board(call.db, call.req.params["boardId"])
    return _issue_page(call, records.issues(call.db, "project_id = ?", (board["project_id"],)))


def backlog(call):
    board = records.board(call.db, call.req.params["boardId"])
    rows = records.issues(call.db, "project_id = ? AND resolution IS NULL AND (sprint_id IS NULL OR sprint_id IN "
                                   "(SELECT id FROM sprints WHERE state = 'closed'))", (board["project_id"],))
    return _issue_page(call, rows)


def sprint_issues(call):
    sprint = records.sprint(call.db, call.req.params["sprintId"])
    return _issue_page(call, records.issues(call.db, "sprint_id = ?", (sprint["id"],)))


def update_sprint(call):
    sprint = records.sprint(call.db, call.req.params["sprintId"])
    return agile_shapes.sprint(call, sprint_rules.update(call, sprint, call.body()))


def add_to_sprint(call):
    sprint_rules.add_issues(call, records.sprint(call.db, call.req.params["sprintId"]), call.body().get("issues"))
    return no_content()


def to_backlog(call):
    sprint_rules.to_backlog(call, call.body().get("issues"))
    return no_content()


def rank(call):
    body = call.body()
    sprint_rules.rank(call, body.get("issues"), body.get("rankBeforeIssue"), body.get("rankAfterIssue"))
    return no_content()


def register(api) -> None:
    api.agile("GET", "/board")(boards)
    api.agile("GET", "/board/{boardId}")(lambda c: agile_shapes.board(c, records.board(c.db, c.req.params["boardId"])))
    api.agile("GET", "/board/{boardId}/sprint")(board_sprints)
    api.agile("GET", "/board/{boardId}/issue")(board_issues)
    api.agile("GET", "/board/{boardId}/backlog")(backlog)
    api.agile("POST", "/sprint")(lambda c: created(agile_shapes.sprint(c, sprint_rules.create(c, c.body()))))
    api.agile("GET", "/sprint/{sprintId}")(
        lambda c: agile_shapes.sprint(c, records.sprint(c.db, c.req.params["sprintId"])))
    api.agile("PUT", "/sprint/{sprintId}")(update_sprint)
    api.agile("POST", "/sprint/{sprintId}")(update_sprint)
    api.agile("GET", "/sprint/{sprintId}/issue")(sprint_issues)
    api.agile("POST", "/sprint/{sprintId}/issue")(add_to_sprint)
    api.agile("POST", "/backlog/issue")(to_backlog)
    api.agile("PUT", "/issue/rank")(rank)
