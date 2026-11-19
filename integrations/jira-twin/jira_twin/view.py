"""What the rules, shapes and acts work with: the instance, its store, its workflow and metadata,
whether it is Cloud, and the public base and API version its URLs are built from."""
from functools import cached_property

from .meta import Meta
from .urls import Urls
from .workflow import Workflow


def config_base(config: dict) -> str:
    hosts = config.get("hosts") or [{"host": "localhost"}]
    return f"https://{hosts[0]['host']}"


class View:
    def __init__(self, ctx, base: str | None = None, version: str = "2"):
        self.ctx = ctx
        self.base = base or config_base(ctx.config)
        self.version = version

    @property
    def db(self):
        return self.ctx.store.conn

    @property
    def config(self) -> dict:
        return self.ctx.config

    @property
    def cloud(self) -> bool:
        return self.config.get("deployment", "cloud") == "cloud"

    @property
    def adf(self) -> bool:
        """Rich text is ADF (API v3) rather than text."""
        return self.version == "3"

    @cached_property
    def urls(self) -> Urls:
        return Urls(self.base, self.version)

    @cached_property
    def meta(self) -> Meta:
        return Meta(self.config)

    @cached_property
    def workflow(self) -> Workflow:
        return Workflow(self.config.get("workflow"))

    def now(self) -> int:
        return self.ctx.now_ms()

    def notify(self, kind: str, data: dict) -> None:
        self.ctx.notify(kind, f"jira {kind} {data.get('key', '')}".strip(), data)
