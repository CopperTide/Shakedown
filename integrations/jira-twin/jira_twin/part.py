"""The Jira twin part: an instance seeded from its config, the router, docs, acts and route env."""
from pathlib import Path

from shakedown_part.twin import HttpTwin

from . import act, docs, routes, seed, users

SCHEMA_SQL = (Path(__file__).parent / "schema.sql").read_text()


def route_env(config: dict, host: str) -> dict:
    """What agents get to find the twin (the env the jira client's examples and most agents read)."""
    return {"JIRA_URL": f"https://{host}", "JIRA_SERVER": f"https://{host}", "JIRA_API_TOKEN": config["token"],
            "JIRA_EMAIL": users.email_of(config, config["default_user"])}


class JiraTwin(HttpTwin):
    NAME = "jira-twin"
    SYSTEM = "jira"
    VERSION = "0.1.0"
    SCHEMA_SQL = SCHEMA_SQL
    READS = ["POST */rest/api/*/search/jql", "POST */rest/api/*/search", "POST */rest/api/*/search/approximate-count"]
    HONOURS_READONLY = True

    def router(self):
        return routes.build()

    def seed(self, ctx, config):
        seed.seed(ctx, config)

    def act(self, ctx, person, action, args):
        return act.run(ctx, person, action, args)

    def docs(self, ctx):
        return docs.build(ctx)

    def route_env(self, config, host):
        return {**route_env(config, host["host"]), **(host.get("env") or {})}

    def describe(self):
        return {**super().describe(), "system": {"kind": "tracker", "product": "Jira twin (Cloud or Server mode)",
                                                  "stands_in_for": "Jira", "simulated": True}}
