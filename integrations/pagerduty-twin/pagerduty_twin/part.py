"""The PagerDuty twin part: an account seeded from its config, the REST and Events routers, docs, the
`pagerduty_act` and `pagerduty_event` world tools, and route env."""
from pathlib import Path

from arena_part.twin import HttpTwin

from . import act, docs, events_v2, routes, seed

SCHEMA_SQL = (Path(__file__).parent / "schema.sql").read_text()


def route_env(config: dict, host: str) -> dict:
    """What agents get to find the twin: the REST host for the first configured host, the Events
    host for the second (the alert link calls the world tool instead)."""
    hosts = [h["host"] for h in config.get("hosts", [])]
    events = hosts[1] if len(hosts) > 1 else host
    return {"PAGERDUTY_API_URL": f"https://{hosts[0] if hosts else host}",
            "PAGERDUTY_EVENTS_URL": f"https://{events}/v2/enqueue", "PAGERDUTY_TOKEN": config["token"]}


class PagerDutyTwin(HttpTwin):
    NAME = "pagerduty-twin"
    SYSTEM = "pagerduty"
    VERSION = "0.1.0"
    SCHEMA_SQL = SCHEMA_SQL
    HONOURS_READONLY = True

    def router(self):
        return routes.build()

    def seed(self, ctx, config):
        seed.seed(ctx, config)

    def act(self, ctx, person, action, args):
        return act.run(ctx, person, action, args)

    def world_tools(self):
        return {"pagerduty_event": (events_v2.TOOL, events_v2.tool)}

    def docs(self, ctx):
        return docs.build(ctx)

    def route_env(self, config, host):
        return {**route_env(config, host["host"]), **(host.get("env") or {})}

    def describe(self):
        return {**super().describe(), "system": {"kind": "paging", "product": "PagerDuty twin (REST v2, Events v2)",
                                                  "stands_in_for": "PagerDuty", "simulated": True}}
