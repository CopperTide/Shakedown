from pathlib import Path

import pytest

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "templates"


@pytest.fixture
def fixtures() -> Path:
    return FIXTURES


@pytest.fixture
def shop():
    from worldbench.template import load_template
    return load_template(FIXTURES / "shop")


@pytest.fixture
def shop_only_beta_tasks(tmp_path):
    from support import copy_with

    from worldbench.template import load_template
    return load_template(copy_with(FIXTURES / "shop", tmp_path, {}, drop=("tasks.0",)))


@pytest.fixture
def built_shop(shop, tmp_path):
    """A folder holding the default shop world's world.json and its copied files."""
    import shutil

    from worldbench.generate import generate, world_json
    r = generate(shop, {})
    out = tmp_path / "built"
    out.mkdir()
    (out / "world.json").write_text(world_json(r.world))
    for src, dst in r.files:
        shutil.copytree(src, out / dst)
    return out


def _builder(monkeypatch, present: bool):
    from worldbench import needs
    from worldbench.builder import Builder
    monkeypatch.setattr(needs, "image_present", lambda image: present)
    return Builder([FIXTURES])


@pytest.fixture
def builder(monkeypatch):
    """A builder over the fixture templates on a machine that has every image."""
    return _builder(monkeypatch, True)


@pytest.fixture
def builder_without_docker(monkeypatch):
    return _builder(monkeypatch, False)
