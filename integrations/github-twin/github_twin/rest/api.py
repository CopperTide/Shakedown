"""How every REST operation is served: the token checked, the caller made a user, the answer given
GitHub's headers, and an `ApiError` rolled back to where the operation started."""
from dataclasses import dataclass
from functools import cached_property

from arena_part.twin import Response

from .. import auth, users
from ..gitcmd import Person
from ..headers import api_headers, etag
from ..pagination import page
from ..repos import Repos
from ..urls import API, Urls

DOCS = "https://docs.github.com/enterprise-server@3.20/rest"


class ApiError(Exception):
    def __init__(self, status: int, message: str, errors: list | None = None, headers: dict | None = None):
        super().__init__(message)
        self.status, self.message, self.errors, self.headers = status, message, errors, headers

    def response(self) -> Response:
        body = {"message": self.message, "documentation_url": DOCS}
        if self.errors is not None:
            body["errors"] = self.errors
        return Response(self.status, body, headers=dict(self.headers or {}))


def not_found() -> ApiError:
    return ApiError(404, "Not Found")


def invalid(message: str, errors: list | None = None) -> ApiError:
    return ApiError(422, message, errors)


@dataclass
class Call:
    """One REST request: what its handler works with."""
    req: object
    ctx: object
    user: object

    @property
    def db(self):
        return self.ctx.store.conn

    @cached_property
    def urls(self) -> Urls:
        return Urls(self.req.base)

    @cached_property
    def repos(self) -> Repos:
        return Repos(self.ctx.files)

    @property
    def login(self) -> str:
        return self.user["login"]

    def now(self) -> int:
        return self.ctx.now_ms()

    def person(self, given: dict | None = None) -> Person:
        """The caller as a commit's author (or the `{name, email}` a request gives), dated now."""
        if given and given.get("name") and given.get("email"):
            return Person(given["name"], given["email"], self.now())
        return users.person(self.ctx.config, self.login, self.now())

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


def _serve(fn, req, ctx) -> Response:
    db = ctx.store.conn
    db.execute("SAVEPOINT api")
    try:
        resp = fn(Call(req, ctx, _caller(req, ctx)))
    except ApiError as e:
        db.execute("ROLLBACK TO api")
        resp = e.response()
    db.execute("RELEASE api")
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


def api_not_found(ctx) -> Response:
    return _finish(not_found().response(), ctx)


def no_content() -> Response:
    return Response(204)


def created(body) -> Response:
    return Response(201, body)


def paged(call: Call, items: list) -> Response:
    """The page of `items` the request asks for, with its `Link` header."""
    body, link = page(items, call.req, call.urls)
    return Response(200, body, headers={"Link": link} if link else None)
