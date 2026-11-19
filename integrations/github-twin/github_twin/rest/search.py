"""`GET /search/issues` over a subset of GitHub's query syntax: `repo:`, `is:pr|issue|open|closed|merged`,
`type:`, `state:`, `author:`, `assignee:`, `label:`, `head:`, `base:`, `review-requested:`, and words in the
title or body; `@me` is the caller."""
from .. import records
from . import issue_shapes
from .api import invalid, paged

QUALIFIERS = {"repo", "is", "type", "state", "author", "assignee", "label", "head", "base", "in", "sort", "user", "org",
              "review-requested"}


def parse(q: str, me: str = "") -> tuple[dict, list]:
    quals, words = {}, []
    for token in q.split():
        key, sep, value = token.partition(":")
        if sep and key in QUALIFIERS:
            value = value.strip('"')
            quals.setdefault(key, []).append(me if value == "@me" else value)
        else:
            words.append(token.lower())
    return quals, words


def _is(row, pull, value: str) -> bool:
    checks = {"pr": bool(row["is_pull"]), "pull-request": bool(row["is_pull"]), "issue": not row["is_pull"],
              "open": row["state"] == "open", "closed": row["state"] == "closed",
              "merged": bool(pull and pull["merged"]), "unmerged": bool(pull and not pull["merged"])}
    return checks.get(value, True)


def _field(db, row, pull, key: str, value: str) -> bool:
    if key in ("is", "type"):
        return _is(row, pull, value)
    if key == "state":
        return row["state"] == value
    if key == "author":
        return row["user"] == value
    if key == "assignee":
        return value in records.assignees_of(db, row["id"])
    if key == "label":
        return value in {x["name"] for x in records.labels_of(db, row["id"])}
    if key in ("head", "base"):
        return bool(pull) and pull[f"{key}_ref"] == value
    if key == "review-requested":
        return value in records.requested(db, row["id"])
    return True


def _match(db, row, pull, quals: dict, words: list) -> bool:
    text = f"{row['title']} {row['body'] or ''}".lower()
    return all(w in text for w in words) and all(
        _field(db, row, pull, k, v) for k, vs in quals.items() for v in vs)


def _rows(v, quals: dict) -> list:
    """(repo, issue, pull or None) of the repos the query names (else every repo)."""
    names = quals.get("repo") or [f"{r['owner']}/{r['name']}" for r in v.db.execute("SELECT * FROM repos")]
    out = []
    for name in names:
        repo = records.repo_named(v.db, name)
        if repo is None:
            continue
        for row in v.db.execute("SELECT * FROM issues WHERE repo_id = ? ORDER BY number DESC", (repo["id"],)):
            pull = v.db.execute("SELECT * FROM pulls WHERE issue_id = ?", (row["id"],)).fetchone()
            out.append((repo, row, pull))
    return out


def _item(call, repo, row) -> dict:
    """An `issue-search-result-item`: the issue, a score, and a body that is never null."""
    out = issue_shapes.issue(call, repo, row)
    return {**out, "body": out["body"] or "", "score": 1.0}


def find(v, q: str) -> list:
    """(repo, issue row) of what the query `q` finds, newest first in each repo."""
    quals, words = parse(q, v.login)
    return [(repo, row) for repo, row, pull in _rows(v, quals) if _match(v.db, row, pull, quals, words)]


def search_issues(call):
    q = call.req.arg("q")
    if not q:
        raise invalid("Validation Failed", [{"resource": "Search", "field": "q", "code": "missing"}])
    found = find(call, q)
    resp = paged(call, found)
    items = [_item(call, repo, row) for repo, row in resp.body]
    resp.body = {"total_count": len(found), "incomplete_results": False, "items": items, "search_type": "lexical"}
    return resp


def register(api) -> None:
    api.route("GET", "/search/issues")(search_issues)
