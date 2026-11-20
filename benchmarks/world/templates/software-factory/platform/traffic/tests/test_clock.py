import json

from traffic.clock import Clock

EPOCH = 1_773_122_400_000


class Wall:
    def __init__(self, ms: int):
        self.ms = ms

    def __call__(self) -> float:
        return self.ms / 1000


def write(path, clock_ms: int, wall_ms: int, speed: float = 1.0):
    path.write_text(json.dumps({"epoch_ms": EPOCH, "clock_ms": clock_ms, "world_ms": EPOCH + clock_ms,
                                "speed": speed, "wall_ms": wall_ms}))


def test_world_and_clock_time_come_from_the_clock_file(tmp_path):
    wall = Wall(50_000)
    write(tmp_path / "clock.json", 60_000, 50_000)
    now = Clock(tmp_path / "clock.json", wall).now()
    assert (now.world_ms, now.clock_ms) == (EPOCH + 60_000, 60_000)


def test_time_runs_on_at_speed_but_stands_still_a_second_past_the_last_advance(tmp_path):
    wall, path = Wall(50_000), tmp_path / "clock.json"
    write(path, 60_000, 50_000, speed=2.0)
    clock = Clock(path, wall)
    wall.ms = 50_300
    assert clock.now().clock_ms == 60_600
    wall.ms = 59_000                                      # a paused world: no advance for 9 s
    assert clock.now().clock_ms == 61_000


def test_time_never_goes_back(tmp_path):
    wall, path = Wall(50_000), tmp_path / "clock.json"
    write(path, 60_000, 50_000)
    clock = Clock(path, wall)
    wall.ms = 50_800
    assert clock.now().clock_ms == 60_800
    write(path, 60_500, 50_800)                           # an advance that lands behind what was given
    assert clock.now().clock_ms == 60_800


def test_without_the_file_world_time_is_wall_time_from_the_start(tmp_path):
    wall = Wall(1_000_000)
    clock = Clock(tmp_path / "missing.json", wall)
    wall.ms = 1_004_000
    now = clock.now()
    assert (now.world_ms, now.clock_ms, clock.source()) == (1_004_000, 4_000, "wall")
