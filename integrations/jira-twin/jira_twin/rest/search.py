"""Search: Cloud's `search/jql` (token pages, at most 100 an answer; the last page has no token),
Server's offset `search` (with `total`; Cloud answers it 410 Gone), and the approximate count. Every
search is a read, POST too."""
import base64
import json

from .. import finder, issue_shapes
from ..errors import JiraError, bad
from .api import not_found

PAGE_CAP = 100
GONE = ("The requested API has been removed. Please migrate to the /rest/api/3/search/jql API. A full migration "
        "guideline is available at https://developer.atlassian.com/changelog/#CHANGE-2046")


def _params(call) -> dict:
    """GET query parameters or a POST body, as one dict."""
    if call.req.method == "POST":
        return {k: v for k, v in call.body().items() if v is not None}
    return {k: vs[0] for k, vs in call.req.query.items()}


def _token(offset: int) -> str:
    return base64.urlsafe_b64encode(json.dumps({"o": offset}).encode()).decode()


def _offset(token) -> int:
    if not token:
        return 0
    try:
        return int(json.loads(base64.urlsafe_b64decode(str(token).encode()))["o"])
    except (ValueError, KeyError, TypeError):
        raise bad("The nextPageToken is not valid.") from None


def _size(params: dict, default: int, cap: int) -> int:
    try:
        return max(0, min(int(params.get("maxResults", default)), cap))
    except (TypeError, ValueError):
        raise bad("The 'maxResults' parameter must be a number.") from None


def search_jql(call):
    if not call.cloud:
        return not_found(call.req)
    p = _params(call)
    rows = finder.find(call, call.login, p.get("jql", ""))
    start, size = _offset(p.get("nextPageToken")), _size(p, 50, PAGE_CAP)
    chunk = rows[start:start + size]
    out = {"issues": [issue_shapes.issue(call, r, p.get("fields"), p.get("expand"), default="id") for r in chunk]}
    if start + size < len(rows):
        out["nextPageToken"] = _token(start + size)
    out["isLast"] = start + size >= len(rows)
    return out


def search_offset(call):
    if call.cloud:
        raise JiraError(410, [GONE])
    p = _params(call)
    rows = finder.find(call, call.login, p.get("jql", ""))
    start, size = int(p.get("startAt", 0) or 0), _size(p, 50, 1000)
    issues = [issue_shapes.issue(call, r, p.get("fields"), p.get("expand"), default="*navigable")
              for r in rows[start:start + size]]
    return {"expand": "schema,names", "startAt": start, "maxResults": size, "total": len(rows), "issues": issues}


def count(call):
    return {"count": len(finder.find(call, call.login, call.body().get("jql", "")))}


def register(api) -> None:
    for method in ("GET", "POST"):
        api.platform(method, "/search/jql", read=True)(search_jql)
        api.platform(method, "/search", read=True)(search_offset)
    api.platform("POST", "/search/approximate-count", read=True)(count)
