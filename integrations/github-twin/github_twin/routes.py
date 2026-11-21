"""The twin's router: git smart HTTP beside the REST API, and GitHub's 404 for API paths it lacks."""
from shakedown_part.twin import Response, Router

from . import auth, events, git_http, rest, users
from .actions import logs
from .graphql import endpoint as graphql
from .repos import Repos
from .rest.api import not_found_response
from .urls import API


class GithubRouter(Router):
    def not_found(self, req) -> Response:
        if req.path.startswith(API):
            return not_found_response()
        return Response(404, "Not Found\n")


def _git(tail: str):
    def handle(req, ctx):
        if auth.check(req.headers, ctx.config.get("token", "")) != "ok":
            return git_http.challenge()
        if req.readonly and tail == "git-receive-pack":
            return Response(403, "Read-only agents may not push.\n", effect="read")
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
    repos = Repos(ctx.files, ctx.on_rollback)
    before = repos.refs(f"{owner}/{repo}")
    repos.on_rollback_refs(f"{owner}/{repo}", before)
    resp = git_http.read_whole(git_http.handle(req, ctx.files, owner, repo, "git-receive-pack", user))
    events.after_push(ctx, row, repos.refs_changed(f"{owner}/{repo}", before), by=user)
    return resp


def build() -> GithubRouter:
    router = GithubRouter()
    rest.register(router)
    router.add("POST", "/api/graphql", graphql.handle)
    router.add("GET", "/_logs/jobs/{id}", logs.serve_job)
    router.add("GET", "/_logs/runs/{id}", logs.serve_run)
    router.add("GET", "/{owner}/{repo}/info/refs", _git("info/refs"))
    router.add("POST", "/{owner}/{repo}/git-upload-pack", _git("git-upload-pack"))
    router.add("POST", "/{owner}/{repo}/git-receive-pack", _git("git-receive-pack"))
    return router
