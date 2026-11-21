"""comparison.json, what stage 6's page reads: per design and run what reached production, what broke, cost, time
and the steps that mattered with the reasoning just before each, checked for one world, paired seeds and equal caps."""
import json

from shaked_fake import detail, fake_shaked, routes

from factory_designs.comparison import export

LEDGER = [{"at": "2026-11-20T09:00:00Z", "kind": "start", "pod_id": "p1", "gpu_id": "H100 SXM", "usd_h": 2.69}]


def exported(tmp_path, ledger=LEDGER, **kw):
    path = tmp_path / "ledger.jsonl"
    path.write_text("".join(json.dumps(e) + "\n" for e in ledger))
    sd, _ = fake_shaked(routes(*detail(**kw)))
    return export(sd, "e1", path)


def test_export_shape_and_values(tmp_path):
    out = exported(tmp_path)
    head = out["experiment"]
    assert (head["id"], head["world"], head["seed"], head["world_minutes"], head["max_cost_usd"]) == (
        "e1", "northwind-labs", 20261119, 60, 1.5)
    assert (head["model"], head["provider"], head["gpu"]) == ("openai/gpt-oss-120b", "local:runpod",
                                                              {"id": "H100 SXM", "usd_h": 2.69})
    a = out["setups"][0]["runs"][0]
    assert [(k["seq"], k["agent"], k["what"], k["reasoning_seq"]) for k in a["key_steps"]] == [
        (4, "release", "merge", 3), (5, "pipeline", "deploy", None)]
    assert a["checks"]["production_error_rate"] == {"passed": True, "measure": 0.002, "safety": False}
    assert a["checks"]["no_unreviewed_merge"]["safety"] is True
    assert a["production"] == {"reached": {"storefront": True, "orders": True, "inventory": False},
                               "error_rate_30m": 0.002, "open_incidents": 1, "contracts_held": False}
    assert a["tokens"] == {"prompt": 800, "completion": 60} and a["cost_usd_hosted_equiv"] == 0.42
    assert (a["wall_minutes"], a["world_minutes"], a["gpu_minutes"]) == (30.0, 60.0, 30.0)
    assert a["gpu_usd"] == round(30 / 60 * 2.69, 4) and a["stopped"] == "window"
    assert out["fair"] == {"same_world": True, "paired_seeds": True, "same_caps": True, "problems": []}
    summary = out["setups"][1]["summary"]
    assert summary["pass_rate"]["changes_reached_production"] == 1.0
    assert summary["median"]["cost_usd_hosted_equiv"] == 0.42
    assert out["gpu_total_usd"] == round(2 * 30 / 60 * 2.69, 4)


def test_an_approval_over_rest_is_a_key_step(tmp_path):
    b = exported(tmp_path)["setups"][1]["runs"][0]
    assert [(k["what"], k["agent"]) for k in b["key_steps"]] == [("approval", "engineer")]


def test_export_flags_runs_on_different_worlds_or_seeds(tmp_path):
    fair = exported(tmp_path, b_seed=12, b_bench="other-world")["fair"]
    assert (fair["same_world"], fair["paired_seeds"]) == (False, False)
    assert len(fair["problems"]) == 2 and all(isinstance(p, str) for p in fair["problems"])


def test_a_run_stopped_at_the_spend_limit_is_marked(tmp_path):
    assert exported(tmp_path, b_stopped="spend")["setups"][1]["runs"][0]["stopped"] == "spend limit"


def test_without_a_gpu_session_the_gpu_figures_are_zero(tmp_path):
    out = exported(tmp_path, ledger=[])
    assert out["experiment"]["gpu"] is None and out["gpu_total_usd"] == 0
    assert out["setups"][0]["runs"][0]["gpu_usd"] == 0
