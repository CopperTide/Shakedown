from traffic.schedule import multiplier, plan

FLAT = [1.0] * 24
PROFILE = {"journeys": {"browse": {"rpm": 30}, "order": {"rpm": 4}}, "day": {"start_hour": 9, "hourly": FLAT},
           "peaks": []}
HOUR = 3_600_000


def profile(**changes) -> dict:
    return {**PROFILE, **changes}


def count(entries, journey=None) -> int:
    return sum(1 for _, j in entries if journey in (None, j))


def test_the_schedule_is_seeded_and_does_not_depend_on_how_time_is_split():
    whole = plan(7, 0, 600_000, PROFILE)
    split = plan(7, 0, 300_000, PROFILE) + plan(7, 300_000, 600_000, PROFILE)
    assert whole == split and plan(8, 0, 600_000, PROFILE) != whole
    odd = plan(7, 0, 123_457, PROFILE) + plan(7, 123_457, 400_001, PROFILE) + plan(7, 400_001, 600_000, PROFILE)
    assert odd == whole


def test_no_requests_while_the_clock_stands_still():   # Review Focus 3
    assert plan(7, 300_000, 300_000, PROFILE) == []


def test_entries_are_in_time_order_inside_the_half_open_window():
    entries = plan(3, 60_000, 180_000, PROFILE)
    times = [t for t, _ in entries]
    assert times == sorted(times) and all(60_000 < t <= 180_000 for t in times)


def test_each_journey_arrives_at_about_its_rate():
    entries = plan(11, 0, HOUR, PROFILE)
    assert 30 * 60 * 0.9 < count(entries, "browse") < 30 * 60 * 1.1
    assert 4 * 60 * 0.8 < count(entries, "order") < 4 * 60 * 1.2


def test_the_rate_follows_the_hour_of_the_day_from_the_data():
    hourly = [1.0] * 24
    hourly[11], hourly[12] = 0.25, 0.25          # the run starts at 09:00 company time
    entries = plan(5, 0, 4 * HOUR, profile(day={"start_hour": 9, "hourly": hourly}))
    busy = count([e for e in entries if e[0] <= HOUR])
    quiet = count([e for e in entries if 2 * HOUR < e[0] <= 3 * HOUR])
    assert 3.4 < busy / quiet < 4.6


def test_the_multiplier_runs_linearly_between_hours_and_wraps_at_midnight():
    hourly = [0.0] * 24
    hourly[23], hourly[0] = 1.0, 0.5
    day = {"start_hour": 23, "hourly": hourly}
    assert multiplier(day, 0) == 1.0
    assert multiplier(day, HOUR // 2) == 0.75
    assert multiplier(day, HOUR) == 0.5


def test_a_peak_multiplies_every_journey_inside_its_window():
    peak = profile(peaks=[{"from_ms": HOUR, "to_ms": 2 * HOUR, "factor": 3}])
    entries = plan(9, 0, 3 * HOUR, peak)
    before = count([e for e in entries if e[0] <= HOUR])
    during = count([e for e in entries if HOUR < e[0] <= 2 * HOUR])
    after = count([e for e in entries if e[0] > 2 * HOUR])
    assert 2.6 < during / before < 3.4 and 0.85 < after / before < 1.15
    assert plan(9, 0, HOUR, peak) == plan(9, 0, HOUR, PROFILE)
