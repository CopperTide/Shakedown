"""Timed disruptions: a part learns of each at its `known_at` (an `alert`) and it takes effect at its
`at` (a `disruption` event once it is known), inside the part's own one-minute steps, so how the advances
are split never changes the state. The part reports its next reveal or effect as `wake_at_ms`."""
import json

import pytest

from shakedown_part import disruptions
from shakedown_part.jsonstate import Done, JsonStatePart, Tool
from shakedown_part.params import Param, Refusal

MIN = 60_000


def _set_cap(config, state, do):
    state["cap"] = do["cap"]
    return ["box/cap"]


KINDS = {"cap": disruptions.Kind({"cap": "integer"}, _set_cap)}


def _scenario(config, state, args):
    fired = disruptions.arm_timed(config, state, KINDS, args["disruptions"])
    return Done({"armed": len(args["disruptions"])}, writes=["scenario/current", *fired.writes])


class Box(JsonStatePart):
    NAME = "box"
    TOOLS = {t.name: t for t in [
        Tool("scenario", "Arm disruptions.", "write", (Param("disruptions", "array"),), _scenario)]}
    DISRUPTIONS = KINDS
    WORLD_TOOLS = frozenset({"scenario"})

    def describe_fields(self):
        return {"version": "0.1.0", "fingerprint": {"box": 1}, "uses_clock": True}

    def new_state(self, config):
        return {"cap": 9, "minutes": 0}

    def on_minute(self, inst, clock_ms):
        inst.state["minutes"] += 1
        return super().on_minute(inst, clock_ms)

    def docs(self, config, state):
        return {"box/cap": {"cap": state["cap"]}, **disruptions.docs(state)}


def storm(at, known):
    return {"id": "storm", "at": "-", "known_at": "-", "at_ms": at, "known_at_ms": known,
            "do": {"kind": "cap", "cap": 0}, "text": "A storm closes the box."}


def armed(*given):
    box = Box()
    box.start("i1", {}, 0, 0)
    out = box.call_tool("i1", "scenario", {"disruptions": list(given)}, 0)
    assert "error" not in out, out
    return box, out


def cap(box):
    return box.inspect({"instance": "i1"}, ["box/cap"])["docs"]["box/cap"]["cap"]


def test_forecast_reveals_before_effect():
    box, out = armed(storm(at=90 * MIN, known=60 * MIN))
    assert out["wake_at_ms"] == 60 * MIN, "the arming call says when the part next wakes"
    early = box.advance("i1", 59 * MIN)
    assert early["events"] == [] and early["wake_at_ms"] == 60 * MIN
    told = box.advance("i1", 60 * MIN)
    assert [e["kind"] for e in told["events"]] == ["alert"]
    assert told["events"][0]["data"]["known_only"] is True, "a forecast: it has not hit yet"
    assert told["events"][0]["data"]["at_ms"] == 90 * MIN
    assert cap(box) == 9 and told["wake_at_ms"] == 90 * MIN
    hit = box.advance("i1", 90 * MIN)
    assert [e["kind"] for e in hit["events"]] == ["disruption"]
    assert cap(box) == 0 and "wake_at_ms" not in hit


def test_late_discovery_reveals_after_effect():
    box, _ = armed(storm(at=30 * MIN, known=90 * MIN))
    hidden = box.advance("i1", 30 * MIN)
    assert hidden["events"] == [], "it takes effect unseen"
    assert cap(box) == 0 and hidden["changed"]
    assert hidden["wake_at_ms"] == 90 * MIN
    assert box.advance("i1", 89 * MIN)["events"] == []
    found = box.advance("i1", 90 * MIN)
    assert [e["kind"] for e in found["events"]] == ["alert"]
    assert found["events"][0]["data"]["known_only"] is False
    assert "wake_at_ms" not in found


def test_known_at_the_start_is_told_at_once():
    box, _ = armed(storm(at=30 * MIN, known=0))
    first = box.advance("i1", 0)
    assert [e["kind"] for e in first["events"]] == ["alert"]


def _saved(box, tmp_path, name):
    box.save("i1", name, str(tmp_path / name))
    return json.loads((tmp_path / name / "state.json").read_text())


def test_split_advances_same_state(tmp_path):
    whole, _ = armed(storm(at=40 * MIN, known=20 * MIN))
    one = whole.advance("i1", 90 * MIN)
    split, _ = armed(storm(at=40 * MIN, known=20 * MIN))
    parts = [split.advance("i1", t * MIN) for t in (30, 60, 90)]
    assert _saved(whole, tmp_path, "a") == _saved(split, tmp_path, "b")
    assert _saved(whole, tmp_path, "a")["state"]["minutes"] == 90, "one step per world minute"
    assert one["events"] == [e for p in parts for e in p["events"]]
    uneven, _ = armed(storm(at=40 * MIN, known=20 * MIN))
    for t in (61_000, 20 * MIN + 1, 45 * MIN + 30_000, 90 * MIN):
        uneven.advance("i1", t)
    assert _saved(uneven, tmp_path, "c") == _saved(whole, tmp_path, "a")


def test_a_save_keeps_what_is_pending(tmp_path):
    box, _ = armed(storm(at=40 * MIN, known=20 * MIN))
    box.advance("i1", 30 * MIN)
    box.save("i1", "s1", str(tmp_path / "saves" / "box" / "s1"))
    box.start("i2", {}, 0, 0, {"save": "s1", "mode": "exact"})
    hit = box.advance("i2", 40 * MIN)
    assert [e["kind"] for e in hit["events"]] == ["disruption"]


@pytest.mark.parametrize("bad, text", [
    ({"at_ms": None, "known_at_ms": None}, "at_ms"),
    ({"do": {"kind": "melt"}}, "do.kind"),
    ({"id": ""}, "id"),
])
def test_bad_timed_disruptions_are_refused(bad, text):
    box = Box()
    box.start("i1", {}, 0, 0)
    out = box.call_tool("i1", "scenario", {"disruptions": [{**storm(1, 1), **bad}]}, 0)
    assert text in out["error"], out


def test_next_wake_is_the_earliest_reveal_or_effect():
    state = {}
    disruptions.arm_timed({}, state, KINDS, [storm(at=50 * MIN, known=70 * MIN)])
    assert disruptions.next_wake(state) == 50 * MIN
    reveals, effects = disruptions.due(state, 50 * MIN)
    assert (len(reveals), len(effects)) == (0, 1)
    assert disruptions.next_wake(state) == 70 * MIN
    with pytest.raises(Refusal):
        disruptions.arm_timed({}, state, KINDS, [storm(1, 1)])
