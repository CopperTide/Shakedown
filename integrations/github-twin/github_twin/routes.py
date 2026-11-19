"""The twin's router: git smart HTTP beside the REST API, and GitHub's 404 for API paths it lacks."""
from arena_part.twin import Response, Router

from . import auth, events, git_http, rest, users
from .headers import api_headers
from .repos import Repos
from .rest.api import not_found
from .urls import API


class GithubRouter(Router):
    def not_found(self, req) -> Response:
        if req.path.startswith(API):
            resp = not_found().response()
            resp.headers = {**api_headers(), **(resp.headers or {})}
            return resp
        return Response(404, "Not Found\n")


def _git(tail: str):
    def handle(req, ctx):
        if auth.check(req.headers, ctx.config.get("token", "")) != "ok":
            return git_http.challenge()
        owner, repo = req.params["owner"], req.params["repo"].removesuffix(".git")
        user = users.ensure(ctx.store.conn, ctx.actor(req), ctx.now_ms())["login"]
        if tail != "git-receive-pack":
            return git_http.handle(req, ctx.files, owner, repo, tail, user)
        return _push(req, ctx, owner, repo, user)

    return handle


def _push(req, ctx, owner: str, repo: str, user: str) -> Response:
    row = ctx.store.conn.execute("SELECT * FROM repos WHERE owner = ? AND name = ?", (owner, repo)).fetchone()
    if row is None:
        return Response(404, "Repository not found.\n")
    repos = Repos(ctx.files)
    before = repos.refs(f"{owner}/{repo}")
    resp = git_http.read_whole(git_http.handle(req, ctx.files, owner, repo, "git-receive-pack", user))
    events.after_push(ctx, row, repos.refs_changed(f"{owner}/{repo}", before))
    return resp


def build() -> GithubRouter:
    router = GithubRouter()
    rest.register(router)
    router.add("GET", "/{owner}/{repo}/info/refs", _git("info/refs"))
    router.add("POST", "/{owner}/{repo}/git-upload-pack", _git("git-upload-pack"))
    router.add("POST", "/{owner}/{repo}/git-receive-pack", _git("git-receive-pack"))
    return router
