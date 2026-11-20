"""How every REST operation is served: the token checked, the caller made a user, the answer given
GitHub's headers, and an `ApiError` rolled back to where the operation started."""
from shakedown_part.twin import Response

from .. import auth, users
from ..errors import GithubError, invalid, not_found, read_only
from ..headers import api_headers, etag
from ..pagination import page
from ..urls import API
from ..view import View

DOCS = "https://docs.github.com/enterprise-server@3.20/rest"
READ_METHODS = ("GET", "HEAD", "OPTIONS")


class ApiError(GithubError):
    """A REST refusal: GitHub's message and the documentation link."""


def _error_response(e: GithubError) -> Response:
    body = {"message": e.message, "documentation_url": DOCS}
    if e.errors is not None:
        body["errors"] = e.errors
    return Response(e.status, body, headers=dict(e.headers or {}))


class Call(View):
    """One REST request: what its handler works with."""

    def __init__(self, req, ctx, user):
        super().__init__(ctx, req.base)
        self.req, self.user = req, user

    @property
    def login(self) -> str:
        return self.user["login"]

    def author(self, given: dict | None = None):
        """The caller as a commit's author (or the `{name, email}` the request gives)."""
        return super().person(self.login, given)

    def body(self) -> dict:
        try:
            out = self.req.json()
        except ValueError:
            raise ApiError(400, "Problems parsing JSON") from None
        if not isinstance(out, dict):
            raise ApiError(400, "Problems parsing JSON")
        return out

    def repo(self):
        """The repository the path names; 404 when there is none."""
        row = self.db.execute("SELECT * FROM repos WHERE owner = ? AND name = ?",
                              (self.req.params["owner"], self.req.params["repo"])).fetchone()
        if row is None:
            raise not_found()
        return row


def _caller(req, ctx):
    state = auth.check(req.headers, ctx.config.get("token", ""))
    if state != "ok":
        raise ApiError(401, "Requires authentication" if state == "missing" else "Bad credentials")
    return users.ensure(ctx.store.conn, ctx.actor(req), ctx.now_ms())


def _finish(resp, ctx) -> Response:
    if not isinstance(resp, Response):
        resp = Response(200, resp)
    body, _ = resp.encoded()
    resp.headers = {**api_headers(ctx.config.get("installed_version", "3.20.0"), ctx.now_ms()),
                    "ETag": etag(body), **(resp.headers or {})}
    return resp


def refused_write(req) -> Response | None:
    """A read-only agent's write, refused and marked a read (owner ruling, 2026-11-19); else None."""
    if not req.readonly or req.method in READ_METHODS:
        return None
    resp = _error_response(read_only())
    resp.effect = "read"
    return resp


def _serve(fn, req, ctx) -> Response:
    refused = refused_write(req)
    if refused is not None:
        return _finish(refused, ctx)
    try:
        with ctx.savepoint("api"):
            resp = fn(Call(req, ctx, _caller(req, ctx)))
    except GithubError as e:
        resp = _error_response(e)
    return _finish(resp, ctx)


class Api:
    """Registers REST operations on a router under `/api/v3`; a handler takes a `Call` and answers a
    `Response` or a JSON value (200)."""

    def __init__(self, router):
        self.router = router

    def route(self, method: str, path: str):
        def register(fn):
            self.router.add(method, API + path, lambda req, ctx: _serve(fn, req, ctx))
            return fn

        return register


def not_found_response() -> Response:
    resp = _error_response(not_found())
    resp.headers = {**api_headers(), **(resp.headers or {})}
    return resp


def no_content() -> Response:
    return Response(204)


def created(body) -> Response:
    return Response(201, body)


def paged(call: Call, items: list) -> Response:
    """The page of `items` the request asks for, with its `Link` header."""
    body, link = page(items, call.req, call.urls)
    return Response(200, body, headers={"Link": link} if link else None)
