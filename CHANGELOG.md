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

- **Start from an example.** `project.example` (`POST /api/project/example`, MCP `project_example`) adds one small,
  real example to the project: a world on the built-in `retail` benchmark, a scenario (one customer ticket), an
  agent setup on the caller's newest working model key and a one-setup experiment ready to deploy. It works in a
  folder and in a hosted project, answers what it would write unless `confirm`, and is 409 when any of its ids is
  taken (nothing is overwritten). The content is data in `core/crates/shaked/example/`. Home offers it while the
  project has no worlds, then opens the experiment.
- Typed experiment deployments use design-row identity, keeping equal slugs in different projects separate.
- Hosted project menu and New project dialog when the layer advertises `projects`. Editors send
  loaded item/settings revisions so stale writes are refused with a reload message.

- Per-kind project storage through `Snapshot` and `ChangeSet`, with transactional row revision
  checks and history for typed homes. Default file homes remain unchanged. Project export/import
  operations use a base64 file map; project and item history operations expose recent changes.

- **Tables for a project's state (used by opt-in typed homes).** Migration shakedown 10 adds `projects`,
  `project_scenarios`, `project_setups`, `project_setup_files`, `project_experiments`, `project_changes`,
  `project_change_items` and `project_imports`, plus `experiments.project_experiment_id`. Each item's row keeps its
  file's contents as JSON, with a revision so a stale write is refused, and a delete only marks the row. The store
  reads and writes them (`store/project_items.rs`, `store/project_history.rs`). Projects still live in their folder.

- **Install a runtime from the app, and build a world from scratch.** **Build → Runtimes** installs the package that
  runs a world from a git URL (optionally one folder of the repo) or an uploaded `.zip`/`.tar.gz`: its Python
  dependencies with uv, its packages registered, and each package's conformance check (`shakedown-part-check`) shown.
  Each install lists its source, packages, versions, check and the worlds it runs, with Check again, Update and Remove
  (refused while a world uses it, naming the worlds). The app warns that a runtime is code that runs on this machine;
  a hosted shaked refuses installs (403), as it refuses Import from git. **New world** picks an installed runtime, the
  world's Runtime section can install one, and shaked starts with no benchmark when none is
  chosen, so a fresh Shakedown with nothing installed is built from the app alone. API `GET/POST /api/runtimes`,
  `POST /api/runtimes/{id}/check`, `DELETE /api/runtimes/{id}`; MCP `runtimes_*`.
