"""How every REST operation is served: the API version checked, a read-only agent's write refused,
the token checked, the caller made a user, and a `JiraError` rolled back to where the operation
started and answered in Jira's words."""
from shakedown_part.twin import Response

from .. import auth, users
from ..errors import JiraError, bad
from ..urls import AGILE, VERSIONS
from ..view import View

PLATFORM = "/rest/api/{version}"
READ_METHODS = ("GET", "HEAD", "OPTIONS")
NOT_AUTHENTICATED = "You are not authenticated. Authentication required to perform this operation."
READ_ONLY = "You do not have permission to perform this operation: this agent may only read."


class Call(View):
    """One REST request: what its handler works with."""

    def __init__(self, req, ctx, user):
        super().__init__(ctx, req.base, req.params.get("version", "2"))
        self.req, self.user = req, user

    @property
    def login(self) -> str:
        return self.user["name"] if self.user is not None else ""

    def body(self) -> dict:
        try:
            out = self.req.json()
        except ValueError:
            raise bad("Unexpected character in the request body: it is not JSON.") from None
        if not isinstance(out, dict):
            raise bad("The request body must be a JSON object.")
        return out

    def arg(self, name: str, default=None):
        return self.req.arg(name, default)

    def int_arg(self, name: str, default: int) -> int:
        value = self.req.arg(name)
        try:
            return int(value) if value not in (None, "") else default
        except ValueError:
            raise bad(f"The '{name}' parameter must be a number.") from None

    def page(self, name: str, default: int, cap: int) -> int:
        return max(0, min(self.int_arg(name, default), cap))


def error(e: JiraError) -> Response:
    return Response(e.status, e.body())


def _refused(req, read: bool) -> Response | None:
    if not req.readonly or req.method in READ_METHODS or read:
        return None
    return Response(403, JiraError(403, [READ_ONLY]).body(), effect="read")


def _caller(req, ctx, anonymous: bool):
    state = auth.check(req.headers, ctx.config.get("token", ""))
    if state != "ok":
        if anonymous:
            return None
        raise JiraError(401, [NOT_AUTHENTICATED])
    return users.ensure(ctx.store.conn, ctx.config, ctx.actor(req), ctx.now_ms())


def _run(fn, req, ctx, anonymous: bool) -> Response:
    db = ctx.store.conn
    db.execute("SAVEPOINT api")
    try:
        resp = fn(Call(req, ctx, _caller(req, ctx, anonymous)))
    except JiraError as e:
        db.execute("ROLLBACK TO api")
        resp = error(e)
    db.execute("RELEASE api")
    return resp if isinstance(resp, Response) else Response(200, resp)


def _serve(fn, req, ctx, read: bool, anonymous: bool) -> Response:
    if req.params.get("version", "2") not in VERSIONS:
        return not_found(req)
    refused = _refused(req, read)
    if refused is not None:
        return refused
    resp = _run(fn, req, ctx, anonymous)
    if read or req.method in READ_METHODS:
        resp.effect = "read"
    return resp


class Api:
    """Registers operations: `platform` under `/rest/api/{2,3,latest}`, `agile` under `/rest/agile/1.0`.
    A handler takes a `Call` and answers a `Response` or a JSON value (200). `read` marks a POST that
    only reads; `anonymous` an operation that needs no token."""

    def __init__(self, router):
        self.router = router

    def _add(self, prefix: str, method: str, path: str, read: bool, anonymous: bool):
        def register(fn):
            self.router.add(method, prefix + path, lambda req, ctx: _serve(fn, req, ctx, read, anonymous))
            return fn

        return register

    def platform(self, method: str, path: str, read: bool = False, anonymous: bool = False):
        return self._add(PLATFORM, method, path, read, anonymous)

    def agile(self, method: str, path: str, read: bool = False):
        return self._add(AGILE, method, path, read, False)


def not_found(req) -> Response:
    return Response(404, {"errorMessages": [f"null for uri: {req.base}{req.path}"], "errors": {}})


def no_content() -> Response:
    return Response(204)


def created(body) -> Response:
    return Response(201, body)


def offset_page(call: Call, items: list, key: str, default: int = 50, cap: int = 100) -> dict:
    """An offset page (`startAt`, `maxResults`, `total`, `isLast`) of `items` under `key`."""
    start, size = call.int_arg("startAt", 0), call.page("maxResults", default, cap)
    chunk = items[start:start + size]
    return {"maxResults": size, "startAt": start, "total": len(items), "isLast": start + len(chunk) >= len(items),
            key: chunk}
