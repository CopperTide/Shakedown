"""The world's data a part's config or a benchmark call carries (docs/protocol.md, "World data")."""
from shakedown_part import benchmark, worlddata


def given(tmp_path):
    (tmp_path / "data").mkdir(exist_ok=True)
    (tmp_path / "data" / "db.json").write_text('{"users": {"u1": {"name": "Ada"}}}', encoding="utf-8")
    return {"digest": "abc", "dir": str(tmp_path), "world": "airline",
            "files": [{"path": "data/db.json", "seeds": "desk", "size": 35, "sha256": "f"}]}


def test_the_data_is_read_from_a_config_by_path_or_name(tmp_path):
    data = worlddata.of({"domain": "x", "world_data": given(tmp_path)})
    assert (data.digest, data.world, data.files[0].seeds) == ("abc", "airline", "desk")
    assert data.read_json("db.json")["users"]["u1"]["name"] == "Ada"
    assert data.path("data/db.json") == tmp_path / "data" / "db.json"
    assert data.find("policy.md") is None
    assert worlddata.of({"domain": "x"}) is None
    assert worlddata.of(None) is None


class Seen(benchmark.Benchmark):
    def describe(self):
        return {}

    def score(self, task, docs):
        data = self.world_data()
        return {"checks": [benchmark.check("data", data is not None, data.digest if data else "")]}

    def gold_actions(self, task):
        return {"actions": [], "digest": self.world_data().digest}


def test_a_benchmark_call_sees_its_world_data_only_while_it_runs(tmp_path):
    b = Seen()
    out = benchmark.dispatch(b, "score", {"task": "t", "docs": {}, "world_data": given(tmp_path)})
    assert out["checks"][0]["detail"] == "abc"
    assert b.world_data() is None
    assert benchmark.dispatch(b, "gold_actions", {"task": "t", "world_data": given(tmp_path)})["digest"] == "abc"
    plain = benchmark.dispatch(b, "score", {"task": "t", "docs": {}})
    assert plain["checks"][0]["passed"] is False
