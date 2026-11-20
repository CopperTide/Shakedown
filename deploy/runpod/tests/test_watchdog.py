from datetime import timedelta

from conftest import SHAKED_HOST, POD_HOST, T0, shaked, make_ops, route, vllm
from gpupod.spec import load_spec
from gpupod.watchdog import Probe, WatchState, decide, observe, parse_metrics, probe, watch

CFG = load_spec()
IDLE = Probe(running_requests=0, success_total=7.0, shaked_running=False, healthy=True)


def _after(minutes, state, p=IDLE, started=T0, rate=2.69, spent=1.0):
    now = T0 + timedelta(minutes=minutes)
    state = observe(state, p, now)
    return decide(now, state, started, rate, spent, CFG), state


def test_idle_pod_is_stopped_after_ten_minutes():
    state = observe(WatchState(), IDLE, T0)
    d, state = _after(9, state)
    assert not d.stop
    d, state = _after(10, state)
    assert (d.stop, d.by) == (True, "idle")


def test_requests_keep_the_pod():
    state = observe(WatchState(), IDLE, T0)
    d, state = _after(9, state, Probe(running_requests=1, success_total=7.0, shaked_running=False, healthy=True))
    d, state = _after(18, state)
    assert not d.stop
    d, state = _after(17 + 1, state, Probe(running_requests=0, success_total=9.0, shaked_running=False, healthy=True))
    d, state = _after(27, state)
    assert not d.stop


def test_a_running_shaked_run_keeps_the_pod():
    state = observe(WatchState(), IDLE, T0)
    busy = Probe(running_requests=0, success_total=7.0, shaked_running=True, healthy=True)
    for m in range(1, 31):
        d, state = _after(m, state, busy)
        assert not d.stop


def test_budget_and_max_hours_stop_the_pod():
    state = observe(WatchState(), IDLE, T0)
    busy = Probe(running_requests=2, success_total=7.0, shaked_running=True, healthy=True)
    d, state = _after(30, state, busy, spent=49.60)
    assert (d.stop, d.by) == (True, "budget")
    d, state = _after(30, state, busy, spent=49.0)
    assert not d.stop
    d, state = _after(8 * 60, state, busy, spent=30.0)
    assert (d.stop, d.by) == (True, "max_hours")


def test_ten_unhealthy_minutes_stop_the_pod():
    state = observe(WatchState(), IDLE, T0)
    sick = Probe(running_requests=None, success_total=None, shaked_running=True, healthy=False)
    d, state = _after(5, state, sick)
    assert not d.stop
    d, state = _after(6, state, IDLE)
    d, state = _after(7, state, sick)
    d, state = _after(16, state, sick)
    assert not d.stop
    d, state = _after(17, state, sick)
    assert (d.stop, d.by) == (True, "unhealthy")


def test_metrics_are_parsed():
    text = """# HELP vllm:num_requests_running Number of requests in model execution batches.
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{engine="0",model_name="openai/gpt-oss-120b"} 2.0
# TYPE vllm:request_success_total counter
vllm:request_success_total{engine="0",finished_reason="stop",model_name="openai/gpt-oss-120b"} 5.0
vllm:request_success_total{engine="0",finished_reason="length",model_name="openai/gpt-oss-120b"} 1.0
"""
    assert parse_metrics(text) == (2, 6.0)
    assert parse_metrics("") == (None, None)


def test_probe_reads_vllm_and_shaked():
    runs = [{"id": "r1", "branches": [{"id": "b1", "status": "passed"}, {"id": "b2", "status": "running"}]}]
    http = route({POD_HOST: vllm("k", metrics="vllm:num_requests_running 0\n"), SHAKED_HOST: shaked(runs)})
    p = probe(http, f"https://{POD_HOST}", "http://127.0.0.1:4510")
    assert p == Probe(running_requests=0, success_total=None, shaked_running=True, healthy=True)
    down = probe(route({}), f"https://{POD_HOST}", "http://127.0.0.1:4510")
    assert down == Probe(running_requests=None, success_total=None, shaked_running=None, healthy=False)


def test_the_loop_stops_the_pod_and_records_it(tmp_path, runner, clock):
    http = route({POD_HOST: vllm("k", metrics="vllm:num_requests_running 0\n"),
                  SHAKED_HOST: shaked([])})
    ops = make_ops(tmp_path, runner, clock, http=http)
    ops.ledger.append({"kind": "start", "pod_id": "p7h9k2m4n6", "gpu_id": "g", "usd_h": 2.69})
    assert watch(ops, "p7h9k2m4n6") == "idle"
    assert ["pod", "stop", "p7h9k2m4n6"] in runner.calls
    stops = [e for e in ops.ledger.entries() if e["kind"] == "stop"]
    assert stops[-1]["by"] == "idle" and stops[-1]["pod_id"] == "p7h9k2m4n6"
    assert clock.now() - T0 >= timedelta(minutes=10)


def test_the_loop_ends_when_the_pod_was_stopped_another_way(tmp_path, runner, clock):
    http = route({POD_HOST: vllm("k", metrics="vllm:num_requests_running 1\n"), SHAKED_HOST: shaked([])})
    ops = make_ops(tmp_path, runner, clock, http=http)
    ops.ledger.append({"kind": "start", "pod_id": "p7h9k2m4n6", "gpu_id": "g", "usd_h": 2.69})
    ops.ledger.append({"kind": "stop", "pod_id": "p7h9k2m4n6", "by": "owner"})
    assert watch(ops, "p7h9k2m4n6") is None
    assert ("pod", "stop") not in runner.commands()
