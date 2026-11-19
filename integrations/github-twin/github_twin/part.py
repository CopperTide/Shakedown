"""The GitHub twin part: repositories seeded from the config, the router, docs and the save rules."""
from pathlib import Path

from arena_part.twin import HttpTwin

from . import docs, routes, users
from .repos import Repos, is_immutable

SCHEMA_SQL = (Path(__file__).parent / "schema.sql").read_text()


def route_env(config: dict, host: str) -> dict:
    """What drivers and agents get to find the twin (gh in GHES mode, git's credential helper)."""
    return {"GH_HOST": host, "GH_ENTERPRISE_TOKEN": config["token"], "GITHUB_API_URL": f"https://{host}/api/v3",
            "GITHUB_SERVER_URL": f"https://{host}", "GH_NO_UPDATE_NOTIFIER": "1",
            "GH_NO_EXTENSION_UPDATE_NOTIFIER": "1", "GH_TELEMETRY": "0", "GIT_TERMINAL_PROMPT": "0",
            "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": f"credential.https://{host}.helper",
            "GIT_CONFIG_VALUE_0": "!f() { echo username=x-access-token; echo password=$GH_ENTERPRISE_TOKEN; }; f"}


def _seed_repo(ctx, spec: dict) -> None:
    owner, now = ctx.config["org"], ctx.now_ms()
    author = users.person(ctx.config, ctx.config["default_user"], now)
    Repos(ctx.files).init_from(Path(spec["source"]), f"{owner}/{spec['name']}", spec.get("default_branch", "main"), author)
    ctx.store.conn.execute(
        "INSERT INTO repos(owner, name, description, private, default_branch, created_ms, updated_ms, pushed_ms) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        (owner, spec["name"], spec.get("description"), int(spec.get("private", True)),
         spec.get("default_branch", "main"), now, now, now))


class GithubTwin(HttpTwin):
    NAME = "github-twin"
    SYSTEM = "github"
    VERSION = "0.1.0"
    SCHEMA_SQL = SCHEMA_SQL
    READS = ["POST */git-upload-pack"]

    def router(self):
        return routes.build()

    def seed(self, ctx, config):
        db, now = ctx.store.conn, ctx.now_ms()
        users.ensure(db, config["org"], now, kind="Organization")
        users.ensure(db, config["default_user"], now)
        for spec in config.get("repos", []):
            _seed_repo(ctx, spec)

    def docs(self, ctx):
        return docs.build(ctx)

    def is_immutable(self, path):
        return is_immutable(path)

    def route_env(self, config, host):
        return {**route_env(config, host["host"]), **(host.get("env") or {})}

    def describe(self):
        return {**super().describe(), "system": {"kind": "code", "product": "GitHub twin (GHES mode)",
                                                  "stands_in_for": "GitHub Enterprise Server", "simulated": True}}
