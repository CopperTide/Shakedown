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

- **Product analytics in hosted Shakedown.** Served under `/sd/` by CopperTide's gateway with a PostHog key,
  the UI starts PostHog: the person by id, their org as a group, masked autocapture and session replay,
  page addresses as route templates, and named steps of the core flow (never names or content). A local
  shaked never loads PostHog, nor does a browser with Global Privacy Control or Do Not Track on.
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

### Fixed

- **Claude Code and Codex no longer inherit cloud credentials or Claude tokens.** Their
  environment now loses every AWS credential (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
  `AWS_SESSION_TOKEN`, `AWS_BEARER_TOKEN_BEDROCK` and the like), object-store secret keys, and any
  variable ending in `_AUTH_TOKEN` or `_OAUTH_TOKEN` (`ANTHROPIC_AUTH_TOKEN`,
  `CLAUDE_CODE_OAUTH_TOKEN`), so a parent `ANTHROPIC_AUTH_TOKEN` no longer reaches the gateway as a
  bearer token on routed runs either.
- **Shared saves of unrelated hubs are no longer made one at a time.** The lock that makes each
  base and gold save once is now per hub saves folder and name, so runs on different hubs stop
  queueing behind each other.
- **A shaked that runs packages only in sandboxes refuses a fork before starting anything.**
  The check now comes before the fork's world is planned (which started its package programs),
  as it already did for new runs, and before the new branch is recorded.
- **A sandbox's chat call that takes its run past the limit stops the branch even when the call
  can't be recorded.** The gateway used to answer 500 without stopping it, so the branch ran on
  until its next call.
- **Open agents' chat calls to Claude and Bedrock are priced at the cache rates.** Their canonical
  usage now keeps the cache-write and cache-read counts, so a cache hit is no longer charged (and
  metered) as fresh input, about 10 times its price.
