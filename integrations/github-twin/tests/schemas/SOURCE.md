# Source of `ghes-3.20-subset.json`

An extract of GitHub's REST API description for GitHub Enterprise Server 3.20, API version
2022-11-28, from [github/rest-api-description](https://github.com/github/rest-api-description)
(MIT licence, `LICENSE-rest-api-description.md` beside this file).

- URL: https://raw.githubusercontent.com/github/rest-api-description/c9c95fe34ced05fe076001f5c55704742bc8e7f8/descriptions/ghes-3.20/ghes-3.20.2022-11-28.json
- Commit: `c9c95fe34ced05fe076001f5c55704742bc8e7f8` (main, 2026-11-19)
- sha256 of the full description: `ed2a1642965d26dacf962b534dbdb30ebbc214ccac5d5b2c48e525c3023ffeb3`

The extract keeps every documented operation's method and path (`paths_ops`), the JSON answers of
the operations the twin covers (`github_twin/rest/covered.py`), keyed `METHOD path status`, and the
component schemas they use, as JSON Schema draft 7 (`nullable` becomes a null type; `oneOf` becomes
`anyOf`, see the script). Rebuild it after changing `COVERED`:

    curl -sSfLo /tmp/ghes.json <URL above>
    uv run python scripts/schema_subset.py /tmp/ghes.json <URL above> <commit> tests/schemas/ghes-3.20-subset.json
