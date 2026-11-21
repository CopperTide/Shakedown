"""Atlassian's API descriptions, which may not be committed (owner ruling, 2026-11-19): the tests
fetch them once into `tests/.vendor-cache/` (git-ignored), from immutable Wayback Machine copies
pinned by sha256, and skip saying why when they cannot. See `tests/schemas/SOURCE.md`."""
from pathlib import Path

import pytest

from shakedown_part.twin.vendor_schema import Pinned, Unavailable, load

CACHE = Path(__file__).parent / ".vendor-cache"
WAYBACK = "https://web.archive.org/web/{stamp}id_/https://developer.atlassian.com/cloud/jira/{path}"

PLATFORM_V2 = Pinned("jira-platform-v2.json", (WAYBACK.format(stamp="20260120160850", path="platform/swagger.v3.json"),),
                     "2f2340302124823749fb12ace44d282485dfec24c9c501a660a1367f5d1e6424")
PLATFORM_V3 = Pinned("jira-platform-v3.json",
                     (WAYBACK.format(stamp="20261118085801", path="platform/swagger-v3.v3.json"),),
                     "aa9f61ed0219f04406ab2c73c5a07a4aa48cb30bef01c54d79cd533a740f72fe")
SOFTWARE = Pinned("jira-software.json", (WAYBACK.format(stamp="20240212195136", path="software/swagger.v3.json"),),
                  "10049ef3cca318ee7fbb63e987581e15e71afde2d23137dc391e278af1b0ea94")


def spec(pinned: Pinned) -> dict:
    try:
        return load(pinned, CACHE)
    except Unavailable as e:
        pytest.skip(f"Atlassian's description is not available offline: {e}")
