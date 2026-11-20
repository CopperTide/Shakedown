"""The comparison experiment is data (experiment.yaml); creating it makes each design a Code connection and posts
the experiment with paired seeds, one window and one spend cap."""
from shaked_fake import body, fake_shaked

from factory_designs.experiment import EXPERIMENT, REPO, create, ensure_connection, load_experiment
from factory_designs.settings import load_settings


def test_the_experiment_is_data_with_both_designs():
    exp = load_experiment()
    assert (exp["task"], exp["runs"], exp["world_minutes"], exp["max_cost_usd"]) == ("sprint", 3, 60, 1.5)
    assert [s["module"] for s in exp["setups"]] == ["factory_designs.team_a", "factory_designs.solo_b"]


def test_create_posts_the_experiment_with_paired_seed_window_and_cap():
    made = []
    sd, seen = fake_shaked({"GET /api/connections": [],
                            "POST /api/connections": lambda r: made.append(body(r)) or {"id": f"c{len(made)}"},
                            "POST /api/experiments": {"id": "e1"}})
    assert create(sd, EXPERIMENT, provider="local:runpod", runs=1, world_minutes=20) == "e1"
    posted = body(seen[-1])
    assert posted == {"name": "Hierarchical team vs single agent · openai/gpt-oss-120b on local:runpod",
                      "task": "sprint", "runs": 1, "seed": 20261119, "world_minutes": 20, "max_cost_usd": 1.5,
                      "setups": [{"name": "A · LangGraph team", "connection": "c1"},
                                 {"name": "B · Agents SDK + test loop", "connection": "c2"}]}
    assert made[0]["command"] == ["uv", "run", "--project", "designs/software-factory", "python", "-m",
                                  "factory_designs.team_a"]
    assert made[0]["workdir"] == str(REPO)
    assert made[1]["env"] == {"FACTORY_PROVIDER": "local:runpod", "FACTORY_MODEL": "openai/gpt-oss-120b"}


def test_connections_are_reused_by_name():
    sd, seen = fake_shaked({"GET /api/connections": [{"id": "old", "name": "A · gpt-oss"}]})
    assert ensure_connection(sd, "A · gpt-oss", "factory_designs.team_a", load_settings(env={})) == "old"
    assert [r.method for r in seen] == ["GET"]


def test_a_benchmark_and_task_may_be_named():
    sd, seen = fake_shaked({"GET /api/connections": [{"id": "c", "name": n} for n in (
        "A · LangGraph team · openai/gpt-oss-120b on local:runpod",
        "B · Agents SDK + test loop · openai/gpt-oss-120b on local:runpod")],
        "POST /api/experiments": {"id": "e2"}})
    create(sd, EXPERIMENT, task="contract-change", benchmark="northwind-labs")
    posted = body(seen[-1])
    assert (posted["task"], posted["benchmark"], posted["runs"]) == ("contract-change", "northwind-labs", 3)
