"""Settings: SHAKEDOWN_<NAME>, else the old ARENA_<NAME> with one warning per name."""

import warnings

import pytest

from shakedown_part.settings import both, old_name, setting


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


def test_the_old_import_name_arena_part_is_shakedown_part_and_warns_once():
    import subprocess
    import sys

    code = """
import warnings
warnings.simplefilter("always")
with warnings.catch_warnings(record=True) as w:
    import arena_part, arena_part
    from arena_part import Part, serve
    from arena_part.settings import setting
    from arena_part.twin import HttpTwin
import shakedown_part, shakedown_part.settings, shakedown_part.twin
assert Part is shakedown_part.Part and serve is shakedown_part.serve
assert setting is shakedown_part.settings.setting and HttpTwin is shakedown_part.twin.HttpTwin
print(len([x for x in w if "shakedown_part" in str(x.message)]))
"""
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    assert out.stdout.strip() == "1"
