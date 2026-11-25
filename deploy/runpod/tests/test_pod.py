import json
import stat

import pytest

from conftest import SHAKED_HOST, POD_HOST, shaked, make_ops, route, vllm
from gpupod.pod import create_argv, delete, reconcile, start, stop
from gpupod.spec import load_spec


def _healthy_ops(tmp_path, runner, clock, out, runs=()):
    http = route({POD_HOST: vllm("unused"), SHAKED_HOST: shaked(list(runs))})
    return make_ops(tmp_path, runner, clock, http=http, out=out)


def test_start_prints_the_rate_before_anything_else(tmp_path, runner, clock):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    assert start(ops, confirm_rate=2.69, hours=8) == 0
    assert out[0].startswith("NVIDIA H100 80GB HBM3, community: $2.69/h (runpodctl gpu list, ")
    assert runner.commands()[0] == ("gpu", "list")
    assert ("pod", "create") in runner.commands()


def test_start_refuses_an_unconfirmed_or_changed_rate(tmp_path, runner, clock):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    assert start(ops, confirm_rate=2.59) == 2
    assert out[0].startswith("NVIDIA H100 80GB HBM3, community: $2.69/h")
    assert "$2.69/h, not the $2.59/h confirmed" in out[-1]
    assert ("pod", "create") not in runner.commands()
    assert start(ops, confirm_rate=2.694) == 0  # within $0.005 of the quote
    assert ("pod", "create") in runner.commands()


def test_a_changed_rate_creates_nothing(tmp_path, runner, clock):
    out: list[str] = []
    assert start(_healthy_ops(tmp_path, runner, clock, out), confirm_rate=2.80) == 2
    assert runner.commands() == [("gpu", "list")]


def test_start_never_falls_back_silently(tmp_path, runner, clock):
    runner.set_gpu("NVIDIA H100 80GB HBM3", available=False)
    out: list[str] = []
    assert start(_healthy_ops(tmp_path, runner, clock, out), confirm_rate=2.69) == 2
    assert "NVIDIA H100 NVL" in out[-1] and "$2.59/h" in out[-1]
    assert ("pod", "create") not in runner.commands()


@pytest.mark.parametrize("spent,code", [(45.0, 2), (28.48, 0)])
def test_start_refuses_past_the_budget(tmp_path, runner, clock, spent, code):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    ops.ledger.append({"kind": "openrouter", "usd": spent, "branches": []})
    assert start(ops, confirm_rate=2.69, hours=8) == code
    assert out[0].startswith("NVIDIA H100 80GB HBM3, community: $2.69/h")
    if code:
        assert "would bring the spend to $66.52, over the $50.00 budget" in out[-1]
        assert ("pod", "create") not in runner.commands()


def test_create_argv():
    spec = load_spec()
    argv = create_argv(spec, spec.gpus[0], {"HF_HOME": "/workspace/hf", "VLLM_API_KEY": "k1"}, False)
    assert argv[:2] == ["pod", "create"]
    flags = dict(zip(argv[2::2], argv[3::2]))
    assert flags == {
        "--name": "shakedown-gpt-oss", "--image": spec.image, "--gpu-id": "NVIDIA H100 80GB HBM3",
        "--cloud-type": "COMMUNITY", "--min-cuda-version": "12.9", "--container-disk-in-gb": "40",
        "--volume-in-gb": "100", "--volume-mount-path": "/workspace", "--ports": "8000/http",
        "--env": json.dumps({"HF_HOME": "/workspace/hf", "VLLM_API_KEY": "k1"}),
        "--docker-args": "openai/gpt-oss-120b --revision b5c939de8f754692c1647ca79fbf85e8c1e70f8a "
                         "--host 0.0.0.0 --port 8000 --tool-call-parser openai --enable-auto-tool-choice "
                         "--reasoning-parser openai_gptoss --max-model-len 131072 "
                         "--gpu-memory-utilization 0.92 --enable-prompt-tokens-details"}
    low = dict(zip(*[iter(create_argv(spec, spec.gpus[0], {}, True)[2:])] * 2))["--docker-args"]
    assert low.endswith("--enable-prompt-tokens-details --max-num-batched-tokens 1024")


