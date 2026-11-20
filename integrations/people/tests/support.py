"""Neutral fixtures and a driver for the people part: a few people of each fixture role, the fixture rules, and
an engine that takes notices and runs the world clock the way shaked does."""
import copy
from pathlib import Path

import yaml

from shakedown_part.serve import dispatch
from people_part.part import People

FIXTURES = Path(__file__).resolve().parent / "fixtures"
MIN = 60_000
HOUR = 60 * MIN
DAY = 24 * HOUR
EPOCH = 1_767_571_200_000  # 2026-01-05T00:00:00Z: clock 0 is midnight UTC

PEOPLE = [
    {"id": "developer-001", "name": "Dev One", "login": "dev1", "role": "developer"},
    {"id": "developer-002", "name": "Dev Two", "login": "dev2", "role": "developer"},
    {"id": "reviewer-001", "name": "Rev One", "login": "rev1", "role": "reviewer"},
    {"id": "reviewer-002", "name": "Rev Two", "login": "rev2", "role": "reviewer"},
    {"id": "oncall-001", "name": "Oc One", "login": "oc1", "role": "oncall"},
    {"id": "pm-001", "name": "Pm One", "login": "pm1", "role": "pm"},
    {"id": "pm-002", "name": "Pm Two", "login": "pm2", "role": "pm"},
]
SYSTEMS = ["github", "jira", "slack", "pagerduty"]


def rules_doc(role: str) -> dict:
    return yaml.safe_load((FIXTURES / "rules" / f"{role}.yaml").read_text())


def config(seed=7, roles=("developer", "reviewer", "oncall", "pm"), people=None, systems=None) -> dict:
    return {"seed": seed, "people": copy.deepcopy(people or PEOPLE), "rules": {r: rules_doc(r) for r in roles},
            "systems": list(systems or SYSTEMS)}


class Engine:
    """One people instance, started through the protocol (so it gets the world's time), clock in world ms."""

    def __init__(self, cfg=None, instance="p", epoch=EPOCH, speed=1.0, part=None, save=None, clock_ms=0, **kw):
        self.part, self.instance, self.clock = part or People(), instance, clock_ms
        params = {"instance": instance, "config": cfg or config(**kw), "seed": 1, "clock_ms": clock_ms,
                  "world_time": {"epoch_ms": epoch, "speed": speed}}
        if save is not None:
            params["from"] = {"save": save, "mode": "exact"}
        dispatch(self.part, "start", params)

    def notice(self, system, kind, data, at_ms=None):
        self.clock = self.clock if at_ms is None else at_ms
        args = {"system": system, "kind": kind, "data": data}
        return dispatch(self.part, "call_tool", {"instance": self.instance, "name": "people_notice",
                                                       "args": args, "clock_ms": self.clock})

    def advance(self, clock_ms) -> list:
        self.clock = clock_ms
        return dispatch(self.part, "advance", {"instance": self.instance, "clock_ms": clock_ms})["events"]

    def run(self, until_ms, step_ms=MIN) -> list:
        """Every act event up to `until_ms`, advancing a step at a time."""
        out = []
        while self.clock < until_ms:
            out += self.advance(min(self.clock + step_ms, until_ms))
        return out

    def docs(self, *keys) -> dict:
        return self.part.inspect({"instance": self.instance}, list(keys))["docs"]

    def save(self, save_id, saves_root) -> None:
        self.part.save(self.instance, save_id, str(saves_root / "people" / save_id))


def acts(events) -> list:
    """(world ms, login, system, action, args) of each act event."""
    return [(e["data"]["at_ms"], e["data"]["person"], e["kind"].removesuffix("_act"), e["data"]["action"],
             e["data"]["args"]) for e in events]
