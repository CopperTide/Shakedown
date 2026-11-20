"""The twin stamps what it makes in world time: the epoch shaked gives at start plus the branch clock."""
from arena_part.serve import dispatch

from conftest import Clock, Twin, api, config
from github_twin.part import GithubTwin
from github_twin.times import iso

EPOCH = 1_900_000_000_000  # far from the wall clock the tests' Clock gives


def test_seeded_and_new_records_carry_world_time():
    part = GithubTwin(clock=Clock())
    dispatch(part, "start", {"instance": "i", "config": config(), "seed": 1, "clock_ms": 0,
                             "world_time": {"epoch_ms": EPOCH, "speed": 1.0}})
    try:
        t = Twin(part)
        assert api(t, "GET", "/api/v3/repos/acme/app")["created_at"][:16] == iso(EPOCH)[:16]
        dispatch(part, "advance", {"instance": "i", "clock_ms": 3_600_000})
        issue = api(t, "POST", "/api/v3/repos/acme/app/issues", {"title": "Later"})
        assert issue["created_at"][:16] == iso(EPOCH + 3_600_000)[:16]
    finally:
        part.stop("i")
