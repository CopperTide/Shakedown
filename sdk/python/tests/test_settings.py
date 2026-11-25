"""Settings are read under their SHAKEDOWN_ names."""

from shakedown.settings import setting


def test_a_setting_is_read_from_the_environment_given():
    assert setting("SHAKEDOWN_X", environ={"SHAKEDOWN_X": "1"}) == "1"
    assert setting("SHAKEDOWN_X", "d", environ={}) == "d"
    assert setting("SHAKEDOWN_X", environ={}) is None
