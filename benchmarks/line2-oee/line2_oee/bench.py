"""The Line 2 OEE benchmark: one simulation part in its world, one task (reach a target OEE within a
simulation budget and the plant's limits), three suggested optimiser designs, scored by named checks
on the re-simulated recommendation, drawn as a twin."""
from pathlib import Path

from arena_part import benchmark

from . import checks, setups, twin

VERSION = "0.1.0"
DATA = Path(__file__).resolve().parents[1] / "data" / "plant.json"
SIM = "line-sim"


def load(path: Path = DATA) -> dict:
    return benchmark.load(path)


class Line2Oee(benchmark.SingleTaskBenchmark):
    NAME, VERSION, TASK, DATA = "line2-oee", VERSION, setups.TASK, DATA
    INSTRUCTIONS, INSTRUCTIONS_TITLE = setups.INSTRUCTIONS, setups.INSTRUCTIONS_TITLE
    TEAM, TEAMS = setups.SINGLE, [setups.TEAM, setups.STRATEGIST]

    def world(self):
        return [{"part": "sim", "integration": SIM, "config": self.config}]

    def task(self):
        return {"id": setups.TASK, "reason": "Line 2 from 77% to 85% OEE, no extra headcount",
                "ticket": setups.TICKET}

    def evaluate(self, docs):
        return checks.evaluate(docs)

    def twin(self, docs):
        return twin.view(self.config, docs)
