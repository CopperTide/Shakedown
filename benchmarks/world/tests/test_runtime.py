import json
import subprocess
import sys

import pytest

from arena_part.errors import PartError
from worldbench.runtime import WorldBenchmark


def test_describe_is_the_world_with_its_folder_filled_in(built_shop):   # a world.json written by generate into tmp
    d = WorldBenchmark(built_shop / "world.json").describe()
    assert d["scored"] is True and d["tools"] == []
    assert all("${WORLD_DIR}" not in json.dumps(p) for p in d["world"])
    assert d["team"]["id"] == "shop-team" and d["tasks"][0]["id"] == "alpha-task"


def test_describe_has_what_arenad_reads_and_nothing_of_the_build(built_shop):
    d = WorldBenchmark(built_shop / "world.json", version="1.0.0+abc").describe()
    assert (d["name"], d["version"], d["protocol"]) == ("example-co", "1.0.0+abc", 1)
    assert d["world"][0]["config"]["seed"] == f"{built_shop.resolve()}/files/seed/"
    assert set(d["tasks"][0]) == {"id", "reason", "ticket", "setup", "clock_start"}
    assert [t["id"] for t in d["teams"]] == ["solo"] and d["instructions_title"] == "Shop policy"
    assert d["instructions"] == "Follow the Example Co policy.\n" and len(d["links"]) == 2 and len(d["gates"]) == 1
    assert "scenario_options" not in d and "people" not in d


def test_score_evaluates_the_tasks_checks(built_shop):
    out = WorldBenchmark(built_shop / "world.json").score("alpha-task", {"alpha": {"order/1": {"status": "shipped"}}})
    assert [c["name"] for c in out["checks"]] == ["shipped"] and out["checks"][0]["passed"]


def test_an_unknown_task_is_invalid_params(built_shop):
    with pytest.raises(PartError) as e:
        WorldBenchmark(built_shop / "world.json").score("nope", {})
    assert (e.value.code, e.value.message) == (-32602, "no task nope in this world")


def test_the_program_answers_describe_over_the_line_protocol(built_shop):
    proc = subprocess.run([sys.executable, "-m", "worldbench", "--world", "world.json", "--version", "1.0.0+abc"],
                          cwd=built_shop, input='{"jsonrpc":"2.0","id":1,"method":"describe"}\n',
                          capture_output=True, text=True, timeout=30)
    reply = json.loads(proc.stdout.splitlines()[0])
    assert reply["id"] == 1 and reply["result"]["name"] == "example-co" and reply["result"]["version"] == "1.0.0+abc"
