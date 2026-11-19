"""A repository and its branches."""
from . import shapes
from .api import not_found, paged
from .pull_shapes import required_reviews


def _protected(call, repo) -> bool:
    return required_reviews(call, repo) > 0


def get_repo(call):
    return shapes.repository(call, call.repo())


def list_branches(call):
    repo = call.repo()
    fn = shapes.full_name(repo)
    items = [{"name": b, "commit": {"sha": sha, "url": call.urls.repo_api(fn, f"/commits/{sha}")},
              "protected": _protected(call, repo)} for b, sha in sorted(call.repos.branches(fn).items())]
    return paged(call, items)


def get_branch(call):
    repo, branch = call.repo(), call.req.params["branch"]
    fn = shapes.full_name(repo)
    sha = call.repos.branches(fn).get(branch)
    if sha is None:
        raise not_found()
    info = call.repos.commit_info(fn, sha)
    api = call.urls.repo_api(fn, f"/branches/{branch}")
    return {"name": branch, "commit": shapes.commit(call, repo, info), "protected": _protected(call, repo),
            "_links": {"self": api, "html": call.urls.repo_html(fn, f"/tree/{branch}")},
            "protection": {"enabled": _protected(call, repo)}, "protection_url": f"{api}/protection"}


def register(api) -> None:
    api.route("GET", "/repos/{owner}/{repo}")(get_repo)
    api.route("GET", "/repos/{owner}/{repo}/branches")(list_branches)
    api.route("GET", "/repos/{owner}/{repo}/branches/{branch:path}")(get_branch)
