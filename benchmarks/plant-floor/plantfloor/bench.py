"""The plant-floor benchmark: an ERP (orders, suppliers, purchasing with a spend cap) and Plant 2's floor over
OPC UA; one task (a supplier fire, a rush order and a robot cell down land on Monday morning); three agent
designs; scored by named checks; drawn as a twin."""
from pathlib import Path

from arena_part import benchmark

from . import checks, setups, twin

VERSION = "0.2.0"
DATA = Path(__file__).resolve().parents[1] / "data" / "plant-floor.json"


def load(path: Path = DATA) -> dict:
    return benchmark.load(path)


class PlantFloor(benchmark.SingleTaskBenchmark):
    NAME, VERSION, TASK, DATA = "plant-floor", VERSION, setups.TASK, DATA
    INSTRUCTIONS, INSTRUCTIONS_TITLE = setups.INSTRUCTIONS, setups.INSTRUCTIONS_TITLE
    TEAM, TEAMS = setups.SINGLE_AGENT, [setups.TEAM, setups.TEAM_RULES]

    def world(self):
        return [{"part": "erp", "integration": "factory-01-erp", "config": self.config["erp"]},
                {"part": "plant", "integration": "plant-opcua", "config": self.config["plant"]}]

    def task(self):
        return {"id": setups.TASK, "reason": "A supplier fire, a key-account rush order and a faulted robot cell land "
                "on the plant floor on Monday morning; re-plan the week over OPC UA.", "ticket": setups.TICKET,
                "setup": [{"part": "erp", "tool": "erp_scenario", "args": {"name": "apex-fire"}},
                          {"part": "plant", "tool": "plant_scenario", "args": {"name": "disrupted"}}]}

    def evaluate(self, docs):
        return checks.evaluate(docs)

    def twin(self, docs):
        return twin.view(self.config, docs)
