"""The fields JQL knows: how each reads an issue record and what kind of values it takes, and the
fields `ORDER BY` sorts on."""
from dataclasses import dataclass
from typing import Callable

from .. import times


@dataclass(frozen=True)
class Field:
    name: str
    kind: str      # project key status category type user priority labels resolution sprint parent date text rank none
    get: Callable


def _rec(name: str) -> Callable:
    return lambda r, ctx: r[name]


def _due(r, ctx):
    return times.parse_iso(r["duedate"]) if r["duedate"] else None


def _text(r, ctx):
    return "\n".join([r["summary"], r["description"], r["comment"]])


def _none(r, ctx):
    return None


_FIELDS = [
    (("project",), "project", _rec("project")),
    (("key", "issuekey", "id"), "key", _rec("key")),
    (("status",), "status", _rec("status")),
    (("statuscategory",), "category", lambda r, ctx: ctx.category_of(r["status"])),
    (("issuetype", "type"), "type", _rec("type")),
    (("assignee",), "user", _rec("assignee")),
    (("reporter",), "user", _rec("reporter")),
    (("creator",), "user", _rec("creator")),
    (("priority",), "priority", _rec("priority")),
    (("labels",), "labels", _rec("labels")),
    (("resolution",), "resolution", _rec("resolution")),
    (("sprint", "cf[10020]"), "sprint", _rec("sprint")),
    (("rank", "cf[10019]"), "rank", _rec("rank")),
    (("parent",), "parent", _rec("parent")),
    (("created", "createddate"), "date", _rec("created")),
    (("updated", "updateddate"), "date", _rec("updated")),
    (("resolved", "resolutiondate"), "date", _rec("resolved")),
    (("duedate", "due"), "date", _due),
    (("summary",), "text", _rec("summary")),
    (("description",), "text", _rec("description")),
    (("comment",), "text", _rec("comment")),
    (("text",), "text", _text),
    (("fixversion",), "none", _none),
    (("component",), "none", _none),
]

NAMES = {"statuscategory": "statusCategory", "issuetype": "issuetype", "fixversion": "fixVersion"}

#: Every JQL name (lower case) to its field; errors name a field as Jira spells it.
FIELDS = {alias: Field(NAMES.get(aliases[0], aliases[0]), kind, get) for aliases, kind, get in _FIELDS
          for alias in aliases}

SORTS = {
    "created": _rec("created"), "updated": _rec("updated"), "resolved": _rec("resolved"), "duedate": _due,
    "rank": _rec("rank"), "cf[10019]": _rec("rank"), "key": lambda r, ctx: (r["project"], r["num"]),
    "issuekey": lambda r, ctx: (r["project"], r["num"]), "id": _rec("id"),
    "priority": lambda r, ctx: -ctx.priority_rank(r["priority"]),
    "status": lambda r, ctx: ctx.status_order.index(r["status"]) if r["status"] in ctx.status_order else None,
    "summary": lambda r, ctx: r["summary"].lower(), "assignee": _rec("assignee"), "reporter": _rec("reporter"),
    "project": _rec("project"), "issuetype": _rec("type"), "type": _rec("type"),
}
