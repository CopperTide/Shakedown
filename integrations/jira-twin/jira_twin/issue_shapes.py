"""An issue as Jira answers it, with only the fields a request asks for (`fields=` names, `*all`,
`*navigable`, `-name` to leave one out)."""
from . import records, shapes, times
from .meta import RANK_FIELD, SPRINT_FIELD
from .sprint_rules import rank_text

EXPAND = "renderedFields,names,schema,operations,editmeta,changelog,versionedRepresentations"
ALL = ("*all", "*navigable")


def selection(given, default: str = "*all"):
    """The asked fields as `(names, excluded)`; `names` None means every field."""
    if given is None:
        given = default
    names = [n.strip() for n in (given if isinstance(given, list) else str(given).split(",")) if n.strip()]
    excluded = {n[1:] for n in names if n.startswith("-")}
    picked = [n for n in names if not n.startswith("-")]
    return (None if not picked or any(n in ALL for n in picked) else set(picked)), excluded


def link_entry(view, row, other_id: int, side: str) -> dict:
    other = records.issue(view.db, str(other_id))
    t = view.meta.link_type(row["type"])
    brief = {"summary": other["summary"], "status": shapes.status_named(view, other["status"]),
             "priority": shapes.priority(view, view.meta.priority(other["priority"])),
             "issuetype": shapes.issue_type(view, view.meta.type(other["type"]))}
    return {"id": str(row["id"]), "self": view.urls.api(f"issueLink/{row['id']}"), "type": shapes.link_type(view, t),
            side: {"id": str(other["id"]), "key": other["key"], "self": view.urls.api(f"issue/{other['id']}"),
                   "fields": brief}}


def links(view, issue_id: int) -> list:
    return [link_entry(view, r, r["inward_id"], "inwardIssue") if r["outward_id"] == issue_id
            else link_entry(view, r, r["outward_id"], "outwardIssue") for r in records.links(view.db, issue_id)]


def _sprint_field(view, sprint_id):
    s = records.sprint_or_none(view.db, sprint_id)
    if s is None:
        return None
    out = {"id": s["id"], "name": s["name"], "state": s["state"], "boardId": s["board_id"], "goal": s["goal"]}
    for key, col in (("startDate", "start_ms"), ("endDate", "end_ms"), ("completeDate", "complete_ms")):
        if s[col] is not None:
            out[key] = times.stamp(s[col])
    return [out]


def _comments(view, row) -> dict:
    items = [shapes.comment(view, row["id"], c) for c in records.comments(view.db, row["id"])]
    return {"comments": items, "self": view.urls.api(f"issue/{row['id']}/comment"), "maxResults": len(items),
            "total": len(items), "startAt": 0}


def _parent(view, key):
    if not key:
        return None
    p = records.issue(view.db, key)
    return {"id": str(p["id"]), "key": p["key"], "self": view.urls.api(f"issue/{p['id']}"),
            "fields": {"summary": p["summary"], "status": shapes.status_named(view, p["status"])}}


def _people_and_text(view, row) -> dict:
    return {"assignee": shapes.user_or_none(view, row["assignee"]), "reporter": shapes.user_or_none(view, row["reporter"]),
            "creator": shapes.user_or_none(view, row["creator"]), "summary": row["summary"],
            "description": shapes.rich_out(view, row["description"], shapes.display_names(view))}


def _every_field(view, row) -> dict:
    res = view.meta.resolution(row["resolution"]) if row["resolution"] else None
    out = {"issuetype": shapes.issue_type(view, view.meta.type(row["type"])),
           "project": shapes.project(view, records.project_by_id(view.db, row["project_id"])),
           "status": shapes.status_named(view, row["status"]),
           "priority": shapes.priority(view, view.meta.priority(row["priority"])),
           "resolution": shapes.resolution(view, res) if res else None,
           "resolutiondate": times.stamp(row["resolved_ms"]), "labels": records.labels(view.db, row["id"]),
           "created": times.stamp(row["created_ms"]), "updated": times.stamp(row["updated_ms"]),
           "duedate": row["duedate"], "issuelinks": links(view, row["id"]), "comment": _comments(view, row),
           "subtasks": [], SPRINT_FIELD: _sprint_field(view, row["sprint_id"]), RANK_FIELD: rank_text(row["rank"])}
    out.update(_people_and_text(view, row))
    parent = _parent(view, row["parent_key"])
    if parent:
        out["parent"] = parent
    return out


def issue(view, row, fields=None, expand: str | None = None, default: str = "*all") -> dict:
    names, excluded = selection(fields, default)
    if names == {"id"}:
        return {"id": str(row["id"])}
    every = _every_field(view, row)
    picked = {k: v for k, v in every.items() if (names is None or k in names) and k not in excluded}
    out = {"expand": EXPAND, "id": str(row["id"]), "self": view.urls.api(f"issue/{row['id']}"), "key": row["key"],
           "fields": picked}
    if expand and "transitions" in expand:
        out["transitions"] = transitions(view, row)
    return out


def transitions(view, row) -> list:
    return [{"id": t.id, "name": t.name, "to": shapes.status_named(view, t.to), "hasScreen": False,
             "isGlobal": t.sources is None, "isInitial": False, "isAvailable": True, "isConditional": False,
             "looped": False} for t in view.workflow.available(row["status"])]
