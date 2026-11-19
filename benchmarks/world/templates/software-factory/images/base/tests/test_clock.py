import json

from platform_lib.clock import MAX_LEAD_MS, WorldClock

W = 1_800_000_000_000  # a wall clock instant, ms


def clock_at(tmp_path, doc: dict, wall_ms: list) -> WorldClock:
    path = tmp_path / "clock.json"
    path.write_text(json.dumps(doc))
    return WorldClock(path, wall=lambda: wall_ms[0] / 1000)


def test_world_time_runs_on_from_the_last_advance(tmp_path):
    wall = [W + 400]
    c = clock_at(tmp_path, {"world_ms": 1_773_122_460_000, "wall_ms": W, "speed": 1.0}, wall)
    assert c.now_ms() == 1_773_122_460_400


def test_a_pause_stops_world_time(tmp_path):
    wall = [W + 5_000]
    c = clock_at(tmp_path, {"world_ms": 10_000, "wall_ms": W, "speed": 1.0}, wall)
    assert c.now_ms() == 10_000 + MAX_LEAD_MS == 11_000


def test_world_time_never_goes_back(tmp_path):
    wall = [W + 900]
    c = clock_at(tmp_path, {"world_ms": 10_000, "wall_ms": W, "speed": 1.0}, wall)
    assert c.now_ms() == 10_900
    (tmp_path / "clock.json").write_text(json.dumps({"world_ms": 10_500, "wall_ms": W + 900, "speed": 1.0, "x": 1}))
    assert c.now_ms() == 10_900


def test_without_a_clock_file_it_is_wall_time(tmp_path):
    c = WorldClock(tmp_path / "missing.json", wall=lambda: 12.5)
    assert c.now_ms() == 12_500 and c.now() == 12.5
