# PagerDuty's API descriptions

The schema tests (`tests/test_schema.py`) check every covered operation's answer against PagerDuty's
OpenAPI descriptions from [PagerDuty/api-schema](https://github.com/PagerDuty/api-schema). The
repository has no licence, so they are not committed (owner ruling, 2026-11-19): the tests fetch them
on first use into `tests/.vendor-cache/` (git-ignored) from a pinned commit, check the sha256
(`tests/vendor.py`), and skip, saying why, when they cannot.

| file | source (commit `c928d7cf3c7c8b50030316149e04318b6b9300e1`, 2026-11-17) | sha256 |
|---|---|---|
| `pagerduty-rest.json` | `reference/REST/openapiv3.json` (REST API v2) | `788e9bf1e3575109d9e89d845650a56050082d8a63c83430b8218425032c2fa0` |
| `pagerduty-events-v2.json` | `reference/events-v2/openapiv3.json` (Events API v2) | `f5570ec1049850501363447415a9202118586281286c3ccc8cd996f8429d1908` |

To move to a newer commit, put it and the files' sha256 in `tests/vendor.py` and delete the cache.
