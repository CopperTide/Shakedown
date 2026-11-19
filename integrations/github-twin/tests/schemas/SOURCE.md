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

# Source of `ghes-3.20-graphql-subset.graphql`

GitHub's own definitions (descriptions and directives other than `@deprecated` and `@oneOf` removed) of every
type the twin's GraphQL schema (`github_twin/graphql/schema.graphql`) has, from GitHub's published GraphQL schema
for GitHub Enterprise Server 3.20 in [github/docs](https://github.com/github/docs), `src/graphql/data/`, which is
code under the MIT licence (`LICENSE-github-docs-code.txt` beside this file; the repository's `README.md`, "License").

- URL: https://raw.githubusercontent.com/github/docs/1d0f4e29e7d9e9d8619ab24898e54fe47c1a4005/src/graphql/data/ghes-3.20/schema.docs-enterprise.graphql
  (the same file docs.github.com serves at `/public/ghes-3.20/schema.docs-enterprise.graphql`)
- Commit: `1d0f4e29e7d9e9d8619ab24898e54fe47c1a4005` (main, 2026-11-19; the file last changed in `2ef9609`)
- sha256 of the full SDL: `decea572d89c706e91f0a8584c451b45638e5f50ce9b8df96516b0e335111ec3`

`scripts/prune_sdl.py` writes both the twin's schema and this extract from the full SDL:

    curl -sSfLo /tmp/ghes.graphql <URL above>
    uv run python scripts/prune_sdl.py /tmp/ghes.graphql github_twin/graphql/schema.graphql tests/schemas/ghes-3.20-graphql-subset.graphql

`tests/gh_queries/` holds the queries the tests replay: `*.graphql` are gh's five GHES feature probes, copied from
cli/cli `acee710547dcf7dfb6490afea77972187e74b819` `internal/featuredetection/feature_detection.go`; `gh-2.45.0/`
are the queries gh 2.45.0 sent the twin in `tests/test_clients.py`'s flows and more commands (`pr
create/view/list/checks/status/review/comment/edit/ready/close/reopen/merge`, `issue create/list/view/comment/edit/close/reopen/status`,
`label list`). gh is MIT (`tests/gh_queries/LICENSE-cli.txt`).
