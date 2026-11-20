"""The clock helpers: a fixed-step clock and a JSON-state part on the world clock."""
import pytest

from shakedown_part import FixedStep
from shakedown_part.jsonstate import ClockedJsonStatePart, Done, Tool, event
from shakedown_part.params import Param


def test_a_fixed_step_clock_starts_at_the_last_whole_step():
    assert FixedStep(500, 1_250).done_ms == 1_000
    assert FixedStep(500).done_ms == 0
    steps = []
    f = FixedStep(500, 1_250)
    assert f.advance_to(2_400, steps.append) == 2 and f.done_ms == 2_000


def _bump(config, state, args):
    state["level"] += args["by"]
    return Done({"level": state["level"]}, writes=["tank/level"])


class Tank(ClockedJsonStatePart):
    """Drains one unit a second; says so each time it runs dry."""
    NAME = "tank"
    TOOLS = {"bump": Tool("bump", "Fill.", "write", (Param("by", "integer"),), _bump)}

    def describe_fields(self):
        return {"version": "0.1.0", "fingerprint": {"tank": 1}}

    def new_state(self, config):
        return {"level": config["level"]}

    def step(self, config, state, clock_ms):
        out = []
        while state["clock_ms"] + 1000 <= clock_ms:
            state["clock_ms"] += 1000
            if state["level"] > 0:
                state["level"] -= 1
                if state["level"] == 0:
                    out.append(event("dry", f"dry at {state['clock_ms']}", {}))
        return out

    def docs(self, config, state):
        return {"tank/level": {"level": state["level"], "at": state["clock_ms"]}}


def test_a_clocked_part_runs_to_each_calls_clock_and_queues_events_until_advance():
    tank = Tank()
    assert tank.describe()["uses_clock"] is True and list(tank.describe())[3] == "uses_clock"
    tank.start("i1", {"level": 2}, 0, 5_000)
    assert tank.inspect({"instance": "i1"}, [])["docs"] == {"tank/level": {"level": 2, "at": 5_000}}
    assert "events" not in tank.call_tool("i1", "bump", {"by": 0}, 7_000)
    assert tank.advance("i1", 7_500) == {"changed": True, "events": [
        {"kind": "dry", "text": "dry at 7000", "data": {}}]}
    assert tank.advance("i1", 7_900) == {"changed": False, "events": []}
    assert tank.advance("i1", 8_000) == {"changed": True, "events": []}
    tank.call_tool("i1", "bump", {"by": 3}, 1_000)  # a clock behind the world's changes nothing
    assert tank.inspect({"instance": "i1"}, [])["docs"]["tank/level"] == {"level": 3, "at": 8_000}


def test_a_restored_clocked_part_keeps_its_clock_and_its_queued_events(tmp_path):
    tank = Tank()
    tank.start("i1", {"level": 1}, 0, 0)
    tank.call_tool("i1", "bump", {"by": 0}, 1_000)
    tank.save("i1", "s1", str(tmp_path / "saves" / "tank" / "s1"))
    again = Tank()
    again.start("i2", {"level": 1}, 0, 60_000, {"save": "s1"})
    assert again.advance("i2", 1_000)["events"][0]["kind"] == "dry"
    with pytest.raises(NotImplementedError):
        ClockedJsonStatePart().step({}, {}, 0)
