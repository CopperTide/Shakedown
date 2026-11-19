"""The twin's router: `/api/<method>` (GET or POST) to the method table. Each call runs in a savepoint
rolled back when it refuses; a refused write changes nothing and is marked a read, and a read-only
agent's write gets `missing_scope`."""
from arena_part.twin import Response, Router

from .call import Call
from .errors import SlackError
from .methods import METHODS, READ_SCOPES


def _answer(body: dict, effect: str | None) -> Response:
    return Response(200, body, effect=effect)


def _run(method, call) -> dict:
    db = call.db
    db.execute("SAVEPOINT api")
    try:
        call.authenticate()
        body = {"ok": True, **method.fn(call)}
    except SlackError as e:
        db.execute("ROLLBACK TO api")
        body = e.body()
    db.execute("RELEASE api")
    return body


def handle(req, ctx) -> Response:
    method = METHODS.get(req.params["method"])
    if method is None:
        return _answer({"ok": False, "error": "unknown_method"}, "read")
    try:
        call = Call(req, ctx)
    except SlackError as e:
        return _answer(e.body(), "read")
    if method.write and req.readonly:
        return _answer(SlackError("missing_scope", needed=method.scope, provided=READ_SCOPES).body(), "read")
    body = _run(method, call)
    return _answer(body, "write" if method.write and body["ok"] else "read")


def build() -> Router:
    router = Router()
    for verb in ("GET", "POST"):
        router.add(verb, "/api/{method}", handle)
    return router
