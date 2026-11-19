"""Both releases of the internal pricing-lib the wheelhouse builds: 1.4 rounds to cents, 2.0 to the currency's minor
unit and needs the currency."""
import importlib.util
from decimal import Decimal
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parents[1]


def release(version: str):
    path = HERE / f"pricing-lib-{version}" / "pricing_lib" / "__init__.py"
    spec = importlib.util.spec_from_file_location(f"pricing_lib_{version.replace('.', '_')}", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_1_4_rounds_half_up_to_cents():
    lib = release("1.4.0")
    assert lib.__version__ == "1.4.0"
    assert lib.round_price(Decimal("19.995")) == Decimal("20.00")
    assert lib.round_price(12.344) == Decimal("12.34")
    assert lib.apply_discount(Decimal("80.00"), 15) == Decimal("68.00")


def test_2_0_rounds_to_the_currency_and_needs_it():
    lib = release("2.0.0")
    assert lib.__version__ == "2.0.0"
    assert lib.round_price(Decimal("19.995"), "USD") == Decimal("20.00")
    assert lib.round_price(Decimal("1999.5"), "JPY") == Decimal("2000")
    assert lib.apply_discount(Decimal("80.00"), 15, "EUR") == Decimal("68.00")
    with pytest.raises(TypeError):
        lib.round_price(Decimal("19.99"))
    with pytest.raises(ValueError, match="unknown currency XXX"):
        lib.round_price(Decimal("1"), "XXX")


def test_the_versions_the_packages_declare_match_their_folders():
    for version in ("1.4.0", "2.0.0"):
        assert f'version = "{version}"' in (HERE / f"pricing-lib-{version}" / "pyproject.toml").read_text()
