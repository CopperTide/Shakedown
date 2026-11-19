"""The arena-part/1 integration: a config (the site, the tariff, the load at the start) and a state (the meter,
the demand-response events and the schedule) per instance, saved as one JSON file. On the arena clock: the meter
integrates the load as the clock runs, and a window's start and end are world events."""
from arena_part import disruptions
from arena_part.jsonstate import ClockedJsonStatePart

from . import disrupt, meter
from .tools import BASE, TOOLS

VERSION = "0.1.0"


class Utility(ClockedJsonStatePart):
    NAME = "utility"
    TOOLS = TOOLS
    DISRUPTIONS = disrupt.KINDS
    WORLD_TOOLS = frozenset({"meter_load", "utility_scenario"})

    def describe_fields(self):
        return {"version": VERSION, "fingerprint": {"adapter": VERSION},
                "system": {"kind": "utility", "product": "Simulated electric utility and site meter",
                           "stands_in_for": "a utility's demand-response program and a revenue meter",
                           "simulated": True}}

    def new_state(self, config):
        meter.check_config(config)
        return {"scenario": BASE, "clock_ms": 0, "meter": meter.new_meter(config), "events": {}, "queued": []}

    def step(self, config, state, clock_ms) -> list:
        """The meter up to `clock_ms`, and the window events on the way."""
        return meter.run_to(config, state, clock_ms)

    def docs(self, config, state):
        out = {"scenario/current": {"name": state["scenario"]}, "meter/site": meter.meter_doc(config, state)}
        out.update({f"dr/{k}": meter.event_doc(config, ev) for k, ev in state["events"].items()})
        out.update(disruptions.docs(state))
        return out
