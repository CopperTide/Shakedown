"""A one-task benchmark scored by named checks and drawn as a twin, and the named check itself."""
import json

import pytest

from arena_part import PartError
from arena_part.benchmark import SingleTaskBenchmark, check, load


class Demo(SingleTaskBenchmark):
    NAME, VERSION, TASK = "demo", "0.1.0", "the-task"
    INSTRUCTIONS, INSTRUCTIONS_TITLE = "do it", "How"
    TEAM, TEAMS = {"name": "solo"}, [{"name": "pair"}]

    def world(self):
        return [{"part": "a", "integration": "x", "config": self.config["a"]},
                {"part": "b", "integration": "y", "config": {}}]

    def task(self):
        return {"id": self.TASK, "reason": "why"}

    def evaluate(self, docs):
        return [check("has_docs", docs, f"{len(docs)} parts")]

    def twin(self, docs):
        return {"parts": sorted(docs)}


def test_describe_names_the_world_the_task_and_the_teams_and_views_every_part():
    d = Demo({"a": {"n": 1}}).describe()
    assert list(d) == ["name", "version", "protocol", "world", "tasks", "instructions", "instructions_title",
                       "team", "teams", "tools", "scored", "view"]
    assert (d["name"], d["version"], d["protocol"], d["tasks"]) == ("demo", "0.1.0", 1,
                                                                    [{"id": "the-task", "reason": "why"}])
    assert (d["team"], d["teams"], d["tools"], d["scored"]) == ({"name": "solo"}, [{"name": "pair"}], [], True)
    assert d["view"] == {"docs": ["a", "b"]}


def test_score_and_view_are_the_checks_and_the_twin_of_the_one_task():
    b = Demo({"a": {}})
    assert b.score("the-task", {"a": {}}) == {"checks": [
        {"name": "has_docs", "passed": True, "detail": "1 parts", "safety": False}]}
    assert b.view({}, {"b": {}, "a": {}}) == {"view": {"parts": ["a", "b"]}}
    with pytest.raises(PartError) as e:
        b.score("other", {})
    assert e.value.message == "unknown task other"


def test_config_is_read_from_data_when_not_given(tmp_path):
    path = tmp_path / "c.json"
    path.write_text(json.dumps({"a": {"n": 2}}))
    assert load(path) == {"a": {"n": 2}}

    class FromFile(Demo):
        DATA = path

    assert FromFile().config == {"a": {"n": 2}}


def test_a_check_is_safety_or_not_and_has_a_measure_only_when_given():
    assert check("x", 0, "none") == {"name": "x", "passed": False, "detail": "none", "safety": False}
    assert check("y", [1], "d", safety=True)["safety"] is True
    measure = {"value": 0.8, "unit": "OEE"}
    assert check("z", True, "d", measure=measure)["measure"] == measure
