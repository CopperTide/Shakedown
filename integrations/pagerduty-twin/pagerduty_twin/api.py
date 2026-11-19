"""How every REST operation is served: a read-only agent's write refused, the token checked
(`Authorization: Token token=…` or `Bearer`), writes made to name their user in `From`, and a
`PagerDutyError` rolled back and answered in PagerDuty's words."""
import hmac

from arena_part.twin import Response

from . import records
from .errors import ACCESS_DENIED, NO_FROM, UNAUTHORIZED, PagerDutyError, invalid
from .shapes import Shapes

READ_METHODS = ("GET", "HEAD", "OPTIONS")


def token_of(headers) -> str | None:
    value = headers.get("authorization", "")
    for prefix in ("token token=", "bearer "):
        if value.lower().startswith(prefix):
            return value[len(prefix):].strip()
    return None


class Call:
    def __init__(self, req, ctx):
        self.req, self.ctx = req, ctx
        self.shapes = Shapes(ctx.store.conn, req.base)

    @property
    def db(self):
        return self.ctx.store.conn

    def me(self):
        """The agent the proxy names, else the default user."""
        return records.ensure_user(self.db, self.ctx.config, self.ctx.actor(self.req), self.ctx.now_ms())

    def sender(self) -> str:
        """Who a write acts as: PagerDuty requires a user's email in `From`; the agent the proxy names
        acts as itself (one identity per agent), else the `From` user."""
        given = self.req.headers.get("from")
        if not given:
            raise NO_FROM
        user = records.user_by_email(self.db, given)
        if user is None:
            raise invalid("The From header is not a valid user email address.")
        return self.me()["name"] if self.req.agent else user["name"]

    def body(self) -> dict:
        try:
            out = self.req.json()
        except ValueError:
            raise invalid("The request body is not JSON.") from None
        return out if isinstance(out, dict) else {}

    def list_arg(self, name: str) -> list[str]:
        return self.req.query.get(f"{name}[]", []) + self.req.query.get(name, [])


def _error(e: PagerDutyError) -> Response:
    return Response(e.status, e.body())


def serve(fn, req, ctx) -> Response:
    if req.readonly and req.method not in READ_METHODS:
        resp = _error(ACCESS_DENIED)
        resp.effect = "read"
        return resp
    token = token_of(req.headers)
    if not token or not hmac.compare_digest(token.encode(), ctx.config.get("token", "").encode()):
        return _error(UNAUTHORIZED)
    db = ctx.store.conn
    db.execute("SAVEPOINT api")
    try:
        resp = fn(Call(req, ctx))
    except PagerDutyError as e:
        db.execute("ROLLBACK TO api")
        resp = _error(e)
    db.execute("RELEASE api")
    resp = resp if isinstance(resp, Response) else Response(200, resp)
    if req.method in READ_METHODS:
        resp.effect = "read"
    return resp


def page(call: Call, key: str, items: list) -> dict:
    """Classic pagination: `limit` (default 25, at most 100), `offset`, `more`, and `total` when asked."""
    try:
        limit = max(1, min(int(call.req.arg("limit", "25")), 100))
        offset = max(0, int(call.req.arg("offset", "0")))
    except ValueError:
        raise invalid("limit and offset must be numbers.") from None
    want_total = str(call.req.arg("total", "false")).lower() == "true"
    return {key: items[offset:offset + limit], "limit": limit, "offset": offset, "more": offset + limit < len(items),
            "total": len(items) if want_total else None}

