# Changelog

## Unreleased

### Breaking

- **`Layer::project_dir` is now `Layer::project_home`**, answering a `ProjectHome`: a folder as before, or a
  `ProjectDb` the layer keeps projects in (every file read into memory, each change saved in one transaction).
  `ProjectState::current` and `change` are async, and `shakedown_project::Project` reads and writes through a
  `Store` (`open_in`, `init_in`; `open` and `init` still take a folder). Local projects are unchanged.
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

- **A scenario's page lists its experiments in their own section.** Experiments (N) lists every experiment
  that plays the scenario, sub-experiments under their parent, each with a link and whether it has run.
- **Experiments show whether they ran.** The experiments list has a Status column: has results, running, ran
  with no results, or not run yet, read from each experiment's deployments. Filter by status, and sort with
  the ones that have results first or by the last deployment.
- **Analytics.** A new Analytics page (sidebar, under Experiments) shows figures across runs: runs, passes, pass
  rate, total cost, cost per passed run and median run time; pass rate over time per setup or model; cost per run
  against pass rate, one dot per setup or model; and a sortable setup × scenario leaderboard. Filters (7, 30 or 90
  days or all time, world, scenario, setup, model) stay in the address, and a dot or a row opens its runs (the runs
  list now takes `setup`, `scenario`, `model`, `from` and `to`). The same figures are `GET /api/analytics/summary`
  and the `analytics.summary` operation for the assistant and MCP. Each ended branch's figures are worked out once
  by a background job and kept in the new `branches.stats` column (migration 7), old runs included. See
  [docs/analytics.md](docs/analytics.md).
- **Search every run's steps.** Analytics › Search (or the ⌘K palette's *Search every run's steps for …*) finds
  any step across runs by its words: reasoning, messages, reports and delegations, tool calls' names, arguments,
  results and errors, checks. Results come newest first, grouped by run, each with a snippet of the match; Kind and
  Agent chips narrow them, and a click opens the run page at that step. The same search is
  `GET /api/analytics/search` and the `analytics.search` operation for the assistant and MCP. A full-text index on
  the events (migration shakedown 8, an expression GIN index: no table rewrite) keeps it fast. See
  [docs/analytics.md](docs/analytics.md#search).
- **Observability.** Settings → Observability sends every run as an OpenTelemetry trace (OTLP/HTTP protobuf) to
  Langfuse, Arize Phoenix or any OTLP collector: run, branch, agent turns, model calls (GenAI semantic conventions
  and OpenInference), tool calls with rows read and written, reasoning and the check; content only where the target
  takes it. Exports are queued, sent from the record once a branch ends, retried with backoff, and can be asked for
  (`POST /api/branches/{id}/export`, `runs.export`). `GET /metrics` serves Prometheus text; an OTLP target can also
  get OTLP metrics. An agent's own OTel SDK can send its spans to `/otlp/{branch}/{agent}/v1/traces` (each agent's
  endpoints carry `otlp_url` and `OTEL_EXPORTER_OTLP_*` in `env`); they are kept as a new `spans` event. Model
  calls now record `duration_ms`. Migration shakedown 6. See `docs/observability.md`.
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

- **Re-importing a repo removes what the repo no longer has.** A re-import kept every setup, experiment,
  scenario and world an earlier import wrote, even once the repo had dropped them. Now an item the last import
  wrote that the repo no longer has is removed (to the project's trash, so it can be restored) when it is
  unchanged since, nothing left in the project uses it and no other import holds it; one you changed, or still
  use, is kept and the plan says why. Items you made in the project, and runs, are never removed.
- **A rate limit (429) no longer ends a run in seconds.** The API-mode agent loop tried a 429 three times over
  about 7 s, too short for a per-minute limit such as OpenRouter's 20 requests a minute on a new account, so a
  team with two agents on one model ended `infra_error`. A 429 is now tried up to 6 times, each after the
  provider's `retry-after`, else the time until its limit resets (its `x-ratelimit-reset`, also read from
  OpenRouter's error body), else 1 s doubling, at most 30 s. The timeline marks each rate-limited try as
  `Model call rate-limited (429)`, and a call that still fails says `rate-limited by the model provider (7 tries
  over 61 s)` before the provider's message.
- **A fork at the step where an alert woke the lead now wakes it with the alert again.** On a stepped clock,
  forking a dispatch run at the lead's wake for a world's alert restarted the lead idle: the alert was not
  delivered again and the world ran to its horizon with no one deciding (forking one step later worked). The
  fork now delivers an alert the lead had not been woken with, as it delivers mail, so the lead acts on it with
  the fork's changes.
- **Compare shows only the rows the two runs have.** The outcome table read a fixed list from one
  world (valid plan, orders on time, transport cost), so other worlds' runs showed rows of "–".
  Its rows now come from the runs: checks passed, safety violations when a check is a safety one, a
  row per figure a check measured (Total cost, Late orders, ...), the twin's KPIs when there is a
  twin, agent cost, and escalations when an agent asked for approval.
- **Claude Code, Codex and the AI reviewer get an allowlisted environment.** Of shaked's
  environment they now get only `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `LANG`, `LC_*`, `TERM`,
  `TZ`, `TMPDIR`, `XDG_*`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, `NODE_EXTRA_CA_CERTS` and their own
  vendor's key. Before, a denylist let through the vault key, endpoint credentials
  (`SHAKEDOWN_AGENT_*`), AWS credentials, object-store secret keys, `GH_TOKEN`, `HF_TOKEN` and any
  other token, secret or password, and a parent `ANTHROPIC_AUTH_TOKEN` reached the gateway as a
  bearer token on routed runs. Proxy settings (`HTTPS_PROXY` and the like) no longer reach them
  either.
- **Shared saves of unrelated hubs are no longer made one at a time.** The lock that makes each
  base and gold save once is now per hub saves folder and name, so runs on different hubs stop
  queueing behind each other.
- **A shaked that runs packages only in sandboxes refuses a run or a fork before starting
  anything**, with 403 and the reason (a fork answered 500). The check now comes before the
  benchmark is described and the world planned (which started their package programs), and
  before the run or branch is recorded.
- **A sandbox's chat call that takes its run past the limit stops the branch even when the call
  can't be recorded.** The gateway used to answer 500 without stopping it, so the branch ran on
  until its next call.
- **Claude Sonnet 5.5's cache reads are listed at $0.10 per million tokens**, its list price,
  not $0.20.
- **Open agents' chat calls to Claude are priced at the cache rates.** The Claude adapter's
  canonical usage now keeps the cache-write and cache-read counts, so a cache hit is no longer
  charged (and metered) as fresh input, 10 to 40 times its price. The Bedrock adapter keeps them
  too, but `docs/prices.toml` lists no Bedrock prices yet, so Bedrock calls stay unpriced.
- **A dispatch run's open agents can name your stored keys (`key:<name>`).** Their sessions now
  resolve providers with the caller's keys, as other runs do, instead of the environment's only.
- **An open agent on Claude ends with a known cost.** Its end now sums its calls at the list price
  the gateway charges them (cache rates included), labelled `list_price_estimate`, instead of
  `unknown`. On a provider `docs/prices.toml` lists no prices for (Bedrock and OpenAI, for now)
  the cost stays `unknown`. A branch's metrics label such a cost `list-price estimate`, not
  `reported`.
