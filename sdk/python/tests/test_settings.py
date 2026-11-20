"""Settings: SHAKEDOWN_<NAME>, else the old ARENA_<NAME> with one warning per name."""

import warnings

import pytest

from shakedown.settings import both, old_name, setting


def test_the_new_name_wins_and_the_old_one_is_read_without_it():
    env = {"ARENA_T_SAVES": "/old"}
    with pytest.warns(FutureWarning, match="ARENA_T_SAVES is an old name"):
        assert setting("SHAKEDOWN_T_SAVES", environ=env) == "/old"
    env["SHAKEDOWN_T_SAVES"] = "/new"
    assert setting("SHAKEDOWN_T_SAVES", environ=env) == "/new"
    assert setting("SHAKEDOWN_T_NONE", "d", environ=env) == "d"
    assert old_name("SHAKEDOWN_X") == "ARENA_X" and old_name("PATH") == "PATH"


def test_an_old_name_is_warned_about_once():
    env = {"ARENA_T_ONCE": "1"}
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        setting("SHAKEDOWN_T_ONCE", environ=env)
        setting("SHAKEDOWN_T_ONCE", environ=env)
    said = [str(x.message) for x in w]
    assert said == ["ARENA_T_ONCE is an old name, read for now; set SHAKEDOWN_T_ONCE instead"]


def test_programs_get_both_names():
    assert both("SHAKEDOWN_SAVES", "/s") == {"SHAKEDOWN_SAVES": "/s", "ARENA_SAVES": "/s"}
