"""What the rules, shapes and payloads work with: the instance, its store and repos, and the public
base its URLs are built from (a request's, else the first configured host's)."""
from functools import cached_property

from . import users
from .gitcmd import Person
from .repos import Repos
from .urls import Urls


def config_base(config: dict) -> str:
    hosts = config.get("hosts") or [{"host": "localhost"}]
    return f"https://{hosts[0]['host']}"


class View:
    def __init__(self, ctx, base: str | None = None):
        self.ctx = ctx
        self.base = base or config_base(ctx.config)

    @property
    def db(self):
        return self.ctx.store.conn

    @property
    def config(self) -> dict:
        return self.ctx.config

    @cached_property
    def urls(self) -> Urls:
        return Urls(self.base)

    @cached_property
    def repos(self) -> Repos:
        return Repos(self.ctx.files, self.ctx.on_rollback)

    def now(self) -> int:
        return self.ctx.now_ms()

    def person(self, login: str, given: dict | None = None) -> Person:
        """`login` as a commit's author (or the `{name, email}` a request gives), dated now."""
        if given and given.get("name") and given.get("email"):
            return Person(given["name"], given["email"], self.now())
        return users.person(self.config, login, self.now())
