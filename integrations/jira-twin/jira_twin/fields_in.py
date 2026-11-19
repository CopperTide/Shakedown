"""Reading the fields a create or an edit sends: lenient on what clients send (ids or names, nulls,
v2 text or v3 ADF) and exact in Jira's words when a value is wrong."""
from . import adf, records, users
from .errors import invalid
from .meta import SPRINT_FIELD, settable

ADF_NEEDED = "Operation value must be an Atlassian Document (see the Atlassian Document Format)"


def rich(view, value, field: str, message: str = ADF_NEEDED) -> str:
    """Rich text from a request: v3 needs an ADF document, v2 takes text."""
    if value is None:
        return ""
    if view.adf:
        if not adf.is_doc(value):
            raise invalid(field, message)
        return adf.to_text(value)
    return records.plain(value)


def _named(value):
    return value.get("id") or value.get("name") if isinstance(value, dict) else value


def _type(view, value):
    found = view.meta.type(_named(value) or "")
    if found is None:
        raise invalid("issuetype", "Specify a valid issue type")
    return found.name


def _priority(view, value):
    found = view.meta.priority(_named(value) or "")
    if found is None:
        raise invalid("priority", f"Priority name '{_named(value)}' is not valid")
    return found.name


def _user(view, value, field: str):
    given, row = users.find_in(view.db, value)
    if given and row is None:
        raise invalid(field, f"User '{_named(value) or value}' does not exist.")
    return row["name"] if row else None


def _parent(view, value):
    ref = value.get("key") or value.get("id") if isinstance(value, dict) else value
    return records.issue(view.db, ref)["key"] if ref else None


def _sprint(view, value):
    if isinstance(value, list):
        value = value[-1] if value else None
    if isinstance(value, dict):
        value = value.get("id")
    return records.sprint(view.db, value)["id"] if value is not None else None


READERS = {"summary": lambda v, x: str(x or ""), "description": lambda v, x: rich(v, x, "description"),
           "issuetype": _type, "priority": _priority, "assignee": lambda v, x: _user(v, x, "assignee"),
           "reporter": lambda v, x: _user(v, x, "reporter"), "labels": lambda v, x: [str(s) for s in x or []],
           "duedate": lambda v, x: x or None, "parent": _parent, SPRINT_FIELD: _sprint}


def read(view, fields: dict, allow_project: bool) -> dict:
    """`{field: value}` as the store keeps them; unknown or unsettable fields are refused."""
    out = {}
    for name, value in fields.items():
        if name == "project" and allow_project:
            continue
        if name not in settable() or name == "project":
            raise invalid(name, f"Field '{name}' cannot be set. It is not on the appropriate screen, or unknown.")
        out[name] = READERS[name](view, value)
    return out


def project_of(view, value):
    ref = value.get("id") or value.get("key") if isinstance(value, dict) else value
    row = records.project(view.db, ref) if ref else None
    if row is None:
        raise invalid("project", "valid project is required")
    return row
