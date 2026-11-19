"""Server info, the caller, users, projects and the metadata lists an issue refers to."""
from .. import meta, records, shapes, times, users
from ..errors import not_found
from ..workflow import category_by
from .api import offset_page

CLOUD_VERSION = ("1001.0.0-SNAPSHOT", [1001, 0, 0])
SERVER_VERSION = ("10.3.0", [10, 3, 0])
NO_USER = "Specified user does not exist or you do not have required permissions"


def server_info(call) -> dict:
    version, numbers = CLOUD_VERSION if call.cloud else SERVER_VERSION
    now = times.stamp(call.now())
    return {"baseUrl": call.base, "displayUrl": call.base, "version": version, "versionNumbers": numbers,
            "deploymentType": "Cloud" if call.cloud else "Server", "buildNumber": 100000, "buildDate": now,
            "serverTime": now, "scmInfo": "jira-twin", "serverTitle": "Jira"}


def _user_param(call):
    for name in ("accountId", "username", "key"):
        if call.arg(name):
            return call.arg(name)
    return None


def user(call) -> dict:
    row = users.find(call.db, _user_param(call) or "")
    if row is None:
        raise not_found(NO_USER)
    return users.shape(call.urls, call.cloud, row)


def user_search(call) -> list:
    text = call.arg("query") or call.arg("username") or call.arg("accountId") or ""
    rows = users.search(call.db, text) if text else []
    start, size = call.int_arg("startAt", 0), call.page("maxResults", 50, 1000)
    return [users.shape(call.urls, call.cloud, r) for r in rows[start:start + size]]


def _found(value, kind: str):
    if value is None:
        raise not_found(f"The {kind} does not exist.")
    return value


def _project(call) -> dict:
    return shapes.project(call, records.project_or_404(call.db, call.req.params["projectIdOrKey"]), full=True)


def _createmeta_types(call) -> dict:
    records.project_or_404(call.db, call.req.params["projectIdOrKey"])
    items = [shapes.issue_type(call, t) for t in call.meta.types]
    return offset_page(call, items, "issueTypes" if call.cloud else "values", 50, 200)


def _field_meta(field: dict, required: bool) -> dict:
    return {"required": required, "schema": field["schema"], "name": field["name"], "key": field["key"],
            "fieldId": field["id"], "hasDefaultValue": False, "operations": ["set"]}


def _createmeta_fields(call) -> dict:
    records.project_or_404(call.db, call.req.params["projectIdOrKey"])
    _found(call.meta.type(call.req.params["issueTypeId"]), "issue type")
    needed = {"summary", "issuetype", "project"}
    items = [_field_meta(f, f["id"] in needed) for f in meta.fields() if f["id"] in meta.settable()]
    return offset_page(call, items, "fields" if call.cloud else "values", 50, 200)


def _lists(api) -> None:
    api.platform("GET", "/issuetype")(lambda c: [shapes.issue_type(c, t) for t in c.meta.types])
    api.platform("GET", "/issuetype/{id}")(
        lambda c: shapes.issue_type(c, _found(c.meta.type(c.req.params["id"]), "issue type")))
    api.platform("GET", "/status")(lambda c: [shapes.status(c, s) for s in c.workflow.statuses])
    api.platform("GET", "/status/{idOrName}")(
        lambda c: shapes.status(c, _found(c.workflow.status(c.req.params["idOrName"]), "status")))
    api.platform("GET", "/statuscategory/{idOrKey}")(
        lambda c: shapes.status_category(c, _found(category_by(c.req.params["idOrKey"]), "status category")))
    api.platform("GET", "/priority")(lambda c: [shapes.priority(c, p) for p in c.meta.priorities])
    api.platform("GET", "/priority/{id}")(
        lambda c: shapes.priority(c, _found(c.meta.priority(c.req.params["id"]), "priority")))
    api.platform("GET", "/resolution")(lambda c: [shapes.resolution(c, r) for r in c.meta.resolutions])
    api.platform("GET", "/resolution/{id}")(
        lambda c: shapes.resolution(c, _found(c.meta.resolution(c.req.params["id"]), "resolution")))
    api.platform("GET", "/issueLinkType")(
        lambda c: {"issueLinkTypes": [shapes.link_type(c, t) for t in c.meta.link_types]})
    api.platform("GET", "/issueLinkType/{issueLinkTypeId}")(
        lambda c: shapes.link_type(c, _found(c.meta.link_type(c.req.params["issueLinkTypeId"]), "issue link type")))
    api.platform("GET", "/field")(lambda c: meta.fields())


def register(api) -> None:
    api.platform("GET", "/serverInfo", anonymous=True)(server_info)
    api.platform("GET", "/myself")(lambda c: users.shape(c.urls, c.cloud, c.user))
    api.platform("GET", "/user")(user)
    api.platform("GET", "/user/search")(user_search)
    api.platform("GET", "/project")(lambda c: [shapes.project(c, r) for r in records.projects(c.db)])
    api.platform("GET", "/project/{projectIdOrKey}")(_project)
    api.platform("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes")(_createmeta_types)
    api.platform("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes/{issueTypeId}")(_createmeta_fields)
    _lists(api)
