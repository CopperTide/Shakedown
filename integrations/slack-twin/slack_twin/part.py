"""The Slack twin part: a workspace seeded from its config, the Web API router, docs, acts and route env."""
from pathlib import Path

from arena_part.twin import HttpTwin

from . import act, apps, docs, routes, seed
from .methods import METHODS

SCHEMA_SQL = (Path(__file__).parent / "schema.sql").read_text()


def route_env(config: dict, host: str) -> dict:
    """What agents get to find the twin (slack_sdk reads a base URL; most agents read these)."""
    return {"SLACK_API_URL": f"https://{host}/api/", "SLACK_BOT_TOKEN": config["token"]}


class SlackTwin(HttpTwin):
    NAME = "slack-twin"
    SYSTEM = "slack"
    VERSION = "0.1.0"
    SCHEMA_SQL = SCHEMA_SQL
    READS = [f"POST */api/{name}" for name, m in METHODS.items() if not m.write]
    HONOURS_READONLY = True

    def router(self):
        return routes.build()

    def seed(self, ctx, config):
        seed.seed(ctx, config)

    def act(self, ctx, person, action, args):
        return act.run(ctx, person, action, args)

    def docs(self, ctx):
        return docs.build(ctx)

    def world_tools(self):
        return {apps.TOOL["name"]: (apps.TOOL, apps.run)}

    def route_env(self, config, host):
        return {**route_env(config, host["host"]), **(host.get("env") or {})}

    def describe(self):
        return {**super().describe(), "system": {"kind": "chat", "product": "Slack twin (Web API)",
                                                  "stands_in_for": "Slack", "simulated": True}}
