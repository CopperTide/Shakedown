"""One sample request per covered operation, for the schema test: `exercise(twin, op, v)` answers
`(status, body)` for platform API version `v` (agile operations ignore it)."""
from conftest import api, call

ADF = {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "x"}]}]}


def _text(v):
    return ADF if v == "3" else "x"


def _account(t):
    return api(t, "GET", "myself")["accountId"]


def _comment(t, v):
    return api(t, "POST", "issue/AC-1/comment", {"body": _text(v)}, v=v)["id"]


def _link(t):
    call(t, "POST", "/rest/api/2/issueLink", {"type": {"name": "Blocks"}, "inwardIssue": {"key": "AC-3"},
                                              "outwardIssue": {"key": "AC-1"}})
    return api(t, "GET", "issue/AC-1")["fields"]["issuelinks"][0]["id"]


PLATFORM = {
    ("GET", "/user"): lambda t, v: f"user?accountId={_account(t)}",
    ("GET", "/user/search"): lambda t, v: "user/search?query=re",
    ("GET", "/project/{projectIdOrKey}"): lambda t, v: "project/AC?expand=issueTypes",
    ("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes"): lambda t, v: "issue/createmeta/AC/issuetypes",
    ("GET", "/issue/createmeta/{projectIdOrKey}/issuetypes/{issueTypeId}"): lambda t, v: "issue/createmeta/AC/issuetypes/10003",
    ("GET", "/issuetype/{id}"): lambda t, v: "issuetype/10003",
    ("GET", "/status/{idOrName}"): lambda t, v: "status/10000",
    ("GET", "/statuscategory/{idOrKey}"): lambda t, v: "statuscategory/2",
    ("GET", "/priority/{id}"): lambda t, v: "priority/1",
    ("GET", "/resolution/{id}"): lambda t, v: "resolution/10000",
    ("GET", "/issueLinkType/{issueLinkTypeId}"): lambda t, v: "issueLinkType/10000",
    ("GET", "/issue/{issueIdOrKey}"): lambda t, v: (_comment(t, v), _link(t), "issue/AC-1?expand=transitions")[2],
    ("PUT", "/issue/{issueIdOrKey}"): lambda t, v: ("issue/AC-1", {"fields": {"summary": "y"}}),
    ("DELETE", "/issue/{issueIdOrKey}"): lambda t, v: "issue/AC-4",
    ("GET", "/issue/{issueIdOrKey}/transitions"): lambda t, v: "issue/AC-1/transitions?expand=transitions.fields",
    ("POST", "/issue/{issueIdOrKey}/transitions"): lambda t, v: ("issue/AC-1/transitions", {"transition": {"id": "41"}}),
    ("GET", "/issue/{issueIdOrKey}/comment"): lambda t, v: (_comment(t, v), "issue/AC-1/comment")[1],
    ("POST", "/issue/{issueIdOrKey}/comment"): lambda t, v: ("issue/AC-1/comment", {"body": _text(v)}),
    ("GET", "/issue/{issueIdOrKey}/comment/{id}"): lambda t, v: f"issue/AC-1/comment/{_comment(t, v)}",
    ("PUT", "/issue/{issueIdOrKey}/comment/{id}"): lambda t, v: (f"issue/AC-1/comment/{_comment(t, v)}", {"body": _text(v)}),
    ("DELETE", "/issue/{issueIdOrKey}/comment/{id}"): lambda t, v: f"issue/AC-1/comment/{_comment(t, v)}",
    ("PUT", "/issue/{issueIdOrKey}/assignee"): lambda t, v: ("issue/AC-1/assignee", {"accountId": _account(t)}),
    ("POST", "/issue"): lambda t, v: ("issue", {"fields": {"project": {"key": "AC"}, "summary": "s",
                                                           "issuetype": {"name": "Task"}, "description": _text(v)}}),
    ("POST", "/issueLink"): lambda t, v: ("issueLink", {"type": {"name": "Blocks"}, "inwardIssue": {"key": "AC-3"},
                                                        "outwardIssue": {"key": "AC-1"}}),
    ("GET", "/issueLink/{linkId}"): lambda t, v: f"issueLink/{_link(t)}",
    ("GET", "/search/jql"): lambda t, v: "search/jql?jql=project%3DAC&fields=*all&maxResults=2",
    ("POST", "/search/jql"): lambda t, v: ("search/jql", {"jql": "project = AC", "fields": ["summary", "status"]}),
    ("GET", "/search"): lambda t, v: "search?jql=project%3DAC&maxResults=2",
    ("POST", "/search"): lambda t, v: ("search", {"jql": "project = AC", "fields": ["summary"]}),
    ("POST", "/search/approximate-count"): lambda t, v: ("search/approximate-count", {"jql": "project = AC"}),
}

AGILE = {
    ("GET", "/board/{boardId}"): "board/1",
    ("GET", "/board/{boardId}/sprint"): "board/1/sprint",
    ("GET", "/board/{boardId}/issue"): "board/1/issue",
    ("GET", "/board/{boardId}/backlog"): "board/1/backlog",
    ("POST", "/sprint"): ("sprint", {"name": "Next", "originBoardId": 1, "goal": "g"}),
    ("GET", "/sprint/{sprintId}"): "sprint/1",
    ("PUT", "/sprint/{sprintId}"): ("sprint/2", {"name": "Renamed"}),
    ("POST", "/sprint/{sprintId}"): ("sprint/2", {"goal": "New goal"}),
    ("GET", "/sprint/{sprintId}/issue"): "sprint/1/issue",
    ("POST", "/sprint/{sprintId}/issue"): ("sprint/1/issue", {"issues": ["AC-4"]}),
    ("POST", "/backlog/issue"): ("backlog/issue", {"issues": ["AC-1"]}),
    ("PUT", "/issue/rank"): ("issue/rank", {"issues": ["AC-4"], "rankBeforeIssue": "AC-3"}),
}


def _send(t, method, prefix, target):
    path, body = target if isinstance(target, tuple) else (target, None)
    r = call(t, method, f"{prefix}/{path}", body)
    return r.status, r.json()


def exercise(t, op, v="2"):
    api_name, method, path = op
    if api_name == "agile":
        return _send(t, method, "/rest/agile/1.0", AGILE.get((method, path), path.lstrip("/")))
    make = PLATFORM.get((method, path))
    return _send(t, method, f"/rest/api/{v}", make(t, v) if make else path.lstrip("/"))
