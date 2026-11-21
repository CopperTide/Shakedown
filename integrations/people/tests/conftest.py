import pytest


@pytest.fixture(autouse=True)
def saves_root(monkeypatch, tmp_path):
    root = tmp_path / "saves"
    monkeypatch.setenv("SHAKEDOWN_SAVES", str(root))
    return root
