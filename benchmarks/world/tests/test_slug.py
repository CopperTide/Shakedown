import re

from worldbench.slug import key_of, slug_of


def test_slug_of():   # Review Focus 2
    assert slug_of("Northwind Labs") == "northwind-labs"
    assert slug_of("Ærø Ünlimited!") == "aero-unlimited"
    assert slug_of("株式会社").startswith("world-") and len(slug_of("株式会社")) == 12
    assert len(slug_of("A" * 30 + " " + "B" * 30)) <= 40
    assert key_of("Northwind Labs") == "NL" and key_of("Acme") == "AC"


def test_slugs_of_odd_names_are_valid_package_names():
    for name in ["!!!", "  ", "-a-", "A" * 60, "Straße & Söhne", "Ærø Ünlimited", "x" * 39 + " yy"]:
        assert re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?", slug_of(name)), name
    assert slug_of("A" * 30 + " " + "B" * 30) == "a" * 30
    assert slug_of("!!!") != slug_of("???")
    assert slug_of("Straße & Söhne") == "strasse-sohne"


def test_keys_are_two_to_four_capitals():
    assert key_of("One Two Three Four Five") == "OTTF"
    assert key_of("Ærø Ünlimited") == "AU"
    assert key_of("株式会社") == "WD" and key_of("X") == "XX"
