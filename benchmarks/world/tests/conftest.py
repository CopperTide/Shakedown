from pathlib import Path

import pytest

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "templates"


@pytest.fixture
def fixtures() -> Path:
    return FIXTURES
