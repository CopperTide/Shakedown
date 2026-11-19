"""The JSON shapes several operations share: repositories, commits and git commits."""
from .. import users
from ..diffs import FileChange
from ..repos import full_name
from ..times import iso
from ..users import node_id

REPO_TEMPLATES = {
    "archive_url": "/{archive_format}{/ref}", "assignees_url": "/assignees{/user}", "blobs_url": "/git/blobs{/sha}",
    "branches_url": "/branches{/branch}", "collaborators_url": "/collaborators{/collaborator}",
    "comments_url": "/comments{/number}", "commits_url": "/commits{/sha}", "compare_url": "/compare/{base}...{head}",
    "contents_url": "/contents/{+path}", "contributors_url": "/contributors", "deployments_url": "/deployments",
    "downloads_url": "/downloads", "events_url": "/events", "forks_url": "/forks", "git_commits_url": "/git/commits{/sha}",
    "git_refs_url": "/git/refs{/sha}", "git_tags_url": "/git/tags{/sha}", "hooks_url": "/hooks",
    "issue_comment_url": "/issues/comments{/number}", "issue_events_url": "/issues/events{/number}",
    "issues_url": "/issues{/number}", "keys_url": "/keys{/key_id}", "labels_url": "/labels{/name}",
    "languages_url": "/languages", "merges_url": "/merges", "milestones_url": "/milestones{/number}",
    "notifications_url": "/notifications{?since,all,participating}", "pulls_url": "/pulls{/number}",
    "releases_url": "/releases{/id}", "stargazers_url": "/stargazers", "statuses_url": "/statuses/{sha}",
    "subscribers_url": "/subscribers", "subscription_url": "/subscription", "tags_url": "/tags", "teams_url": "/teams",
    "trees_url": "/git/trees{/sha}",
}
PERMISSIONS = {"admin": True, "maintain": True, "push": True, "triage": True, "pull": True}
UNSIGNED = {"verified": False, "reason": "unsigned", "signature": None, "payload": None, "verified_at": None}


def repository(call, repo) -> dict:
    """The `full-repository` shape."""
    fn, u, n_open = full_name(repo), call.urls, open_issues(call, repo)
    owner = users.find(call.db, repo["owner"])
    counts = {k: 0 for k in ("forks", "forks_count", "stargazers_count", "watchers", "watchers_count", "size",
                             "network_count", "subscribers_count")}
    flags = {k: True for k in ("has_issues", "has_projects", "has_wiki")}
    flags.update({"has_downloads": True})
    flags.update({k: False for k in ("has_pages", "has_discussions", "archived", "disabled", "fork", "is_template")})
    return {"id": repo["id"], "node_id": node_id("Repository", repo["id"]), "name": repo["name"], "full_name": fn,
            "owner": users.simple_user(u, owner), "private": bool(repo["private"]),
            "visibility": "private" if repo["private"] else "public", "description": repo["description"],
            "url": u.repo_api(fn), "html_url": u.repo_html(fn), "clone_url": u.git(repo["owner"], repo["name"]),
            "git_url": f"git://{u.host}/{fn}.git", "ssh_url": f"git@{u.host}:{fn}.git", "svn_url": u.repo_html(fn),
            "mirror_url": None, "homepage": None, "language": None, "license": None,
            "default_branch": repo["default_branch"], "open_issues": n_open,
            "open_issues_count": n_open, "permissions": dict(PERMISSIONS),
            "created_at": iso(repo["created_ms"]), "updated_at": iso(repo["updated_ms"]),
            "pushed_at": iso(repo["pushed_ms"]), **counts, **flags,
            **{k: u.repo_api(fn, v) for k, v in REPO_TEMPLATES.items()}}


def open_issues(call, repo) -> int:
    return call.db.execute("SELECT COUNT(*) FROM issues WHERE repo_id = ? AND state = 'open'", (repo["id"],)).fetchone()[0]


def _author_user(call, name: str):
    row = users.find(call.db, name)
    return users.simple_user(call.urls, row) if row is not None else None


def person(p) -> dict:
    return {"name": p.name, "email": p.email, "date": iso(p.at_ms)}


def parent(call, fn: str, sha: str) -> dict:
    return {"sha": sha, "url": call.urls.repo_api(fn, f"/commits/{sha}"), "html_url": call.urls.repo_html(fn, f"/commit/{sha}")}


def commit(call, repo, info, files: list | None = None) -> dict:
    """The `commit` shape; with `files`, also their changes and the line counts."""
    fn, u = full_name(repo), call.urls
    out = {"sha": info.sha, "node_id": node_id("Commit", int(info.sha[:7], 16)),
           "url": u.repo_api(fn, f"/commits/{info.sha}"), "html_url": u.repo_html(fn, f"/commit/{info.sha}"),
           "comments_url": u.repo_api(fn, f"/commits/{info.sha}/comments"),
           "commit": {"url": u.repo_api(fn, f"/git/commits/{info.sha}"), "author": person(info.author),
                      "committer": person(info.committer), "message": info.message, "comment_count": 0,
                      "tree": {"sha": info.tree, "url": u.repo_api(fn, f"/git/trees/{info.tree}")},
                      "verification": dict(UNSIGNED)},
           "author": _author_user(call, info.author.name), "committer": _author_user(call, info.committer.name),
           "parents": [parent(call, fn, p) for p in info.parents]}
    if files is not None:
        adds, dels = sum(f.additions for f in files), sum(f.deletions for f in files)
        out["stats"] = {"additions": adds, "deletions": dels, "total": adds + dels}
        out["files"] = [diff_entry(call, repo, info.sha, f) for f in files]
    return out


def diff_entry(call, repo, ref: str, f: FileChange) -> dict:
    fn, u = full_name(repo), call.urls
    out = {"sha": f.sha, "filename": f.filename, "status": f.status, "additions": f.additions,
           "deletions": f.deletions, "changes": f.additions + f.deletions,
           "blob_url": u.repo_html(fn, f"/blob/{ref}/{f.filename}"), "raw_url": u.repo_html(fn, f"/raw/{ref}/{f.filename}"),
           "contents_url": u.repo_api(fn, f"/contents/{f.filename}?ref={ref}")}
    if f.patch is not None:
        out["patch"] = f.patch
    if f.previous_filename:
        out["previous_filename"] = f.previous_filename
    return out


def git_commit(call, repo, info) -> dict:
    """The `git-commit` shape (`/git/commits`, and the commit of a contents write)."""
    fn, u = full_name(repo), call.urls
    return {"sha": info.sha, "node_id": node_id("Commit", int(info.sha[:7], 16)),
            "url": u.repo_api(fn, f"/git/commits/{info.sha}"), "html_url": u.repo_html(fn, f"/commit/{info.sha}"),
            "author": person(info.author), "committer": person(info.committer), "message": info.message,
            "tree": {"sha": info.tree, "url": u.repo_api(fn, f"/git/trees/{info.tree}")},
            "parents": [{**parent(call, fn, p), "url": u.repo_api(fn, f"/git/commits/{p}")} for p in info.parents],
            "verification": dict(UNSIGNED)}
