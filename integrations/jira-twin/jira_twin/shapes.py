"""The JSON Jira answers with for statuses, issue types, priorities, resolutions, projects, link
types and comments. Every object a client may follow has an absolute `self`."""
from . import adf, times, users
from .workflow import category

ICONS = "/images/icons"


def status_category(view, key: str) -> dict:
    id_, name, colour = category(key)
    return {"self": view.urls.api(f"statuscategory/{id_}"), "id": id_, "key": key, "colorName": colour, "name": name}


def status(view, s) -> dict:
    return {"self": view.urls.api(f"status/{s.id}"), "description": "", "iconUrl": f"{view.base}{ICONS}/status.png",
            "name": s.name, "untranslatedName": s.name, "id": s.id, "statusCategory": status_category(view, s.category)}


def status_named(view, name: str) -> dict:
    return status(view, view.workflow.status(name))


def issue_type(view, t) -> dict:
    return {"self": view.urls.api(f"issuetype/{t.id}"), "id": t.id, "description": "",
            "iconUrl": f"{view.base}{ICONS}/issuetype/{t.id}.svg", "name": t.name, "subtask": t.subtask,
            "avatarId": int(t.id), "hierarchyLevel": t.level}


def priority(view, p) -> dict:
    return {"self": view.urls.api(f"priority/{p.id}"), "iconUrl": f"{view.base}{ICONS}/priorities/{p.id}.svg",
            "name": p.name, "id": p.id, "description": "", "statusColor": "#cccccc"}


def resolution(view, r) -> dict:
    return {"self": view.urls.api(f"resolution/{r.id}"), "id": r.id, "description": "", "name": r.name}


def project(view, row, full: bool = False) -> dict:
    out = {"expand": "description,lead,issueTypes,url,projectKeys,permissions,insight",
           "self": view.urls.api(f"project/{row['id']}"), "id": str(row["id"]), "key": row["key"],
           "name": row["name"], "avatarUrls": view.urls.avatars("project", row["id"]),
           "projectTypeKey": row["type"], "simplified": False, "style": "classic", "isPrivate": False,
           "properties": {}}
    if full:
        lead = users.by_name(view.db, row["lead"]) if row["lead"] else None
        out.update({"description": "", "components": [], "versions": [], "roles": {}, "assigneeType": "UNASSIGNED",
                    "issueTypes": [issue_type(view, t) for t in view.meta.types]})
        if lead is not None:
            out["lead"] = users.shape(view.urls, view.cloud, lead)
    return out


def link_type(view, t) -> dict:
    id_, name, inward, outward = t
    return {"id": id_, "name": name, "inward": inward, "outward": outward, "self": view.urls.api(f"issueLinkType/{id_}")}


def rich_out(view, text: str | None, names=None):
    """Rich text as the request's API version answers it: v3 ADF, v2 text."""
    if view.adf:
        return adf.to_adf(text, names) if text else None
    return text


def display_names(view):
    def names(ref: str):
        row = users.find(view.db, ref)
        return row["display_name"] if row else None

    return names


def user_or_none(view, name: str | None, full: bool = False):
    row = users.by_name(view.db, name) if name else None
    return users.shape(view.urls, view.cloud, row, full) if row else None


def rich_body(view, text: str):
    """A comment's body: never null (an empty v3 body is an empty document)."""
    return adf.to_adf(text, display_names(view)) if view.adf else text


def comment(view, issue_id: int, row) -> dict:
    return {"self": view.urls.api(f"issue/{issue_id}/comment/{row['id']}"), "id": str(row["id"]),
            "author": user_or_none(view, row["author"]), "body": rich_body(view, row["body"]),
            "updateAuthor": user_or_none(view, row["update_author"]), "created": times.stamp(row["created_ms"]),
            "updated": times.stamp(row["updated_ms"]), "jsdPublic": True}
