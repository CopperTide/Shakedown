"""PagerDuty's API descriptions, which may not be committed (owner ruling, 2026-11-19; the repository
has no licence): the tests fetch them once into `tests/.vendor-cache/` (git-ignored) from a pinned
commit of PagerDuty/api-schema, check their sha256, and skip saying why when they cannot."""
from pathlib import Path

import pytest

from arena_part.twin.vendor_schema import Pinned, Unavailable, load

CACHE = Path(__file__).parent / ".vendor-cache"
COMMIT = "c928d7cf3c7c8b50030316149e04318b6b9300e1"
RAW = f"https://raw.githubusercontent.com/PagerDuty/api-schema/{COMMIT}/reference"

REST = Pinned("pagerduty-rest.json", (f"{RAW}/REST/openapiv3.json",),
              "788e9bf1e3575109d9e89d845650a56050082d8a63c83430b8218425032c2fa0")
EVENTS = Pinned("pagerduty-events-v2.json", (f"{RAW}/events-v2/openapiv3.json",),
                "f5570ec1049850501363447415a9202118586281286c3ccc8cd996f8429d1908")


def spec(pinned: Pinned) -> dict:
    try:
        return load(pinned, CACHE)
    except Unavailable as e:
        pytest.skip(f"PagerDuty's description is not available offline: {e}")
