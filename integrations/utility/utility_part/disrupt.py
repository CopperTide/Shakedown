"""The disruptions a task's setup may schedule at the utility (shakedown_part.disruptions): a demand-response event,
a window of peak prices in which the site is asked to hold its load at or below a target (its notice arrives when
it fires; the window opens `lead_min` minutes later and lasts `minutes`), and a meter that reads `percent` low from
then on (what agents read is low; the meter's state, which the benchmark scores, keeps the true load)."""
from shakedown_part.disruptions import Fired, Kind
from shakedown_part.params import Refusal

from . import meter

PROGRAM = "Peak-demand response"


def _check(config, do) -> None:
    if do["target_kw"] < 0:
        raise Refusal("target_kw must be 0 or more")
    if do["lead_min"] < 0 or do["minutes"] < 1:
        raise Refusal("lead_min must be 0 or more and minutes 1 or more")


def _event(config, state, do) -> Fired:
    now = state["clock_ms"]
    start = now + do["lead_min"] * meter.MINUTE_MS
    ev = {"id": do["event"], "program": do.get("program") or PROGRAM, "target_w": meter.watts(do["target_kw"]),
          "notice_ms": now, "start_ms": start, "end_ms": start + do["minutes"] * meter.MINUTE_MS,
          "ms_in": 0, "wms_in": 0, "ms_over": 0, "max_w": 0, "response": None}
    ev["status"] = meter.status_at(ev, now)
    state["events"][ev["id"]] = ev
    return Fired([f"dr/{ev['id']}"], [meter.window_event(config, ev)])


def _check_low(config, do) -> None:
    if not 1 <= do["percent"] <= 99:
        raise Refusal("percent must be between 1 and 99")


def _low(config, state, do) -> list:
    state["meter"]["read_factor"] = 1 - do["percent"] / 100
    return []


KINDS = {
    "demand_response": Kind({"event": "string", "target_kw": "number", "lead_min": "integer", "minutes": "integer"},
                            _event, _check),
    "meter_low": Kind({"percent": "integer"}, _low, _check_low),
}
