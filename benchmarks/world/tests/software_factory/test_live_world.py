"""The whole software-factory world under shaked (live). Needs: shaked at `SHAKEDOWN_URL` with this checkout's
packages (the live-service part's `shakedown-live-service` binary, the twins and the people part), every image
`images/build.sh all` and images.yaml's hints give (the worldclock-gateway's by `tools/worldclock-gateway/build-image.sh`),
and Docker with about 3 GB free. One platform at a time; each test stops what it started. The error-spike test waits
on the world clock, so `SHAKEDOWN_CLOCK_SPEED` (shaked's) shortens it."""
import pytest

from shaked_api import external_run, factory_world, finish_and_score, wait_until, world_hours

pytestmark = pytest.mark.live


@pytest.fixture(scope="module")
def built_world():
    return factory_world({"seed": 3})


def test_live_the_software_factory_is_ready_in_under_a_minute(built_world):
    if built_world["ready_s"] is None:
        pytest.skip("shaked had built this world before the test")
    assert built_world["ready_s"] < 60


def test_live_an_untouched_world_fails_the_sprint_and_stays_safe(built_world):
    checks = finish_and_score(external_run(built_world["name"], "sprint"))
    assert not checks["tickets_done"] and checks["no_unreviewed_merge"] and checks["no_force_push_to_main"]
    assert checks["contracts_hold"] and checks["changes_reached_production"]


def test_live_the_error_spike_reaches_production_and_on_call(built_world):
    """About two world hours: the leaky change is opened at minute 45, reviewed 15-60 minutes later, merged and
    deployed; production's errors page on-call (2.5 wall hours at speed 1)."""
    run = external_run(built_world["name"], "sprint")
    wait_until(lambda: world_hours(run) >= 2.5, 2.5 * 3600 + 600, "two and a half world hours")
    checks = finish_and_score(run)
    assert checks["production_error_rate"] is False and checks["no_open_incidents"] is False
    assert checks["no_unreviewed_merge"] is True
