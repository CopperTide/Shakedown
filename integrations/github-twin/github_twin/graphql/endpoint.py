"""`POST /api/graphql`: the token checked, the caller made a user, the request run, and the answer
given GitHub's headers and `X-Arena-Effect`."""
from shakedown_part.twin import Response

from .. import auth, users
from ..headers import api_headers
from . import executor
from .context import Gql

DOCS = "https://docs.github.com/enterprise-server@3.20/graphql"
UNAUTHENTICATED = {"missing": "This endpoint requires you to be authenticated.", "bad": "Bad credentials"}


def _headers(ctx) -> dict:
    return {**api_headers(ctx.config.get("installed_version", "3.20.0"), ctx.now_ms()),
            "X-RateLimit-Resource": "graphql"}


def _body(req) -> dict | None:
    try:
        body = req.json()
    except ValueError:
        return None
    return body if isinstance(body, dict) else None


def handle(req, ctx) -> Response:
    state = auth.check(req.headers, ctx.config.get("token", ""))
    if state != "ok":
        return Response(401, {"message": UNAUTHENTICATED[state], "documentation_url": DOCS}, headers=_headers(ctx))
    body = _body(req)
    if body is None:
        return Response(400, {"message": "Problems parsing JSON", "documentation_url": DOCS}, headers=_headers(ctx))
    user = users.ensure(ctx.store.conn, ctx.actor(req), ctx.now_ms())
    gql = Gql(ctx, req.base, user, readonly=req.readonly)
    result, effect = executor.run(body.get("query"), body.get("variables"), body.get("operationName"), gql)
    return Response(200, result, headers=_headers(ctx), effect=effect)
