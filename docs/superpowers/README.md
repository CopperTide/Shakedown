# Plans, specs and research

The plans, specs, research notes and mockups in this folder are historical records: they are kept as they
were written and are not updated when the code changes.

On 2026-11-19 the service `shaked` was renamed `shaked` (crate `core/crates/shaked`), the Python SDK `shakedown`
(`shakedown-sdk`, class `Shakedown`) was renamed `shakedown` (class `Shakedown`), and the other `shakedown` names followed: the
settings `SHAKEDOWN_*` are `SHAKEDOWN_*`, the protocols `shakedown-part/1` and `shakedown-agent/1`, the manifests
`shakedown-package.toml`, the programs and Cargo packages `shakedown-<package>`, the kits `shakedown-part` and
`shakedown_part`, the proxy's headers `X-Shakedown-*`, the MCP server and dataset names, docker resources, and the
Postgres user and database `shakedown` (the SDK installs as `shakedown-sdk`). Records written before that date use the old names; the old names still work
([Old names](../../README.md#old-names)).
