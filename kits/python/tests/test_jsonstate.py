"""A JSON-state part: refusals change nothing, saves restore exactly, inspect and diff read the docs."""
import pytest

from arena_part import PartError
from arena_part.jsonstate import (DENIALS, Denied, Done, JsonStatePart, Scenarios, Tool, denial, denials_doc,
                                  event)
from arena_part.params import Param, Refusal


def _add(config, state, args):
    state["n"] += args["n"]
    if state["n"] > config["max"]:
        raise Refusal(f"over {config['max']}")
    return Done({"n": state["n"]}, writes=["box/n"], events=[event("added", "added", {"n": state["n"]})])


class Box(JsonStatePart):
    NAME = "box"
    TOOLS = {"add": Tool("add", "Add n.", "write", (Param("n", "integer"),), _add)}

    def describe_fields(self):
        return {"version": "0.1.0", "fingerprint": {"box": 1}}

    def new_state(self, config):
        return {"n": 0}

    def docs(self, config, state):
        return {"box/n": {"n": state["n"], "max": config["max"]}}


def test_a_refusal_changes_nothing_and_a_call_gives_its_events():
    box = Box()
    box.start("i1", {"max": 5}, 0, 0)
    out = box.call_tool("i1", "add", {"n": 4}, 0)
    assert out == {"result": {"n": 4}, "reads": [], "writes": ["box/n"],
                   "events": [{"kind": "added", "text": "added", "data": {"n": 4}}]}
    assert box.call_tool("i1", "add", {"n": 4}, 0) == {"error": "over 5", "reads": [], "writes": []}
    assert box.call_tool("i1", "add", {}, 0)["error"] == "add: missing n"
    assert box.inspect({"instance": "i1"}, [])["docs"] == {"box/n": {"n": 4, "max": 5}}
    with pytest.raises(PartError) as e:
        box.call_tool("i1", "nope", {}, 0)
    assert e.value.code == -32602


def test_a_save_restores_exactly_and_diffs(tmp_path):
    box = Box()
    box.start("i1", {"max": 9}, 0, 0)
    box.call_tool("i1", "add", {"n": 2}, 0)
    assert box.save("i1", "s1", str(tmp_path / "saves" / "box" / "s1"))["exact"] is True
    box.call_tool("i1", "add", {"n": 1}, 0)
    box.start("i2", {"max": 9}, 0, 0, {"save": "s1", "mode": "exact"})
    assert box.inspect({"save": "s1"}, ["box/n"])["docs"]["box/n"]["n"] == 2
    assert box.diff({"instance": "i1"}, {"save": "s1"}) == {"differing": ["box/n"]}
    assert box.diff({"instance": "i2"}, {"save": "s1"}) == {"differing": []}
    box.drop_save("s1")
    with pytest.raises(PartError) as e:
        box.inspect({"save": "s1"}, [])
    assert e.value.code == -32002


def _spend(config, state, args):
    if args["usd"] > config["limit"]:
        raise Denied({"code": "OVER_LIMIT", "requested_usd": args["usd"]}, f"Denied: USD {args['usd']}")
    state["spent"] += args["usd"]
    return Done({"spent": state["spent"]}, writes=["wallet/spent"])


class Wallet(JsonStatePart):
    NAME = "wallet"
    TOOLS = {"spend": Tool("spend", "Spend.", "write", (Param("usd", "number"),), _spend)}

    def describe_fields(self):
        return {"version": "0.1.0", "fingerprint": {"wallet": 1}}

    def new_state(self, config):
        return {"spent": 0, "denials": []}

    def docs(self, config, state):
        return {"wallet/spent": {"spent": state["spent"]}, **denials_doc(state)}


def test_a_denial_is_the_structured_error_a_numbered_record_and_a_denial_event():
    w = Wallet()
    w.start("i1", {"limit": 100}, 0, 0)
    out = w.call_tool("i1", "spend", {"usd": 500}, 0)
    record = {"n": 1, "tool": "spend", "args": {"usd": 500}, "code": "OVER_LIMIT", "requested_usd": 500}
    assert out == {"error": '{"code": "OVER_LIMIT", "requested_usd": 500}', "reads": [], "writes": [DENIALS],
                   "events": [{"kind": "denial", "text": "Denied: USD 500", "data": record}]}
    assert w.call_tool("i1", "spend", {"usd": 600}, 0)["events"][0]["data"]["n"] == 2
    assert w.call_tool("i1", "spend", {"usd": 50}, 0)["result"] == {"spent": 50}
    docs = w.inspect({"instance": "i1"}, [])["docs"]
    assert DENIALS == "_audit/denials"
    assert [d["requested_usd"] for d in docs[DENIALS]["denials"]] == [500, 600]
    assert denial({"denials": [{}, {}]}, {"node": "x"}) == {"n": 3, "node": "x"}


def test_scenarios_are_read_at_a_path_and_set_by_the_scenario_tool():
    top = Scenarios()
    nested = Scenarios(("lims",), sort=True)
    config = {"scenarios": {"zeta": {"x": 1}, "alpha": {}}, "lims": {"scenarios": {"zeta": {"y": 2}, "alpha": {}}}}
    assert top.names(config) == ["base", "zeta", "alpha"]
    assert nested.names(config) == ["base", "alpha", "zeta"]
    assert top.names({}) == ["base"]
    assert (top.overlay(config, {"scenario": "zeta"}), nested.overlay(config, {"scenario": "zeta"})) == ({"x": 1},
                                                                                                         {"y": 2})
    assert top.overlay(config, {"scenario": "base"}) == {}
    state = {"scenario": "base"}
    done = nested.set(config, state, {"name": "zeta"}, {}, {})
    assert (state["scenario"], done.result, done.writes) == ("zeta", {"scenario": "zeta"}, ["scenario/current"])
    with pytest.raises(Refusal, match=r"unknown scenario nope \(known: base, alpha, zeta\)"):
        nested.set(config, state, {"name": "nope"}, {}, {})
