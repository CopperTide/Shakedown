from platform_lib.errors import world_stamp


class FixedClock:
    def now(self) -> float:
        return 1_773_122_460.25


def test_events_are_stamped_with_world_time():
    before_send = world_stamp(FixedClock())
    event = before_send({"timestamp": "2031-01-01T00:00:00Z", "message": "boom"}, {})
    assert event == {"timestamp": 1_773_122_460.25, "message": "boom"}
