"""Paths into the software-factory template for its tests. The file tests read only the template's files (and,
where marked, Docker); test_template.py reads it through worldbench, as the builder does."""
from pathlib import Path

import pytest

TEMPLATE = Path(__file__).resolve().parents[2] / "templates" / "software-factory"
REPOS = ("storefront", "orders", "inventory")


@pytest.fixture(scope="session")
def sf_dir() -> Path:
    return TEMPLATE


@pytest.fixture(scope="session")
def platform() -> Path:
    return TEMPLATE / "platform"


def pytest_configure(config):
    config.addinivalue_line("markers", "live: needs services or hardware not always present")


def pytest_collection_modifyitems(config, items):
    """Live tests build images and start containers: they run only when asked for with `-m live`."""
    if "live" in (config.getoption("-m") or ""):
        return
    for item in items:
        if item.get_closest_marker("live"):
            item.add_marker(pytest.mark.skip(reason="a live test: run with -m live"))
