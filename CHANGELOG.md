# Changelog

## Unreleased

### Breaking

- **Agents must send their token to MCP and the model gateway.** Each agent's endpoints (from
  `POST /api/runs`, a suite's claim, or registering an agent) carry a `token`. Every call to that
  agent's `/mcp/...` and `/gateway/...` URLs must send it as `Authorization: Bearer <token>`; a call
  without it is refused 401, with a body that says so. The Python SDK sends it from
  `shakedown-sdk` 0.2.0. Upgrade older SDKs, and add the header to HTTP agents and anything else
  that calls those URLs itself.
- **`/api` answers loopback names only on a local shaked.** A tunnel reaches the token-guarded MCP
  and gateway URLs, never the API. `SHAKEDOWN_PUBLIC_URL` now only shapes the URLs handed out, and
  a local shaked with `SHAKEDOWN_ALLOWED_HOSTS` set refuses to start.
- **An `Authorization` header that is not a Bearer token is refused** instead of being read as the
  person at this machine. The scheme matches in any case (`bearer`, `Bearer`).
- **Deleting a project item that something uses is refused** (409 with `dependents`) unless the
  delete is sent with `cascade: true`, which deletes those too (a setup instead leaves the
  experiments that compare others).

### Added

- Delete for worlds, scenarios, setups and experiments in the app, on each editor page and list
  row: the dialog lists what uses the item, then deletes it only if unused or with everything that
  uses it. `<kind>.dependents` previews the same over HTTP, MCP and the assistant.
- Deleted project files go to `.shakedown/trash/` and come back with Undo (`project.restore`); a
  cascade is planned and checked before anything changes, and refused if what uses the item changed
  since the preview (`expect`). The assistant's delete card shows the same choice as the dialog.
  Undo never overwrites a later edit, relinks a deleted experiment's runs, and is also under
  Project settings → Deleted files.
- World groups: an optional `group` in `world.yaml` (e.g. the customer). The Worlds page and the
  sidebar list worlds under their groups; set a group in the world's Overview, drag a world onto a
  group, or rename a group from its header (`worlds.rename_group`).
- The Worlds page opens each world onto its scenarios and their experiments; a world's page lists
  its scenarios, a scenario's page its experiments, and an experiment's page shows
  World › Scenario › Experiment.

- API tokens (`sd_`), capped by read, run or admin; made in Settings, in the API, or with
  `shaked token create`. The SDK reads one from `SHAKEDOWN_TOKEN`.
- A layer interface for embedding shaked (`docs/embedding.md`). Every way a run starts (a run, a
  suite, an experiment, a suite's claim) is admitted and metered through it.
