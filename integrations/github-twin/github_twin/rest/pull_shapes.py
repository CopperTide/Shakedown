"""The JSON of pull requests, reviews and review comments."""
from .. import ci, merging, records
from ..repos import full_name
from ..times import iso
from ..users import node_id
from . import shapes
from .issue_shapes import times_of, label, user


def protection(v, repo) -> dict:
    """The repo's protection from the config (`protection: {<repo>: {required_reviews}}`)."""
    return (v.config.get("protection") or {}).get(repo["name"]) or {}


def required_reviews(v, repo) -> int:
    return int(protection(v, repo).get("required_reviews", 0))


def base_sha(v, repo, pr) -> str:
    """An open PR's base is the base branch now; a closed one keeps what it had."""
    if pr["state"] == "open":
        return v.repos.branches(full_name(repo)).get(pr["base_ref"], pr["base_sha"])
    return pr["base_sha"]


def _side(v, repo, pr, ref: str, sha: str) -> dict:
    return {"label": f"{repo['owner']}:{ref}", "ref": ref, "sha": sha, "user": user(v, repo["owner"]),
            "repo": shapes.repository(v, repo)}


def _links(v, repo, n: int, head: str) -> dict:
    fn, u = full_name(repo), v.urls
    hrefs = {"self": u.repo_api(fn, f"/pulls/{n}"), "html": u.repo_html(fn, f"/pull/{n}"),
             "issue": u.repo_api(fn, f"/issues/{n}"), "comments": u.repo_api(fn, f"/issues/{n}/comments"),
             "review_comments": u.repo_api(fn, f"/pulls/{n}/comments"),
             "review_comment": u.repo_api(fn, "/pulls/comments{/number}"),
             "commits": u.repo_api(fn, f"/pulls/{n}/commits"), "statuses": u.repo_api(fn, f"/statuses/{head}")}
    return {k: {"href": h} for k, h in hrefs.items()}


URL_KEYS = {"url": "self", "html_url": "html", "issue_url": "issue", "comments_url": "comments",
            "review_comments_url": "review_comments", "review_comment_url": "review_comment",
            "commits_url": "commits", "statuses_url": "statuses"}


def _urls(v, repo, n: int, links: dict) -> dict:
    fn = full_name(repo)
    return {**{k: links[rel]["href"] for k, rel in URL_KEYS.items()},
            "diff_url": v.urls.repo_html(fn, f"/pull/{n}.diff"), "patch_url": v.urls.repo_html(fn, f"/pull/{n}.patch")}


def simple(v, repo, pr) -> dict:
    """The `pull-request-simple` shape."""
    n, head, base = pr["number"], pr["head_sha"], base_sha(v, repo, pr)
    assignees, links = records.assignees_of(v.db, pr["id"]), _links(v, repo, n, head)
    return {"id": pr["id"], "node_id": node_id("PullRequest", pr["id"]), "number": n, "state": pr["state"],
            "locked": bool(pr["locked"]), "title": pr["title"], "body": pr["body"], "user": user(v, pr["user"]),
            "labels": [label(v, repo, x) for x in records.labels_of(v.db, pr["id"])], "milestone": None,
            "active_lock_reason": None, "merged_at": iso(pr["merged_ms"]) if pr["merged_ms"] else None,
            "merge_commit_sha": pr["merge_commit_sha"], "assignee": user(v, assignees[0]) if assignees else None,
            "assignees": [user(v, a) for a in assignees],
            "requested_reviewers": [user(v, r) for r in records.requested(v.db, pr["id"])], "requested_teams": [],
            "head": _side(v, repo, pr, pr["head_ref"], head), "base": _side(v, repo, pr, pr["base_ref"], base),
            "_links": links, "author_association": "MEMBER", "auto_merge": None,
            "draft": bool(pr["draft"]), **times_of(pr), **_urls(v, repo, n, links)}


