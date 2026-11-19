"""One Web API call: its arguments (query, form or JSON body, as Slack takes them all), the token
(`Authorization: Bearer`, or a `token` argument), and the caller made a user on first sight."""
import hmac
import json
from urllib.parse import parse_qs

from . import records
from .errors import SlackError


def _body_args(req) -> dict:
    ctype = req.headers.get("content-type", "").lower()
    if not req.body.strip():
        return {}
    if "json" in ctype:
        try:
            out = json.loads(req.body)
        except ValueError:
            raise SlackError("invalid_json") from None
        if not isinstance(out, dict):
            raise SlackError("json_not_object")
        return out
    return {k: v[0] for k, v in parse_qs(req.body.decode(), keep_blank_values=True).items()}


class Call:
    def __init__(self, req, ctx):
        self.req, self.ctx = req, ctx
        self.args = {**{k: v[0] for k, v in req.query.items()}, **_body_args(req)}
        self.user = None

    @property
    def db(self):
        return self.ctx.store.conn

    @property
    def config(self) -> dict:
        return self.ctx.config

    def authenticate(self) -> None:
        bearer = self.req.headers.get("authorization", "")
        token = bearer[7:] if bearer.lower().startswith("bearer ") else self.args.get("token")
        if not token:
            raise SlackError("not_authed")
        if not hmac.compare_digest(str(token).encode(), self.config.get("token", "").encode()):
            raise SlackError("invalid_auth")
        self.user = records.ensure_user(self.db, self.config, self.ctx.actor(self.req), self.ctx.now_ms())

    @property
    def login(self) -> str:
        return self.user["name"]

    def arg(self, name: str, default=None):
        value = self.args.get(name, default)
        return default if value in (None, "") else value

    def int_arg(self, name: str, default: int, cap: int) -> int:
        try:
            return max(1, min(int(self.arg(name, default)), cap))
        except (TypeError, ValueError):
            raise SlackError("invalid_arg_name") from None

    def need(self, name: str, code: str):
        value = self.arg(name)
        if value is None:
            raise SlackError(code)
        return value
