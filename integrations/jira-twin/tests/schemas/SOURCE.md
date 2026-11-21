# Atlassian's API descriptions

The schema tests (`tests/test_schema.py`) check every covered operation's answer against Atlassian's
OpenAPI descriptions of Jira Cloud. They are not committed (owner ruling, 2026-11-19): the tests fetch
them on first use into `tests/.vendor-cache/` (git-ignored) and skip, saying why, when they cannot.
Atlassian serves them at unversioned URLs that change, so the tests fetch immutable Wayback Machine
copies (`id_` URLs give the original bytes) and check each against its pinned sha256
(`tests/vendor.py`):

| file | original URL | snapshot | sha256 |
|---|---|---|---|
| `jira-platform-v2.json` | https://developer.atlassian.com/cloud/jira/platform/swagger.v3.json | 2026-01-20 16:08:50 | `2f2340302124823749fb12ace44d282485dfec24c9c501a660a1367f5d1e6424` |
| `jira-platform-v3.json` | https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json | 2026-11-18 08:58:01 | `aa9f61ed0219f04406ab2c73c5a07a4aa48cb30bef01c54d79cd533a740f72fe` |
| `jira-software.json` | https://developer.atlassian.com/cloud/jira/software/swagger.v3.json | 2024-02-12 19:51:36 | `10049ef3cca318ee7fbb63e987581e15e71afde2d23137dc391e278af1b0ea94` |

Platform operations are checked twice, under `/rest/api/2` against the v2 description (rich text as
strings) and under `/rest/api/3` against v3 (rich text as ADF); agile operations against Jira
Software's. Jira Software shows its sprint answers only by an example, so those are checked against
its `SprintBean`. To move to newer copies, pick a snapshot on https://web.archive.org, put its stamp
and the sha256 of its (gunzipped) bytes in `tests/vendor.py`, and delete the cache.