def test_env_file_is_private_and_the_ledger_has_no_key(tmp_path, runner, clock):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    assert start(ops, confirm_rate=2.69) == 0
    env = ops.env_path.read_text()
    assert stat.S_IMODE(ops.env_path.stat().st_mode) == 0o600
    lines = dict(line.split("=", 1) for line in env.splitlines() if line)
    assert lines["SHAKEDOWN_LOCAL_MODELS"] == f"runpod=https://{POD_HOST}/v1"
    assert lines["SHAKEDOWN_LOCAL_PRICE_RUNPOD"] == "0.15,0.60"
    key = lines["SHAKEDOWN_LOCAL_KEY_RUNPOD"]
    assert len(key) >= 32
    create = next(c for c in runner.calls if c[:2] == ["pod", "create"])
    assert json.loads(create[create.index("--env") + 1])["VLLM_API_KEY"] == key
    assert key not in ops.ledger.path.read_text()
    assert all(key not in line for line in out)
    kinds = [e["kind"] for e in ops.ledger.entries()]
    assert kinds == ["quote", "start", "ready"]
    assert ops.spawn.argvs and ops.spawn.argvs[0][-2:] == ["watch", "p7h9k2m4n6"]
    assert f"https://{POD_HOST}/v1" in out[-1] and "public URL" in out[-1]


def test_start_reuses_a_stopped_pod_with_its_key(tmp_path, runner, clock):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    start(ops, confirm_rate=2.69)
    key = ops.env_path.read_text()
    assert stop(ops) == 0
    assert start(ops, confirm_rate=2.69) == 0
    assert runner.commands().count(("pod", "create")) == 1
    assert ["pod", "start", "p7h9k2m4n6"] in runner.calls
    assert ops.env_path.read_text() == key
    starts = [e for e in ops.ledger.entries() if e["kind"] == "start"]
    assert [s["reused"] for s in starts] == [False, True]


def test_an_unhealthy_start_stops_the_pod(tmp_path, runner, clock):
    out: list[str] = []
    http = route({POD_HOST: vllm("k", healthy=False)})
    ops = make_ops(tmp_path, runner, clock, http=http, out=out)
    assert start(ops, confirm_rate=2.69) == 1
    assert ["pod", "stop", "p7h9k2m4n6"] in runner.calls
    stops = [e for e in ops.ledger.entries() if e["kind"] == "stop"]
    assert stops and stops[0]["by"] == "unhealthy"
    assert "not healthy after 20 min" in out[-1]


def test_a_balance_above_what_is_left_is_warned(tmp_path, runner, clock):
    runner.answers[("user",)]["clientBalance"] = 80.0
    out: list[str] = []
    assert start(_healthy_ops(tmp_path, runner, clock, out), confirm_rate=2.69) == 0
    assert any("balance is $80.00, more than the $50.00 left" in line for line in out)


def test_delete_is_refused_while_a_run_is_running(tmp_path, runner, clock):
    out: list[str] = []
    runs = [{"id": "r1", "task_id": "sprint", "branches": [{"id": "b1", "status": "running"}]}]
    ops = _healthy_ops(tmp_path, runner, clock, out, runs=runs)
    start(ops, confirm_rate=2.69)
    assert delete(ops, force=False) == 2
    assert ("pod", "delete") not in runner.commands()
    assert delete(ops, force=True) == 0
    assert ["pod", "delete", "p7h9k2m4n6"] in runner.calls
    assert ops.ledger.open_session() is None


def test_reconcile_records_billed_totals(tmp_path, runner, clock):
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    start(ops, confirm_rate=2.69)
    assert reconcile(ops) == 0
    call = next(c for c in runner.calls if c[:2] == ["billing", "pods"])
    assert call == ["billing", "pods", "--pod-id", "p7h9k2m4n6", "--bucket-size", "hour",
                    "--start-time", "2026-11-19T21:29:00Z"]
    rec = [e for e in ops.ledger.entries() if e["kind"] == "reconcile"]
    assert rec[0]["pod_id"] == "p7h9k2m4n6" and rec[0]["usd_actual"] == pytest.approx(4.01)
    assert ops.ledger.spent() == pytest.approx(4.01)


def test_status_shows_the_pod_and_the_spend(tmp_path, runner, clock):
    from gpupod.pod import status
    out: list[str] = []
    ops = _healthy_ops(tmp_path, runner, clock, out)
    assert status(ops) == 0 and out == ["Spent $0.00 of $50.00.", "No pod."]
    start(ops, confirm_rate=2.69)
    out.clear()
    clock.sleep(3600)
    status(ops)
    assert out[0] == "Spent $2.69 of $50.00."
    assert out[1].startswith("Pod p7h9k2m4n6 (NVIDIA H100 80GB HBM3, $2.69/h): RUNNING, runtime initializing")


def test_a_pod_toml_with_the_old_shaked_provider_key_still_loads(tmp_path):
    from gpupod.spec import POD_TOML

    old = tmp_path / "pod.toml"
    old.write_text(POD_TOML.read_text().replace("shaked_provider =", "shaked_provider ="))
    assert "shaked_provider" in old.read_text()
    assert load_spec(old).shaked_provider == load_spec().shaked_provider == "runpod"
