from arena_part import Part
from arena_part.world_time import WorldTime, rfc3339, unix_nanos
from conftest import Client


def test_stamps_are_the_epoch_plus_the_clock():
    w = WorldTime(1_773_122_400_000, 3.0)
    assert w.at(150_000) == 1_773_122_550_000
    assert unix_nanos(1_773_122_550_000) == "1773122550000000000"
    assert rfc3339(1_773_122_550_000) == "2026-03-10T06:02:30.000Z"
    assert rfc3339(1_773_122_550_042) == "2026-03-10T06:02:30.042Z"


def test_between_advances_time_runs_on_but_stands_still_past_the_lead_and_never_goes_back():
    w, t0 = WorldTime(0, 2.0), 100.0
    w.observe(10_000, now=t0)
    assert w.now_ms(now=t0 + 0.2) == 10_400
    assert w.now_ms(now=t0 + 5) == 12_000  # a pause: capped at 1 s of wall time, 2 s of world time
    w.observe(10_600, now=t0 + 6)  # the clock resumed behind the cap
    assert w.now_ms(now=t0 + 6) == 12_000  # never backwards
    assert w.now_ms(now=t0 + 6.75) == 12_100


def test_at_speed_the_lead_grows_with_the_speed():
    w, t0 = WorldTime(0, 10.0), 100.0
    w.observe(0, now=t0)
    assert w.now_ms(now=t0 + 0.2) == 2_000  # one 200 ms tick at speed 10
    assert w.now_ms(now=t0 + 3) == 10_000  # capped at 1 s of wall time


class Timed(Part):
    def start(self, instance, config, seed, clock_ms, from_=None):
        pass

    def advance(self, instance, clock_ms):
        return {"changed": False, "events": []}


def test_start_keeps_world_time():
    part = Timed()
    c = Client(part)
    start = {"instance": "i", "config": {}, "seed": 1, "clock_ms": 0,
             "world_time": {"epoch_ms": 1_000, "speed": 1.0}}
    assert "error" not in c.call("start", start)
    assert part.world_time("i").at(5) == 1_005
    c.call("advance", {"instance": "i", "clock_ms": 50})
    assert part.world_time("i").now_ms() >= 1_050
    assert "error" not in c.call("start", {"instance": "j", "config": {}, "seed": 1, "clock_ms": 0})
    assert part.world_time("j") is None