- **Worlds run from their own data files.** The world's Data section uploads JSON, CSV, YAML or text files (10 MiB
  each) into its folder through the project store, views JSON records as tables where a value is edited in place,
  replaces and deletes them (`worlds.data_put`, `worlds.data_delete`). Files marked "Every run starts from this" reach
  every part of each run as `world_data` in its `start` config, and the benchmark in `gold_actions` and `score`
  (protocol: [World data](docs/protocol.md#world-data)); the data's digest names the base and gold saves and joins each
  part's recorded fingerprint, and forks keep the data their run started on. The kits read it:
  `shakedown_part::world_data` (Rust) and `shakedown_part.worlddata` (Python). A world without data runs as before.
  A hosted shaked refuses to deploy a world that has start data (409) until sandboxes can fetch it.

- **Get started can't be skipped.** Until a project, a model, a world, agents and a first scenario exist (read
  from shaked, never the browser), every page sends you to Get started; **Ready** opens the rest. Connections &
  models, the item editors the steps open and the account menu stay reachable meanwhile. "Skip for now" is gone.

- **Switch runs from the run page, and compare three or more.** A run of an experiment has a folded switcher under
  its header: hop to the previous or next run, or open it for every setup's runs and this run's reruns. Tick
  runs there and **Compare** opens `/compare?branches=a,b,c`: a column per run with the best of each row in
  bold, and where each run first leaves the first one, built from the pairwise compare (no API change).
- **Previous and next step buttons.** The step panel on the run page and the process view, and Run once's step
  drawer, have ‹ › buttons with where the step sits ("12 of 340"), the same moves as ←/→. Arrow keys no longer
  also move the step while a tab has focus.
- **The event log shows each step's whole text, and the step panel renders Markdown and can go full width.**
  The detail column comes last and wraps the full text: a tool call's arguments, a model call's reply (or its
  model and tokens), a message in full. In the step panel, messages, reports and other prose render as
  Markdown (JSON stays JSON), and **Full width** gives the step the whole page; **Side panel** or Esc puts it
  back. The process view's step panel has the same toggle.
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
- **Explore.** Analytics › Explore answers your own questions about runs. The builder takes measures (runs, pass
  rate, cost, input, output and cached tokens, model and tool calls, refusals, errors, run time, thinking time,
  steps) by up to two dimensions (agent, model, setup, scenario, world, experiment, tool, step kind, day), filtered
  by time, experiment, setup and agent, as a table and a bar, stacked or line chart; every value drills through to
  the runs or steps behind it. The SQL tab runs one read-only `SELECT` over documented views (`runs`, `steps`,
  `model_calls`, `tool_calls`, `agents`) behind a SQL parser's check, a read-only transaction with a 5 s timeout, a
  pinned search_path and a role that can read only those views, with a chart toggle and CSV download. Views of
  either kind can be saved by name. All of it is the `explore.*` operations, so the assistant and MCP get it too.
  Migration shakedown 9 adds `call_usage` (one lean row per model call, kept by the analytics job: no call body is
  ever read), `saved_views`, the `explore` views and the `shakedown_explore` role; branch figures move to version 3,
  so each ended branch is worked out once more after the upgrade. A hosting layer with row-level security gives the
  explore views an owner its policies cover, since a view reads its tables as its owner. See
  [docs/analytics.md](docs/analytics.md#explore).
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

### Changed

- **Less text on screen**: help sentences under fields, switches, empty states and errors show on hovering (or tapping) the label or title instead of always (`Hint`); cards keep a short data line and take explanation as `hint`.
- **Quieter shell, Home, Get started, Settings, Runtimes, Connect and Ask**: subtitles, helper lines and policy notes moved onto their titles and labels (hover or tap); the key promises on Get started show on hovering **Your keys stay yours**.
- **Quieter runs and experiments**: the run page, its step panel, Agent and Coordination tabs, Process, Compare, Run once, the runs list, AI review and experiment results show names, figures and results; chart captions, legends, how-to sentences and review reasons moved into hints on their titles.

- **A simpler web app.** The Analytics page (Overview, Search, Explore) is gone from the UI, with its sidebar item
  and the ⌘K palette's *Analytics* and *Search every run's steps*; `/analytics` opens Experiments. The figures,
  step search and Explore stay as the `analytics.*` and `explore.*` operations and their API, for the assistant
  and MCP ([docs/analytics.md](docs/analytics.md)).
- **The sidebar shows only the core:** Home, Experiments, Worlds, Scenarios, Agent setups, plus Get started and
  Settings. Settings has three tabs: Connections & models, Project settings and Advanced, which links Runtimes,
  Observability, API tokens and Connect an agent. Runtimes is also a world's Runtime section's *Manage runtimes*;
  Connect an agent is also a button on Agent setups. Every address still works.
- **Experiment pages lead with results.** An experiment file's page shows its runs first and folds its configuration
  (a one-line summary; open, the editor) instead of Overview, Runs and Settings tabs. An experiment's results page
  gains a folded Configuration under the checks: seed, world time window, spend cap and each setup's agents.

### Fixed

- **An HTTP agent that answers without its own figures is costed from its gateway calls.** Its tokens and cost
  used to show as zero and unknown ("tokens only") though every model call through its `gateway_url` was
  recorded with the provider's cost; those calls now count. A fork that moves back to an HTTP agent's start says
  so, instead of naming only Claude Code and Codex.

- **Switches are Radix's Switch** (`@radix-ui/react-switch` 1.4.1, beside the Radix dialog, menus, tabs and tooltips already used), styled in our colours, instead of a hand-made button.

- **Setup is the project and a model key, both done in Get started**: the key is added on the model step itself (no trip to Settings), and once both are done Shakedown opens. Worlds, agents and scenarios no longer hold the app shut or send you back and forth.

- **Get started moves on by itself**: it opens on the first step not done and goes to the next one when a step is done, rather than staying on "Your project is ready".
- **A key's name can't be refused for its letters**: the Name field turns what you type into lowercase letters, digits, `-` and `_` ("OpenRouter Key" becomes `openrouter-key`), and a new key is named after its kind until you name it.

- **Switches show their knob**: it sat off the right edge (the button centred it) and was dark on dark, so an on switch looked like a plain green pill.

- **Creating a hosted project with git answers 400, not 500, and Get started no longer offers git there.**
  A project kept in a database cannot be tracked with git; `POST /api/project` with `git: true` now says so
  with a 400. `GET /api/project`'s 409 (no project yet) carries `folder`, whether a project made here is a
  folder on disk, and **Track with git** shows only when it is.

- **←/→ and the step buttons stay on the selected agent's lane**: they go to that agent's previous or next step, not the next step by anyone. ↑/↓ still move through every step in order, and the buttons say whose steps they move through (‹ Planner · 4 of 37 ›).

- **Connector logos are in their brand colours** in the world catalog, not black (Simple Icons' brand colours).

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
