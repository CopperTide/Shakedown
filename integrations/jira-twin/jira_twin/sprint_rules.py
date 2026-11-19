"""Sprints and ranks: sprints made, started (with dates) and closed (open issues go back to the
backlog), issues moved into a sprint or the backlog, and issues ranked before or after another."""
from . import records, times
from .errors import JiraError, bad

STATES = ("future", "active", "closed")


def create(view, body: dict):
    board = records.board(view.db, body.get("originBoardId"))
    if not body.get("name"):
        raise JiraError(400, errors={"name": "Sprint name is required."})
    cur = view.db.execute("INSERT INTO sprints(id, board_id, name, state, start_ms, end_ms, created_ms, goal) "
                          "VALUES ((SELECT COALESCE(MAX(id), 0) + 1 FROM sprints), ?, ?, 'future', ?, ?, ?, ?)",
                          (board["id"], body["name"], times.at(body.get("startDate"), view.now()),
                           times.at(body.get("endDate"), view.now()), view.now(), body.get("goal") or ""))
    return view.db.execute("SELECT * FROM sprints WHERE rowid = ?", (cur.lastrowid,)).fetchone()


def _dates(view, sprint, body: dict) -> tuple:
    start = times.at(body["startDate"], view.now()) if body.get("startDate") else sprint["start_ms"]
    end = times.at(body["endDate"], view.now()) if body.get("endDate") else sprint["end_ms"]
    return start, end


def update(view, sprint, body: dict):
    """A partial update (the client sends only what changes); `state` starts or closes the sprint."""
    state = body.get("state", sprint["state"])
    if state not in STATES:
        raise bad(f"Unknown sprint state '{state}'.")
    start, end = _dates(view, sprint, body)
    if state == "active" and sprint["state"] != "active" and (start is None or end is None):
        raise JiraError(400, errors={"startDate": "A start and an end date are needed to start a sprint."})
    complete = sprint["complete_ms"]
    if state == "closed" and sprint["state"] != "closed":
        complete = view.now()
        _to_backlog_unresolved(view, sprint["id"])
    view.db.execute("UPDATE sprints SET name = ?, state = ?, start_ms = ?, end_ms = ?, complete_ms = ?, goal = ? "
                    "WHERE id = ?", (body.get("name") or sprint["name"], state, start, end, complete,
                                     body.get("goal", sprint["goal"]) or "", sprint["id"]))
    return records.sprint(view.db, sprint["id"])


def _to_backlog_unresolved(view, sprint_id: int) -> None:
    view.db.execute("UPDATE issues SET sprint_id = NULL, updated_ms = ? WHERE sprint_id = ? AND resolution IS NULL",
                    (view.now(), sprint_id))


def _issues(view, keys) -> list:
    if not isinstance(keys, list) or len(keys) > 50:
        raise bad("Between 1 and 50 issues may be moved at once.")
    return [records.issue(view.db, k) for k in keys]


def add_issues(view, sprint, keys) -> None:
    if sprint["state"] == "closed":
        raise bad("Cannot move issues to a closed sprint.")
    for row in _issues(view, keys):
        view.db.execute("UPDATE issues SET sprint_id = ?, updated_ms = ? WHERE id = ?", (sprint["id"], view.now(), row["id"]))


def to_backlog(view, keys) -> None:
    for row in _issues(view, keys):
        view.db.execute("UPDATE issues SET sprint_id = NULL, updated_ms = ? WHERE id = ?", (view.now(), row["id"]))


def rank(view, keys, before: str | None, after: str | None) -> None:
    """Moves `keys` (in order) just before `before` or just after `after`; ranks are renumbered 1..n."""
    moving = [r["id"] for r in _issues(view, keys)]
    target = records.issue(view.db, before or after or "")["id"]
    if target in moving:
        raise bad("An issue cannot be ranked against itself.")
    order = [r[0] for r in view.db.execute("SELECT id FROM issues ORDER BY rank, id") if r[0] not in moving]
    at = order.index(target) + (0 if before else 1)
    order[at:at] = moving
    view.db.executemany("UPDATE issues SET rank = ? WHERE id = ?", [(i + 1, x) for i, x in enumerate(order)])


def rank_text(rank: int) -> str:
    """The Rank field's LexoRank-like text (sorts as the rank does)."""
    return f"0|i{rank:05d}:"