def mergeability(v, repo, pr) -> tuple[bool | None, str]:
    """`mergeable` and `mergeable_state` as GitHub reports them."""
    if pr["state"] != "open":
        return None, "unknown"
    m = merging.mergeable(v.repos, full_name(repo), base_sha(v, repo, pr), pr["head_sha"])
    if m.state == merging.CONFLICTING:
        return False, "dirty"
    if pr["draft"]:
        return True, "draft"
    if len(records.approvals_on(v.db, pr["id"], pr["head_sha"])) < required_reviews(v, repo):
        return True, "blocked"
    checks = ci.head_checks(v.db, repo["id"], pr["head_sha"])
    return True, "unstable" if checks in ("failure", "pending") else "clean"


def full(v, repo, pr) -> dict:
    """The `pull-request` shape: the simple one with merge state and the change's size."""
    fn = full_name(repo)
    base = base_sha(v, repo, pr)
    mb = v.repos.merge_base(fn, base, pr["head_sha"]) or base
    files = v.repos.git(fn, "diff", "--numstat", mb, pr["head_sha"]).splitlines()
    nums = [line.split("\t") for line in files]
    mergeable, state = mergeability(v, repo, pr)
    return {**simple(v, repo, pr), "merged": bool(pr["merged"]), "mergeable": mergeable, "rebaseable": mergeable,
            "mergeable_state": state, "merged_by": user(v, pr["merged_by"]),
            "comments": records.comment_count(v.db, pr["id"]),
            "review_comments": records.review_comment_count(v.db, pr["id"]), "maintainer_can_modify": bool(pr["maintainer_can_modify"]),
            "commits": len(v.repos.rev_list(fn, f"{mb}..{pr['head_sha']}")), "changed_files": len(nums),
            "additions": sum(int(a) for a, *_ in nums if a != "-"), "deletions": sum(int(d) for _, d, *_ in nums if d != "-")}


def review(v, repo, pr, r) -> dict:
    fn, n = full_name(repo), pr["number"]
    html = v.urls.repo_html(fn, f"/pull/{n}#pullrequestreview-{r['id']}")
    out = {"id": r["id"], "node_id": node_id("PullRequestReview", r["id"]), "user": user(v, r["user"]),
           "body": r["body"], "state": r["state"], "commit_id": r["commit_id"], "html_url": html,
           "pull_request_url": v.urls.repo_api(fn, f"/pulls/{n}"), "author_association": "MEMBER",
           "_links": {"html": {"href": html}, "pull_request": {"href": v.urls.repo_api(fn, f"/pulls/{n}")}}}
    if r["submitted_ms"]:
        out["submitted_at"] = iso(r["submitted_ms"])
    return out


def review_comment(v, repo, pr, c) -> dict:
    fn, n = full_name(repo), pr["number"]
    api, html = v.urls.repo_api(fn, f"/pulls/comments/{c['id']}"), v.urls.repo_html(fn, f"/pull/{n}#discussion_r{c['id']}")
    out = {"id": c["id"], "node_id": node_id("PullRequestReviewComment", c["id"]), "url": api, "html_url": html,
           "pull_request_review_id": c["review_id"], "diff_hunk": c["diff_hunk"], "path": c["path"],
           "commit_id": c["commit_id"], "original_commit_id": c["commit_id"], "user": user(v, c["user"]),
           "body": c["body"], "line": c["line"], "side": c["side"] or "RIGHT", "author_association": "MEMBER",
           "pull_request_url": v.urls.repo_api(fn, f"/pulls/{n}"), "created_at": iso(c["created_ms"]),
           "updated_at": iso(c["updated_ms"]),
           "_links": {"self": {"href": api}, "html": {"href": html}, "pull_request": {"href": v.urls.repo_api(fn, f"/pulls/{n}")}}}
    if c["in_reply_to"]:
        out["in_reply_to_id"] = c["in_reply_to"]
    return out
