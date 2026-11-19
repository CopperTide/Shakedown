"""Commits: a branch's history, one commit with its files, and the comparison of two."""
from ..diffs import changed_files
from ..repos import full_name
from . import shapes
from .api import invalid, not_found, paged


def _resolve(call, repo, ref: str) -> str:
    sha = call.repos.ref(full_name(repo), ref)
    if sha is None:
        raise invalid(f"No commit found for SHA: {ref}")
    return sha


def list_commits(call):
    repo = call.repo()
    fn = full_name(repo)
    head = call.repos.ref(fn, call.req.arg("sha") or repo["default_branch"])
    if head is None:
        raise not_found()
    shas = call.repos.rev_list(fn, head, call.req.arg("path", "") or "")
    resp = paged(call, shas)
    resp.body = [shapes.commit(call, repo, call.repos.commit_info(fn, s)) for s in resp.body]
    return resp


def with_files(call, repo, sha: str) -> dict:
    fn = full_name(repo)
    info = call.repos.commit_info(fn, sha)
    files = changed_files(call.repos, fn, info.parents[0] if info.parents else None, sha)
    return shapes.commit(call, repo, info, files)


def get_commit(call):
    repo = call.repo()
    return with_files(call, repo, _resolve(call, repo, call.req.params["ref"]))


def _status(ahead: int, behind: int) -> str:
    if ahead and behind:
        return "diverged"
    return "ahead" if ahead else "behind" if behind else "identical"


def compare(call):
    repo, basehead = call.repo(), call.req.params["basehead"]
    if "..." not in basehead:
        raise not_found()
    base_ref, head_ref = basehead.split("...", 1)
    fn = full_name(repo)
    base, head = call.repos.ref(fn, base_ref), call.repos.ref(fn, head_ref)
    if base is None or head is None:
        raise not_found()
    mb = call.repos.merge_base(fn, base, head)
    ahead, behind = call.repos.ahead_behind(fn, base, head)
    shas = list(reversed(call.repos.rev_list(fn, f"{base}..{head}")))
    files = changed_files(call.repos, fn, mb, head)
    u, path = call.urls, f"/compare/{basehead}"
    return {"url": u.repo_api(fn, path), "html_url": u.repo_html(fn, path), "permalink_url": u.repo_html(fn, f"/compare/{base}...{head}"),
            "diff_url": u.repo_html(fn, f"{path}.diff"), "patch_url": u.repo_html(fn, f"{path}.patch"),
            "base_commit": shapes.commit(call, repo, call.repos.commit_info(fn, base)),
            "merge_base_commit": shapes.commit(call, repo, call.repos.commit_info(fn, mb or base)),
            "status": _status(ahead, behind), "ahead_by": ahead, "behind_by": behind, "total_commits": len(shas),
            "commits": [shapes.commit(call, repo, call.repos.commit_info(fn, s)) for s in shas],
            "files": [shapes.diff_entry(call, repo, head, f) for f in files]}


def register(api) -> None:
    api.route("GET", "/repos/{owner}/{repo}/commits")(list_commits)
    api.route("GET", "/repos/{owner}/{repo}/commits/{ref:path}")(get_commit)
    api.route("GET", "/repos/{owner}/{repo}/compare/{basehead:path}")(compare)


