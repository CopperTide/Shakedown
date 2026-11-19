"""Issues: create, read, edit, delete; transitions; comments; the assignee; issue links."""
from .. import fields_in, issue_shapes, records, rules, shapes
from ..errors import JiraError
from .api import created, no_content, offset_page

COMMENT_INVALID = "Comment body is not valid!"


def _issue(call):
    return records.issue(call.db, call.req.params["issueIdOrKey"])


def create_issue(call):
    row = rules.create_issue(call, call.login, call.body().get("fields") or {})
    return created({"id": str(row["id"]), "key": row["key"], "self": call.urls.api(f"issue/{row['id']}")})


def get_issue(call):
    return issue_shapes.issue(call, _issue(call), call.arg("fields"), call.arg("expand"))


def edit_issue(call):
    row, body = _issue(call), call.body()
    rules.edit_issue(call, call.login, row, body.get("fields") or {}, body.get("update") or {})
    if (call.arg("returnIssue") or "").lower() == "true":
        return issue_shapes.issue(call, records.issue(call.db, str(row["id"])))
    return no_content()


def delete_issue(call):
    rules.delete_issue(call, _issue(call))
    return no_content()


def get_transitions(call):
    out = issue_shapes.transitions(call, _issue(call))
    if "transitions.fields" in (call.arg("expand") or ""):
        out = [{**t, "fields": {}} for t in out]
    if call.arg("transitionId"):
        out = [t for t in out if t["id"] == call.arg("transitionId")]
    return {"expand": "transitions", "transitions": out}


def do_transition(call):
    body = call.body()
    transition = body.get("transition") or {}
    if not transition.get("id") and not transition.get("name"):
        raise JiraError(400, ["Missing 'transition' identifier"])
    row = _issue(call)
    tid = transition.get("id") or call.workflow.towards(row["status"], transition["name"]).id
    rules.transition(call, call.login, row, tid, dict(body.get("fields") or {}), body.get("update") or {})
    return no_content()


def _body(call) -> str:
    return fields_in.rich(call, call.body().get("body"), "comment", COMMENT_INVALID)


def list_comments(call):
    row = _issue(call)
    items = [shapes.comment(call, row["id"], c) for c in records.comments(call.db, row["id"])]
    page = offset_page(call, items, "comments", 5000, 5000)
    page.pop("isLast")
    return page


def add_comment(call):
    row = _issue(call)
    c = rules.add_comment(call, call.login, row, _body(call))
    return created(shapes.comment(call, row["id"], c))


def get_comment(call):
    row = _issue(call)
    return shapes.comment(call, row["id"], records.comment(call.db, row, call.req.params["id"]))


def edit_comment(call):
    row = _issue(call)
    c = records.comment(call.db, row, call.req.params["id"])
    return shapes.comment(call, row["id"], rules.edit_comment(call, call.login, row, c, _body(call)))


def delete_comment(call):
    row = _issue(call)
    call.db.execute("DELETE FROM comments WHERE id = ?", (records.comment(call.db, row, call.req.params["id"])["id"],))
    return no_content()


def assign(call):
    body = call.body()
    ref = next((body[k] for k in ("accountId", "name", "key") if k in body), None)
    rules.assign(call, _issue(call), ref)
    return no_content()


def _ref(value):
    return (value or {}).get("key") or (value or {}).get("id") or ""


def link(call):
    body = call.body()
    rules.link(call, body.get("type"), records.issue(call.db, _ref(body.get("inwardIssue"))),
               records.issue(call.db, _ref(body.get("outwardIssue"))))
    return created(None)


def get_link(call):
    row = records.link(call.db, call.req.params["linkId"])
    out = issue_shapes.link_entry(call, row, row["inward_id"], "inwardIssue")
    out["outwardIssue"] = issue_shapes.link_entry(call, row, row["outward_id"], "outwardIssue")["outwardIssue"]
    return out


def register(api) -> None:
    api.platform("POST", "/issue")(create_issue)
    api.platform("GET", "/issue/{issueIdOrKey}")(get_issue)
    api.platform("PUT", "/issue/{issueIdOrKey}")(edit_issue)
    api.platform("DELETE", "/issue/{issueIdOrKey}")(delete_issue)
    api.platform("GET", "/issue/{issueIdOrKey}/transitions")(get_transitions)
    api.platform("POST", "/issue/{issueIdOrKey}/transitions")(do_transition)
    api.platform("GET", "/issue/{issueIdOrKey}/comment")(list_comments)
    api.platform("POST", "/issue/{issueIdOrKey}/comment")(add_comment)
    api.platform("GET", "/issue/{issueIdOrKey}/comment/{id}")(get_comment)
    api.platform("PUT", "/issue/{issueIdOrKey}/comment/{id}")(edit_comment)
    api.platform("DELETE", "/issue/{issueIdOrKey}/comment/{id}")(delete_comment)
    api.platform("PUT", "/issue/{issueIdOrKey}/assignee")(assign)
    api.platform("POST", "/issueLink")(link)
    api.platform("GET", "/issueLink/{linkId}")(get_link)
