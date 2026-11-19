import json

import pytest

from conftest import T0, Clock
from gpupod.ledger import Ledger


def _ledger(tmp_path, clock):
    return Ledger(tmp_path / "ledger.jsonl", now=clock.now)


def test_entries_are_json_lines_stamped_in_utc(tmp_path):
    clock = Clock()
    led = _ledger(tmp_path, clock)
    led.append({"kind": "quote", "gpu_id": "g", "usd_h": 2.69})
    line = json.loads((tmp_path / "ledger.jsonl").read_text().splitlines()[0])
    assert line == {"at": "2026-11-19T21:29:00Z", "kind": "quote", "gpu_id": "g", "usd_h": 2.69}


def test_the_ledger_refuses_keys(tmp_path):
    led = _ledger(tmp_path, Clock())
    for entry in ({"kind": "start", "api_key": "x"}, {"kind": "start", "env": {"VLLM_API_KEY": "x"}},
                  {"kind": "ready", "token": "x"}):
        with pytest.raises(ValueError, match="never holds"):
            led.append(entry)
    assert not (tmp_path / "ledger.jsonl").exists()


def test_spent_counts_start_to_stop_hours_and_open_sessions(tmp_path):
    clock = Clock()
    led = _ledger(tmp_path, clock)
    led.append({"kind": "start", "pod_id": "p1", "usd_h": 2.0})
    clock.sleep(3600)
    led.append({"kind": "stop", "pod_id": "p1", "by": "owner"})
    clock.sleep(7200)
    led.append({"kind": "start", "pod_id": "p1", "usd_h": 2.0})
    clock.sleep(1800)
    led.append({"kind": "openrouter", "usd": 0.04, "branches": ["b1"]})
    assert led.spent() == pytest.approx(2.0 + 1.0 + 0.04)


def test_spent_prefers_reconciled_totals(tmp_path):
    clock = Clock()
    led = _ledger(tmp_path, clock)
    led.append({"kind": "start", "pod_id": "p1", "usd_h": 2.69})
    clock.sleep(2 * 3600)
    led.append({"kind": "stop", "pod_id": "p1", "by": "idle"})
    led.append({"kind": "reconcile", "pod_id": "p1", "usd_actual": 5.10})
    assert led.spent() == pytest.approx(5.10)
    clock.sleep(600)
    led.append({"kind": "start", "pod_id": "p1", "usd_h": 2.69})
    clock.sleep(3600)
    assert led.spent() == pytest.approx(5.10 + 2.69)


def test_open_session_is_the_last_pod_not_deleted(tmp_path):
    clock = Clock()
    led = _ledger(tmp_path, clock)
    assert led.open_session() is None
    led.append({"kind": "start", "pod_id": "p1", "gpu_id": "g", "usd_h": 2.69})
    assert led.open_session() == {"pod_id": "p1", "gpu_id": "g", "usd_h": 2.69,
                                  "started_at": T0, "stopped": False}
    led.append({"kind": "stop", "pod_id": "p1", "by": "idle"})
    assert led.open_session()["stopped"] is True
    led.append({"kind": "delete", "pod_id": "p1"})
    assert led.open_session() is None
