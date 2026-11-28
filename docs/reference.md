# Reference

shaked's HTTP API, its settings, what uses disk, and how to run the tests. For the ideas behind them see
[concepts.md](concepts.md).

## HTTP API

All JSON on `http://127.0.0.1:4510`, except the event stream. Errors are `{error}` with a status code.

| Route | What it does |
|---|---|
| `GET /api/health` | `{open_available, providers, claude, codex, sandboxes, benchmark, disk: {bytes, limit_bytes}}`. `claude` and `codex` say whether each CLI is on this machine's PATH; `sandboxes` is true when packages and agents run only in sandboxes (hosted), where the CLI agents run in the sandbox instead. `providers` is each model provider as the configuration alone tells it, without anyone's own keys (the endpoint is public); in the shape of `/api/providers` without `models`; a local server is not asked, so it is `available: false, checked: false` with why `not checked (GET /api/providers asks the server)`. `open_available` is true when some provider can take calls, judged from the configuration (a configured local server counts, up or not) |
| `GET /api/providers` | `[{id, kind, label, available, checked, why?, models?}]`, one per model provider, your own keys applied; see [Model providers](#model-providers) |
| `GET /api/me` | `{mode, user: {name}, features}`: `mode` is `local` and `user.name` `You` for an open-source shaked; a deployment's layer may add `account` (the UI's account menu) |
| `GET /api/project` | `project.status`: `{name, counts: {worlds, scenarios, setups, experiments}, problems, root, imports}` for the project folder `SHAKEDOWN_PROJECT` names; 409 when none is open, with `folder`: true when a project made here is a folder on disk (git can track it), false when there is no folder or projects are kept in a database (hosted). `problems` lists unreadable files and broken references (`{kind, id, pointer, message}`); a problem never stops the rest of the project from loading. `imports` lists the project's git imports, `[{id, url, ref, commit, imported_at, packages, items}]` |
| `POST /api/project` | `project.init`: `{name, git?}` creates a project in the configured folder; 409 when one is already there; 400 when `git` is asked for and the project is not a folder on disk (`folder` false above) |
| `POST /api/project/import-git` | `project.import_git`: `{url, ref?, id?, confirm?}` imports a git repo, its packages and its project items (see [Import from git](#import-from-git)). Without `confirm` the ref is fetched and the plan answered, and nothing is installed, registered or written: `{proposed: true, id, url, ref, commit, checkout, project: {name, problems}, packages: [{name, kind, dir, python}], items: [{kind, id, label, action: write\|replace\|same\|keep\|remove, reason}], views: [{world, file}], compose: [{file, services, volumes, part}], python: [{root, env}], edits, notes, problems, conflicts, previous_commit}`. With `confirm` the same, plus `registered` (true when shaked runs), `written` and `removed` (the last import's items the repo no longer has, removed to the trash). 400 for a URL other than https, ssh, `git@host:path`, `file://` or an absolute path, or a ref starting with `-`; 409 when a package name is installed already; 422 when git or uv fails; 403 on a shaked that serves others |
| `GET /api/runtimes` | `runtimes.list`: `{installs: [{id, source: git\|archive, url, ref, path, commit, installed_at, runtime, items, packages: [{name, kind, version, dir, used_by, registered, check}]}], builtin: [{name, kind, version, dir, used_by}], hosted}`. `runtime` is false for an Import from git (which also wrote project files); `check` is the package's last conformance check at this commit, `{package, passed, steps: [{name, ok, detail}], skipped, checked_at}`, or null; `used_by` the project's worlds whose runtime it is; `builtin` the packages in `SHAKEDOWN_PACKAGES` folders (see [Runtimes](#runtimes)) |
| `POST /api/runtimes` | `runtimes.install`: `{url, ref?, path?, id?, confirm?}` or `{archive: {name, data}, path?, id?, confirm?}` (`data` base64, a `.zip`, `.tar.gz` or `.tgz`; the body may be up to 256 MiB). Without `confirm` the plan, as `project.import_git` answers it, with `source` and `packages_only` (the folder); with it, installed and registered, plus `checks` (each package's conformance check). An install's `id` updates it. 400 for a bad URL, folder or archive, or both or neither of `url` and `archive`; 409 for a taken package name, or an `id` imported with its project files; 422 when git or uv fails; 403 on a shaked that serves others |
| `POST /api/runtimes/{id}/check` | `runtimes.check`: runs the install's conformance checks again: `{id, checks}`. 404 for no such install; 403 on a shaked that serves others |
| `DELETE /api/runtimes/{id}` | `runtimes.delete`: unregisters the install's packages and deletes its checkout and environments: `{removed, packages}`. 409 while a world's runtime is one of its packages (naming the worlds), or for an Import from git; 404 for no such install; 403 on a shaked that serves others |
| `GET /api/project/{worlds\|scenarios\|setups\|experiments}` | `{items: [{kind, id, label, hash, problems, …}]}`; each item also says where it belongs: a world its `group` (null when none), a scenario its `world`, an experiment its `world`, `scenario`, `parent` and how many `setups` it compares |
| `GET /api/project/{kind}/{id}` | one item with its `body` (the file's contents as JSON) |
| `PUT /api/project/{kind}/{id}` | `{body, confirm?}` creates or replaces the item's file. `body` is checked exactly like the file: unknown fields, a wrong `format:` and credentials written inline are 422 with a JSON pointer (a credential's value is never echoed). Unless `confirm` is true and while the project's `permissions.edit` is `diff_first` (the default), nothing is written and the answer is `{proposed: true, diff}`. Writes are atomic: a file is never left half-written. Needs the `admin` token scope |
| `DELETE /api/project/{kind}/{id}` | `{cascade?, expect?, confirm?}` (`cascade` and `confirm` also as `?cascade=true`) moves the item's file to the trash (a world's data folder and a setup's folder stay) and answers `{deleted, with, trash, runs_unlinked}`. While anything uses the item (see [concepts](concepts.md#project-files)) it is refused with 409 `{error, dependents}`. With `cascade: true` what uses it is deleted or rewritten first (`with` lists each with its `fates`); `expect`, the `dependents` a preview showed, makes it 409 with the fresh `dependents` if they changed; under `diff_first` an unconfirmed cascade answers `{proposed: true, dependents}`. A deleted experiment's deployments are unlinked and list as earlier experiments |
| `GET /api/project/trash` | `project.deleted`: the most recent delete that can be undone, `{trash, label, files}`; 404 when there is none |
| `POST /api/project/trash/restore` | `project.restore`: `{trash?, confirm?}` puts back the delete named `trash` (the most recent when missing): every file it removed or rewrote, and links its experiments' deployments again; answers `{restored, files, trash, runs_relinked}`. Under `diff_first`, unconfirmed, it answers `{proposed, diff, trash}`: send that `trash` back to restore the same delete. 404 when there is nothing to undo; 409 when a removed file's path is taken again or a rewritten file was edited since |
| `GET /api/project/history` | `project.history`: `{limit?}` (1–100, default 50), returning `{changes}` newest first with id, label, created_at, undone_at and before/after items. Legacy file homes return an empty list; their deletes remain available through trash |
| `GET /api/project/{kind}/{id}/history` | `<kind>.history`, where kind is worlds, scenarios, setups or experiments: recent project changes touching that item |
| `GET /api/project/export` | `project.export`: `{files, encoding: "base64"}`; files maps relative paths to base64 bytes. Canonical item YAML, world data preserved, Git metadata and internal trash excluded |
| `POST /api/project/import-files` | `project.import_files`: `{files, confirm?}` with the same base64 file map. Previews unless confirmed; returns proposed, items, problems and skipped paths. Merges supplied files and leaves omitted items intact |
| `GET /api/project/{kind}/{id}/dependents` | `<kind>.dependents`: `{dependents: [{kind, id, label, fate, fates}], kept}`, what uses the item and what a cascading delete does to each (`fates`: `[deleted]`, or `setup_dropped` and/or `parent_cleared`; `fate` is the first), and the folder that stays (`kept`: a world's data folder, a setup's own folder, or null) |
| `POST /api/project/world-groups/rename` | `worlds.rename_group`: `{from, to?, confirm?}` moves every world in group `from` to `to` (empty or missing: no group; an existing group merges), checking every file before writing any, with a diff first under `diff_first` as for a put; answers `{moved, diff}`. 404 when no world is in `from` |
| `PUT /api/project` | `project.update`: `{body, confirm?}` replaces `shakedown.yaml` (name, defaults, permissions, assistant), with a diff first under `diff_first` as for an item |
| `POST /api/project/import` | `project.import`: `{confirm?}`. Without `confirm`, `{proposed: true, files, skipped}`: what moving installed benchmarks, built worlds and earlier experiments into the project would write (nothing existing is overwritten). With it, writes them and links each earlier experiment as its file's first deployment |
| `GET /api/project/worlds/{id}/data` | `worlds.data`: `?path=` reads one file of the world's folder, `{path, size, text, truncated}` (text up to 1 MiB, or 10 MiB with `full=true`) or `{path, size, binary: true}`; without `path`, `{files: [{path, size}], truncated}` (at most 500). 400 for a path outside the folder |
| `POST /api/project/worlds/{id}/data` | `worlds.data_put`: `{path, data}` (base64), `{path, text}`, or `{path, pointer, value}` (one value of a JSON file set at a JSON pointer) writes a data file into the world's folder through the project store: `{path, size, replaced}`. JSON, CSV, TSV, YAML, text or Markdown, at most 10 MiB (413 above), checked to parse; 400 for a path outside the folder, `world.yaml` or a view file, another kind of file, or a value with nothing at its pointer. The body may be up to 256 MiB |
| `DELETE /api/project/worlds/{id}/data` | `worlds.data_delete`: `?path=` deletes a data file: `{deleted}`; 404 when there is none |
| `POST /api/project/experiments/{id}/deploy` | `experiments.deploy`: runs the file experiment on its world's runtime package (see [concepts](concepts.md#project-files)), its parts given the world's start data ([protocol](protocol.md#world-data)); answers the engine experiment's detail. 409 on a shaked that serves others when the world has start data; 422 when a marked data file is missing, the world has no `package`, the scenario no `preset`, an item it names has a problem, or a setup is of kind `code` or `madl`; 400 when a setup's `key` names no stored key (or another member's key that names its own server), or its `auth` names a key that is not your own custom key, is empty, names both `key` and `env`, or names an environment variable that may not be sent (`SHAKEDOWN_AGENT_*` on a local engine only). A built-in agent's `key` becomes its provider `key:<name>`; an `http` setup's `auth` becomes a reference read from your keys on each request and sent as `Authorization: Bearer` |
| `GET /api/project/experiments/{id}/runs` | `experiments.runs`: `{deployments}`, the experiment's deployments newest first, each as `GET /api/experiments/{id}` answers |
| `GET /api/project/setups/{id}/madl` | `setups.madl`: a MADL setup's `{valid, diagnostics: [{code, severity, pointer, message}], graph: {agents, edges}}` (patterns expanded) |
| `POST /api/assistant/chat` | the Ask drawer: `{messages, page?}` answered by the assistant on the project's model (at most 8 model turns): reads run at once; every edit that takes `confirm` comes back as a diff card under `diff_first` (written at once under `apply_then_tell`); deploys, other edits and deletes come back as cards the person confirms. A delete card drops the model's `cascade` and shows the delete dialog's choice: alone if unused, or with what uses it |
| `POST /mcp` | the control MCP server (JSON-RPC, protocol `2025-06-18`): every operation above as a tool (`worlds_list`, `scenarios_put`, …), each checked against the caller's token like its route. See [Use Shakedown from an agent](#use-shakedown-from-an-agent-mcp) |
| `GET /api/tokens` | the live API tokens, `[{id, name, prefix, scopes, created_at, last_used_at, expires_at}]`: never a token or its hash |
| `POST /api/tokens` | `{name, scopes: ["read"\|"run"\|"admin"], expires_days?}`: `{id, token, prefix}`, the token (`sd_…`) shown this once. `read` views; `run` also starts, forks, stops and records; `admin` does everything, model keys and tokens included |
| `DELETE /api/tokens/{id}` | revokes a token; calls with it then get 401 |
| `GET /api/models/keys` | your model keys and servers, `[{id, name, kind, base_url, model, key_last4, checked_ok, checked_at, check_detail, created_at}]`: never the key |
| `POST /api/models/keys` | `{name, kind, secret, base_url?, model?}`: adds one (`kind` `anthropic`, `openai`, `openrouter`, `bedrock` or `custom`). A `custom` server needs `base_url` and is picked by agents as `custom:<name>`; Bedrock's `secret` is JSON `{access_key_id, secret_access_key, session_token?}` and `base_url` its region. 409 when the name is taken |
| `POST /api/models/keys/{id}/secret` | `{secret}`: replaces a key's secret |
| `POST /api/models/keys/{id}/test` | a one-token call with the key; the key with `checked_ok` and `check_detail` (the provider's reason, never the key) |
| `DELETE /api/models/keys/{id}` | deletes a key |
| `GET /api/observability/targets` | where runs are exported, `[{id, name, kind, endpoint, project, header_names, include_content, auto_export, export_metrics, checked_ok, checked_at, check_detail, created_at}]`: header names, never their values. See [observability.md](observability.md) |
| `POST /api/observability/targets` | adds a target (`admin` token scope). `kind` `langfuse` (`region` `us`, `eu` or `self` with `base_url`; `public_key`, `secret_key`), `phoenix` (`base_url`, default `http://127.0.0.1:6006`; `api_key?`; `project?`, default `shakedown`) or `otlp` (`endpoint`, `/v1/traces` added unless it ends with it; `headers?`, at most 20). `include_content` defaults to true for a target on your own server and false for Langfuse Cloud and Phoenix Cloud; `auto_export` defaults to true; `export_metrics` (OTLP metrics, `otlp` only) to false. 400 when the preset lacks a field or the endpoint may not be reached from here; 409 when the name is taken |
| `POST /api/observability/targets/{id}` | `{include_content?, auto_export?, export_metrics?}`: changes what the target gets |
| `POST /api/observability/targets/{id}/test` | sends one span (its own trace); the target with `checked_ok` and `check_detail` (what the collector answered, header values blanked) |
| `DELETE /api/observability/targets/{id}` | deletes a target and its export rows |
| `GET /api/observability/exports?branch=&limit=` | the latest exports (of one branch), `[{target_id, target, branch_id, state, attempts, spans, last_error, exported_at, next_at}]`; `state` `pending`, `done` or `failed` (after 6 tries) |
| `POST /api/branches/{id}/export` | `{target_id?}`: sends the branch to that target (every target when absent) now, from its record; a branch that has not ended is queued for when it does. `{branch_id, exports: [{target_id, target, state, spans?, error?}]}` (`state` `done`, `failed` or `queued`); 409 when there is no target |
| `POST /api/runs/export` | `runs.export`: `{branch_id, target_id?}`, the same as `POST /api/branches/{id}/export`, for the assistant and agents over MCP |
| `GET /api/analytics/summary?from=&to=&world=&scenario=&setup=&model=&status=&group=` | `analytics.summary`: figures across runs, each run's first branch once (reruns left out), as the caller sees them. `from`/`to` are RFC 3339; each list is comma-separated or a JSON array (`setup=["Team (lead, helpers)"]`); `group` is `setup` (default) or `model`. Answers `{group, bucket, totals: {runs, judged, passed, pass_rate, cost_usd, cost_per_run, cost_per_pass, median_seconds, tokens, uncosted}, pending: {unprocessed, running}, series: [{t, group, judged, passed, pass_rate}], points: [{group, runs, judged, passed, pass_rate, cost_usd, cost_per_run, tokens}], leaderboard: [{setup, world, scenario, runs, judged, passed, pass_rate, cost_per_run, median_seconds, cost_per_pass}], options: {world, scenario, setup, model, status}}`; `bucket` is `day` up to 90 days, else `week` (UTC). 400 for a bad time or group. See [analytics.md](analytics.md) |
| `GET /api/analytics/search?q=&kind=&agent=&run=&experiment=&limit=&offset=` | `analytics.search`: steps of every run the caller sees whose text matches `q` (web-search syntax: words match their other forms, `"phrase"`, `or`, `-word`), newest first. Searched: reasoning, messages, reports, delegations, hand-offs, wakes, turn ends, finishes, agent errors, checks, and tool calls' name, arguments, result and error; not model calls or world events. `kind` (`reasoning`, `message`, `tool_call`, `delegation`, `report`, `handoff`, `wake`, `idle`, `finish`, `agent_end`, `check`), `agent`, `run` and `experiment` are lists (comma-separated or a JSON array). `limit` 1–200 (default 50), `offset` up to 10,000. Answers `{hits: [{run_id, run_name, branch_id, seq, kind, agent, at, snippet}], total, total_capped, next_offset, kinds: [{name, count}], agents: [{name, count}]}`; each match in `snippet` sits between U+E000 and U+E001; `total` counts up to 10,000 (`total_capped` beyond); `kinds` and `agents` count matches whatever kinds and agents are picked. 400 without `q`, for an unknown kind or a bad page. See [analytics.md](analytics.md#search) |
| `GET /api/explore/schema` | `explore.schema`: what Explore offers. Answers `{measures: [{id, label, unit, level, description}], dimensions: [{id, label, level, description}], options: {experiments: [{id, name}], setups, agents}, sql: {schema, views: [{name, description, columns: [{name, type, description}]}], functions, limits: {rows, bytes, timeout_seconds, columns}, examples: [{name, sql}]}}`. See [analytics.md](analytics.md#explore) |
| `POST /api/explore/query` | `explore.query`: measures by dimensions over each run's first branch. Body `{measures: [...] (1–8), dimensions: [...] (0–2), filters: {from, to, experiment: [], setup: [], agent: []}, limit}` (measures `runs`, `pass_rate`, `cost`, `tokens_in`, `tokens_out`, `tokens_cached`, `model_calls`, `tool_calls`, `refusals`, `errors`, `duration`, `thinking`, `steps`; dimensions `agent`, `model`, `setup`, `scenario`, `world`, `experiment`, `tool`, `kind`, `day`; `limit` default 500, at most 1,000). Answers `{columns: [{id, label, role, unit}], rows: [[...]], labels: {experiment: {id: name}}, truncated, ms}`, dimensions first; a null dimension value is none. 400 for an unknown name, too many picks, a bad time or a query over 10 s |
| `POST /api/explore/drill` | `explore.drill`: the runs or steps behind one cell. Body `{spec, cell: {<dimension>: value or null}, measure}`. Answers `{kind: "runs", runs: [{run_id, branch_id, name, setup, scenario, world, status, at, seconds, cost_usd}], capped}` or, for a step-level measure or dimension, `{kind: "steps", steps: [{run_id, branch_id, run_name, seq, at, agent, kind, tool, outcome, model}], capped}`; at most 200, newest first |
| `POST /api/explore/sql` | `explore.sql`: one read-only `SELECT` over the views `runs`, `steps`, `model_calls`, `tool_calls`, `agents` (schema `explore`). Body `{sql}`. Answers `{sql (as it ran), columns: [{name, type, kind}], rows: [[...]], truncated: "rows" or "bytes" or null, ms}`. Read-only transaction, 5 s timeout, at most 5,000 rows. 400 for a query the guard refuses (anything but one SELECT on the views, a function off the list), a database error or a timeout |
| `GET /api/explore/views`, `GET /api/explore/views/{id}` | `explore.list`, `explore.get`: saved Explore views `{id, name, kind, spec, created_at, updated_at}`, last changed first (`{items}`) |
| `POST /api/explore/views` | `explore.save`: body `{id?, name (1–120 characters), kind: "builder" or "sql", spec}`; a builder spec must resolve and an SQL spec's `sql` pass the guard (400 otherwise). With `id`, replaces that view (404 if gone) |
| `DELETE /api/explore/views/{id}` | `explore.delete`: deletes a saved view (404 if none) |
| `GET /metrics` | the runs as Prometheus text (format 0.0.4): branches by status, pass rate, run time, model calls, tokens, cost and call latency by world, setup, model and provider. Authenticated like `/api`: the person at this machine, or an API token (`read` scope is enough) |
| `GET /api/tasks` | the benchmark's tasks, `[{id, reason, group?, ticket, instructions, clock_start?}]` (`group`: the group the benchmark lists the task under, such as a split; `instructions` is the benchmark's, the same for every task; `clock_start`, the world's date and time at clock 0 when its world keeps its own calendar: agents are told the world's date, see [protocol](protocol.md#a-tasks-clock)) |
| `GET /api/benchmark[?name=<package>\|?world=<world id>]` | `{name, version, tasks: [{id, reason, group?}], scenario_options, view}` of the current benchmark, of the installed benchmark `name`, or of the package the project world `world` names in `package:` (an import made while shaked runs is registered first, as for a deploy); the scenario editor's task picker reads it. 404 with why for a `name` that is not an installed benchmark, a world the project lacks or one without a package; 409 for `world` with no project open. `scenario_options` is the benchmark's `describe.scenario_options` (null when it offers none), the disruptions a run's scenario may add ([protocol](protocol.md#a-runs-scenario)); `view` is true when the benchmark draws the world's view |
| `GET /api/templates` | the world builder's templates, `[{id, name, summary, version, systems: [label]}]` ([protocol](protocol.md#builder-calls)); 404 `no world builder is installed` when no package of `kind = "builder"` is (with several, the first by name answers). Where packages run only in sandboxes (hosted), this, the form, the preview and `POST /api/worlds` answer 409 `hosted Shakedown ships prebuilt worlds; build custom worlds in local Shakedown` |
| `GET /api/templates/{id}` | the template's form (its choices and defaults); 404 for an unknown template |
| `POST /api/templates/{id}/preview` | body: the choices; what the world would be: name, title, parts, hosts, people, events, tasks, what is dropped, the images and memory it needs, and its `problems` |
| `POST /api/worlds` | `{template, choices}`: builds a world. 202 `{name, state: "preparing"}`: it is written to `SHAKEDOWN_WORLDS/<name>` (generated into a hidden temporary folder first), installed, and its base saves are made in the background, after which it is the current benchmark unless another was chosen since the build was asked for (that choice stands); 422 `{problems}` when the choices have problems (nothing is written); 409 `{error, suggested}` when a world or package has that name (`suggested` is the first free `<name>-N`) |
| `GET /api/worlds` | the benchmarks a user can choose between (those not opt-in, every built world and the current one): `[{name, title, template: {id, name} \| null, built, current, state}]`, `state` being `preparing`, `ready` or `failed` for a world prepared since shaked started, else `ready` when its base saves exist and `not_prepared` when they don't |
| `GET /api/worlds/{name}` | that row plus `elapsed_ms` (how long its preparation took, or has taken) and `error`; 404 for an unknown benchmark |
| `POST /api/worlds/{name}/use` | makes it the benchmark of new runs (runs, suites and experiments made before keep theirs), remembered across a restart in `SHAKEDOWN_WORLDS/.current`; answers its row; 404 for an unknown benchmark |
| `POST /api/worlds/{name}/prepare` | makes its base saves again (after a failure) and then makes it current (unless another was chosen since): 202; 409 while it is being prepared |
| `GET /api/team/default[?kind=&model=&provider=]` | the default team; the query runs every agent with that kind, model or provider instead of Claude Code with `sonnet` |
| `GET /api/teams[?kind=&model=&provider=]` | every team the benchmark suggests, the default first: `[{id, name, agents, runner}]`, `runner` being the runner the team's prompts are written for (`chain` when the benchmark names none or one shaked doesn't know); the query as for `/api/team/default` |
| `GET /api/runs` | the runs with their branches |
| `POST /api/agents/check` | `{kind: "http", url, headers?}` or `{kind: "bedrock", model_id, region}`, with an optional `name`: reaches the agent once (an `shakedown-agent/1` hello, or one Converse call of one token) and answers 200 `{ok: true, agent, name?, ms}`, `agent` being the team member as a team stores it (header values as `{secret: ref}`), or `{ok: false, error, ms}`; 400 for any other kind or a bad URL ([agents.md](agents.md)) |
| `POST /api/runs` | `{task_id, team, runner?, scenario?, seed?, benchmark?}`: start a run; returns `{run_id, branch_id}`. `benchmark` is the benchmark it runs on (the page's world; absent, the current one, read once when the run is asked for), its task and scenario checked against it; 409 when no benchmark of that name is installed. `max_cost_usd` (optional, a positive number) is the run's model-spend limit ([concepts](concepts.md#spend-limits)); absent, `SHAKEDOWN_MAX_COST_USD` applies; 400 for zero or less. `world_minutes` (optional, more than 0 and at most 1440; else 400 "world_minutes must be from 0 to 1440 minutes of world time") is an external or Code run's world time window ([concepts](concepts.md#world-time-windows)), recorded in `RunStart.world_minutes`; shaked's own runners ignore it. `seed` (0 to 2^53 - 1) is the seed the run's world starts its parts with, recorded in `RunStart.seed`; random when absent (every run starts from the same seeded base world either way; the seed reaches parts that draw from it, such as simulators). `scenario` is `{disruptions: [{id, params?: {name: value}}], clock?}`, options of `GET /api/benchmark`'s `scenario_options` (params left out take their defaults): shaked expands each with its template, adds it to the task's setup and records the expanded scenario in `RunStart` (`scenario`); 400 for an unknown option or param, a value out of range or not among the choices, or an option chosen twice. `clock` (`{read_ms, write_ms, tools?: {name: ms}, epoch_ms?, horizon_ms?, team_start_ms?}`) runs the branch on a stepped clock ([concepts](concepts.md#stepped-clock)), recorded with the scenario so forks keep it; 400 "stepped clock needs a team with a lead (dispatch runner)" for any other runner (also for an experiment's setup, the error naming it). `timed` (`[{part, disruption}]`) appends each `disruption` as it is to `args.disruptions` of that part's setup call, and `generators` (a list) is set as `args.generators` on every part's setup call ([protocol](protocol.md#a-runs-scenario)). `runner` is `chain` (default: one agent after another), `concurrent` (all at once), `dispatch` (a lead delegates to the team; see concepts, "Who drives the agents") or `external` (see below; the team may be empty and the answer adds `agents`, each agent's endpoints). `connection` (a Code connection's id) makes shaked launch that connection's driver for the run (the runner is then `external`; the answer adds `connection` and `driver`: `launched`, or `failed` when it couldn't start, the run then ending `failed` saying why); 404 for an unknown connection, 415/403 as for `POST /api/connections`. Each agent is `{id, name, kind, model, prompt, provider?, read_only?, lead?, tools?}`, or an `http` agent `{…, url, headers?, auth?}` (header values you type are kept in memory for the caller that typed them: the person in the UI, an API token and an MCP client each count as a different caller, so a value typed in the UI cannot be cited by your API token or an MCP client, and the other way round; `auth` is `{key: <name>}` of one of your own custom keys, or `{env: SHAKEDOWN_AGENT_NAME}` on a local engine; read on each request, never stored; 400 on any other kind of agent), or a Bedrock agent `{…, kind: "bedrock", model_id, region}` ([agents.md](agents.md)) (`read_only`, default false, gives the agent only read tools; `tools`, a list of world tool names, gives it only those (the runner's talk tools always come), and a call of another world tool is refused; `lead` marks the agent that leads a dispatch run, else the first does); 400 when a dispatch team marks two leads or has a Codex or `http` agent (neither can keep a session across turns), or when an API-mode agent's provider is unknown or unavailable, or has no default model and the agent names none |
| `POST /api/runs/{id}/name` | `{name}` sets an optional display name (1–300 bytes after trimming); 404 for a missing run. `GET /api/runs` includes `name` when set; task ids and event records are unchanged |
| `DELETE /api/runs/{id}` | delete a run and what it saved (409 while a branch runs) |
| `GET /api/branches/{id}` | a branch with its events (`?lean=1`: each model call's `request` and `response` read as null and its `provider_response` left out, for pages that show the world, not the calls), `budget` (`{max_cost_usd, estimate_usd}`: its spend limit, and its running estimate while it runs), `queue_position` (a queued cloud run's place in line, 1 = next; else null) and `metrics`: `{measured_s, overhead_ms: {save?, restore?, record?, gateway?, gate?, link?, mcp?, wait?, score?, pause?}, overhead_s, overhead_recorded, estimated_s, tokens, cost, agents: [{agent, kind, model, provider, tokens, prompt_tokens, completion_tokens, cost, model_calls, peak_prompt_tokens, compactions, overhead_s}]}`, each `cost` `{usd, label, source, as_of}` (see concepts, "Overhead and estimates"); `peak_prompt_tokens` is the largest `usage.prompt_tokens` of the agent's model calls, `overhead_s` the overhead recorded on its own events |
| `POST /api/branches/{id}/agents` | external runs: `{id, name?, kind?, model?, provider?, prompt?, read_only?}` registers an agent (kind `external` by default), or starts a registered one again; returns `{agent, mcp_url, gateway_url, otlp_url, env, token, tools, spec}` (`env` holds the `OTEL_EXPORTER_OTLP_*` variables, see below). 404 when the branch isn't live, 409 when shaked runs its agents |
| `POST /api/branches/{id}/agents/{agent}/end` | external runs: `{ok?, error?, prompt_tokens?, completion_tokens?, cost_usd?, cost_source?, seconds?}` records the agent's end |
| `POST /api/branches/{id}/agents/{agent}/usage` | external runs: `{message_id?, model?, input_tokens?, cache_creation_input_tokens?, cache_read_input_tokens?, output_tokens?, context_window?}` records one model call the agent's client saw (a CLI's per-message usage, such as Claude Code's `stream-json`) as a `model_call` with `request` and `response` null, `reported_by: "client"`, and `usage` whose `prompt_tokens` is the whole input (fresh, cache-written and cache-read tokens); `{compaction: {trigger?, pre_tokens?}}` records that the client compacted the agent's context (a `model_call` with empty `usage`). 404 for an agent the branch doesn't have |
| `POST /api/branches/{id}/agents/{agent}/reasoning` | external runs: `{items: [{text, kind?: "thinking"\|"step"\|"plan", model_call_seq?}]}` (1 to 100 items) records one `reasoning` event per item with `source: "agent"`; blank items are skipped and text over 32 768 characters is cut (`truncated: true`). Answers `{seqs}`. 400 for no items, too many or an unknown `kind`; 404 for an agent the branch doesn't have |
| `GET /api/connections` | `[{id, name, kind: "code", command, workdir, env, created_at}]`, newest first; `env` values whose name contains `KEY`, `TOKEN`, `SECRET` or `PASSWORD` read `"••••"` |
| `POST /api/connections` | `{name, command: [program, ...args], workdir?, env?}` saves a Code connection: the command that starts a team's driver. 400 for an empty command or a `workdir` that doesn't exist; 415 unless the body is JSON and 403 from a web page on another origin (any route that can start a driver refuses both) |
| `DELETE /api/connections/{id}` | deletes it (runs that used it keep its id); 404 for an unknown one |
| `POST /api/connections/{id}/test` | runs the benchmark's first task with it (spend limit $0.50, named `Connection test · <name>`), waits up to 5 minutes and answers `{ok, branch_id, status, agents, tool_calls, error, exit, stderr_tail}`; `ok` when the driver registered an agent that called a tool, finished and exited 0 |
| `POST /api/branches/{id}/finish` | external runs: freezes, scores and stops the world; returns `{status, error}` (409 when it is ending already). Before the end of the run's world time window it is held instead: `{status: "running", held_until_clock_ms}`, the run scored when the window ends ([concepts](concepts.md#world-time-windows)) |
| `POST /api/branches/{id}/sandbox-events` | a sandbox around an agent (`deploy/openshell`): `{agent, events: [{kind, text, data?}]}` (at most 200) records each as a world event of the part `sandbox` at the branch's clock, `data` `{agent, sandbox: data}`; returns `{recorded}`. 404 when the branch isn't live, 400 for an agent it doesn't have |
| `POST /api/branches/{id}/pause` | pauses a live branch, whoever drives it: new tool calls and gateway model calls wait (they don't fail) and the world clock stops; records a `pause` event; returns `{paused, changed}` (404 for no branch, 409 when it isn't running). `GET /api/branches/{id}` reports `paused` |
| `POST /api/branches/{id}/resume` | releases the calls that wait and starts the clock again; records a `resume` event `{paused_ms}` (the pause is counted as overhead `pause`, so it is no part of the estimate in production); returns `{paused, changed}` |
| `POST /api/branches/{id}/stop` | stops a live branch: its agents are dropped where they are (a CLI agent's process is killed), calls that wait are refused, and it ends `stopped`, unscored; a queued cloud run ends `stopped` at once and never starts; returns `{stopping}` (false when it was stopping already). The stream's final `status` says when it has ended |
| `GET /api/branches/{id}/stream` | Server-Sent Events: each `event`, then the final `status` |
| `GET /api/branches/{id}/rows?seq=N\|final&keys=<kind>/<id>,...[&part=]` | the state of those keys just after step N, or at the branch's end with `seq=final` (its check's final saves; a step at or after the check reads them too); at most 50 keys. A clock part saved before step N is shown moved on to step N's clock: its save is started in a scratch instance, advanced and inspected ([concepts](concepts.md#a-clock-parts-state-at-a-step)) |
| `POST /api/branches/{id}/fork` | `{at_seq, changes: {<agent id>: {prompt?, model?, kind?, provider?, read_only?, tools?}}, restore: "exact"\|"approximate"\|"none", team?, disruptions?: {add?: [{id, params?}], remove?: [id]}}` (default exact; `tools` is exactly the agent's world tools from the fork on: a tool the world doesn't offer or an empty list is 400, and tools beyond the agent's setup are named in the fork note; `none`, for evaluation only, keeps the agents' context but leaves the world as the parent ended, see concepts.md "Restore modes"; `disruptions` adds scenario options, chosen as a run's `scenario` chooses them, and removes disruptions the branch's setups armed before the fork point, from the fork point on: an unknown option or id is 400, a part whose scenario tool can't take the change 409); `team` (a run an external system drives only, and not with `changes`; else 400) gives the new branch another team, such as a single agent in place of a team: its agents' `history` holds the events of any agent of the same id, and the fork note says the team changed; returns `{branch_id, start_seq, moved_from, reason}`, and for an external run `agents`: each agent's `{agent, mcp_url, gateway_url, spec, history}` on the new branch, which stays live for the external system (`history` is the agent's recorded events before `start_seq`). The changes are checked on the team the fork would run, as `POST /api/runs` checks a team |
| `GET /api/branches/{id}/views` | `[{seq, event_seq, view}]`: the world's views recorded on the branch, in order (a fork's include those before its fork point, which its record starts with). `seq` is the step the view follows (the `world_view` event's `seq_of`), `event_seq` the view's own event, `view` the benchmark's `TwinView` ([protocol](protocol.md#the-worlds-view)); empty for a benchmark that draws none; 404 for an unknown branch |
| `GET /api/branches/{id}/world-views` | `{world, views: [{id, label, part, styles, panels, screens}], problems: [{file, message}]}`: the declarative views of the project world the run played (the world whose `package` is the run's benchmark), as parsed; `world: null` and no views without one ([protocol](protocol.md#declarative-views-world-files)) |
| `GET /api/branches/{id}/world-views/{view}?screen=<id>&at=live\|final\|<seq>` | one screen of a view evaluated at a point: `{view, screen, at, panels}`; `at` carries the world's `utc_offset_min` (null: UTC) and, when the step asked for couldn't be restored and the one before its unsaved write is shown, `fallback: {asked, why}` ([protocol](protocol.md#declarative-views-world-files)). Without `at`: live while the branch runs, else its end. 400 for a point with no state at all, 404 for an unknown view or screen, 409 for `live` on a branch that is not running |
| `GET /api/branches/{id}/recording` | `{video, of, sidecar}`: the branch's plant recording, if `SHAKEDOWN_RECORDINGS` holds one (an MP4 whose name has the branch id in it; a fork without one shows its parent's, `of` says whose), with its JSON sidecar; `video` is null when there is none. A recording part writes its frames on Shakedown time, so the video's second t is the branch's Shakedown second t |
| `GET /api/branches/{id}/recording.mp4` | that MP4, with byte ranges (so a player can seek); 404 when there is none |
| `GET /api/branches/{id}/restore-plan?at_seq=N&restore=exact\|approximate\|none` | `{parts: [{part, how, estimate_seconds?}]}`, or 409 with the refusal |
| `GET /api/compare?a=<branch>&b=<branch>` | `{a, b, first_divergent_seq, differing_rows, not_compared}` |
| `POST /api/experiments` | `{task, setups: [{team, runner?, name?, connection?}], runs?, scenario?, name?, max_cost_usd?, benchmark?, seed?, world_minutes?}` (`benchmark` as for `POST /api/runs`): an experiment (`max_cost_usd`: each run's spend limit, shown on each setup; `seed`, at most 2^53 - 1, drawn at random when absent: run i of every setup starts its world from the same seed, splitmix64 of `seed + i` below 2^53, recorded in each run's `RunStart.seed` and shown on each run row as `seed`; `world_minutes`, as for `POST /api/runs`: every run's world time window, recorded in its `RunStart`, enforced for Code setups' runs; the detail's and the list's head show `seed`, `world_minutes` and `max_cost_usd`), one task of the benchmark in one scenario (as for `POST /api/runs`, expanded once) run by 1 to 50 agent setups, each `runs` times (default 5, at most 1000; both caps are safety limits). Each setup is stored as a suite of that task (`repeats` = `runs`, the setup's team and runner, `experiment_id` and `setup_ord` set). shaked makes the runs one at a time, interleaved: every setup's first run, then every setup's second, and so on. Setup names default to `Setup A`, `Setup B`, … (`Setup AA` after `Setup Z`) and must be unique (trimmed); the name defaults to the setups' names joined by ` vs `. `runner` is `chain` (default), `concurrent` or `dispatch`; a setup may instead name a Code connection (`{connection, name?, team?}`): shaked launches its driver for each of the setup's runs (404 for an unknown connection, 415/403 as for `POST /api/connections`); 400 for `external` without a connection, an unknown task, too few or many setups, or a team `POST /api/runs` would refuse (the error names the setup). Returns its detail (below) |
| `GET /api/experiments` | every experiment, newest first: `{id, name, tasks, benchmark, runs, created_at, status, legacy, setups: [{suite_id, name, team_name, runner, runs, ended, passed}]}`. `status` is `running`, `finished` or `stopped`. A suite made on its own is listed too, as a one-setup experiment with the suite's id and `legacy: true` |
| `GET /api/experiments/{id}` | `{id, name, tasks, benchmark, runs, created_at, ended_at, error, status, legacy, scenario, setups}`: `scenario` expanded, `setups` each setup's suite report (as `GET /api/suites/{id}`) in setup order. A suite made on its own answers the same shape with one setup |
| `POST /api/experiments/{id}/stop` | no further run starts, the run that is running now is stopped (as `POST /api/branches/{id}/stop`), and the experiment and its suites end with `error: "stopped"`. Returns its detail; stopping one that ended changes nothing. 409 for a suite made on its own |
| `POST /api/suites` | `{team, tasks?, repeats?, runner?, name?, scenario?, scenarios?, max_cost_usd?, benchmark?}` (`benchmark` as for `POST /api/runs`): a test suite (`max_cost_usd`: each run's spend limit, in the suite's report) (`scenario` as for `POST /api/runs`, expanded once and used by every run; the suite's report has it, expanded, as `scenario`; or `scenarios`, at most 20, each expanded: every task × repeat runs once in each, so the suite has tasks × scenarios × repeats runs, and its report has them as `variants`; both at once is a 400), the team run over the benchmark's `tasks` (all when absent), each `repeats` times (default 1, at most 100); returns its report (below). `runner` is `chain` (default), `concurrent` or `dispatch`, and shaked then makes the runs one after another, or `external`, and an external system claims each run. The team is checked as `POST /api/runs` checks it; 400 for an unknown or repeated task |
| `GET /api/suites` | every suite, newest first: its definition with `status`, `runs`, `ended` and `passed` |
| `GET /api/suites/{id}` | the suite's report: `{id, name, team, tasks, repeats, runner, benchmark, created_at, ended_at, error, status, finished_s, runs, checks, summary, spread}`. `status` is `running`, `finished` or `stopped` (a built-in suite that ended before its last run: shaked stopped, or the saves reached the limit); `finished_s` the seconds from its creation to its last run's end. `runs`: per task × variant × repeat `{task_id, variant, repeat, run_id, branch_id, status, error, outcome}` (`variant`: the index in `variants`, 0 without them), `status` being `pending`, `starting`, `error` (it could not start), `missing` (its run was deleted) or the branch's status, and `outcome` `{status, scored, checks, checks_passed, checks_total, safety_violations, denials, flags, measured_s, estimated_s, tokens, cost_usd, peak_context}`. `checks`: per task and check `{task_id, name, safety, passed, runs, pass_rate}`. `summary`: `{runs, ended, scored, unscored, passed, pass_rate, score, safety_violations, runs_with_violation, denials, runs_with_denial, flags, runs_with_flag}` (`denials` and `flags` as [docs/protocol.md](protocol.md#denials-and-flags) defines them, summed over the scored runs) and the spread of each figure; `spread`: per task `{task_id, runs}` and the spread of each figure. A figure (`checks_passed`, `measured_s`, `estimated_s`, `tokens`, `cost_usd`, `peak_context`) is `{n, unknown, min, max, mean}` over the scored runs (`unknown`: runs without the figure, such as an unknown cost) |
| `POST /api/suites/{id}/claim` | external suites: claims the next run and starts it as an external run; returns `{done: false, task_id, variant, repeat, run_id, branch_id, agents}` (driven as any external run), or `{done: true}` when every run is claimed. 409 for a suite shaked runs itself |
| `GET /api/suites/compare?a=<suite>&b=<suite>` | suite `b` against suite `a`: `{a, b, verdict, checks, pass_rate, score, safety, measured_s, estimated_s, tokens, cost_usd, peak_context}`. Each is `{a, b, change, verdict}` (`checks` per task and check, with `task_id`, `name`, `safety`); `verdict` is `improved`, `regressed`, `same`, or `incomparable` when one side lacks the figure. Pass rates and `safety` (failed safety checks per scored run) count any change; time, tokens and cost (means over the scored runs) count a change beyond 10% of `a`'s. The top `verdict` is `regressed` when a check, the pass rate or safety regressed, `improved` when one improved and none regressed |
| `GET /api/review/settings` | `{enabled, model, why?, ask}`: whether AI review is on and with which reviewer (see [AI review](#ai-review)); `ask`, whether asking about a step is on (off only by `SHAKEDOWN_REVIEW=off`: it runs on the asker's own key) |
| `POST /api/branches/{id}/review[?again=1]` | starts an AI review of an ended branch, or returns the stored one (a failed one is tried again; `again=1` always makes a new one); answers at once with the stored row, `status: running` until the model answers. 404 for an unknown branch, 409 while it runs or when AI review is off |
| `GET /api/branches/{id}/review` | the stored review: `{key, kind, status, model, review, issues, error, cost_usd, cost_source, tokens, seconds, digest_bytes, created_at, finished_at}` (`status` `running`, `done` or `error`); 404 before any. `review` is `{verdict, went_wrong: [{title, why, steps, severity, unverified?}], suggestions: [{change, why, kind, agent?, prompt_text?, step?}], went_well, confidence}` |
| `POST`/`GET /api/compare/review?a=&b=[&again=1]` | the same for two ended branches; `review` is `{headline, differences: [{title, a, b, why_it_matters, steps_a, steps_b, unverified?}], which_is_better, why}` |
| `POST`/`GET /api/suites/compare/review?a=&b=[&again=1]` | the same for suite `b` against `a`; `steps_a` and `steps_b` are example runs (branch ids) of each suite |
| `POST /api/branches/{id}/questions` | `{seq, question, model?, wait?}`: asks about step `seq` (operation `questions.ask`, see [Ask about a step](#ask-about-a-step)); answers at once with the stored row (as a review's, `kind: ask`, `status: running`), or with `wait: true` once answered. 400 for a blank question (or over 2000 characters) or a bad `model`, 404 for no such branch or step, 409 when asking is off or the model would not run on your own key |
| `GET /api/branches/{id}/questions[?seq=]` | the questions asked about the branch's steps (or step `seq`'s), oldest first (operation `questions.list`): `{questions: [row]}`, each `review` being `{question, step, agent, asked_by, mine, answer?, evidence?: [{source, step, field, quote, why}], speculation?: [{text, based_on}], sources?, sources_left_out?}` |
| `GET /api/world` | the benchmark's parts with what each integration reports (version, restore, clock use, tool names, and `system` when it says what kind of system it is) and whether it answered (each gets 5 s; a silent one shows `did not answer within 5 s`) |
| `GET /api/usage` | `{integrations: [{integration, parts, bytes, error?}], total_bytes, limit_bytes}`: the bytes each integration keeps (its own `usage` plus its save folders) and the world parts that use it. An integration whose `usage` fails or doesn't answer within 5 s has `bytes: 0` and an `error`, and adds nothing to `total_bytes` |
| `POST /api/gc` | runs the startup cleanup and returns `{removed, freed_bytes, errors}`; it keeps every save a recorded run names and the shared base and gold saves; 409 while agents are running, and new runs and forks answer 409 while it runs. It has no deadline: it answers when every integration's `gc` has (an integration that never answers keeps the cleanup on until it exits), and a client that stops waiting does not end it |

An agent's own OpenTelemetry SDK sends its spans to `POST /otlp/{branch}/{agent}/v1/traces` (OTLP/HTTP,
`application/x-protobuf` or `application/json`, uncompressed, with the agent's token as a bearer token): each
request becomes one `spans` event of that agent. At most 4 MiB a body (413), 512 spans a request and 20,000 a
branch (the rest dropped and counted in the answer's `partial_success`), 20 requests a second a branch (429);
404 once the branch has ended. `/v1/metrics` and `/v1/logs` there take the request and drop it. Each agent's
endpoints carry `otlp_url` and, in `env`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`,
`OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` and `OTEL_EXPORTER_OTLP_HEADERS=Authorization=Bearer%20<token>`; an
`http` agent's run body carries `otlp_traces_url`.

New runs and forks answer 507 once the saves reach the disk limit. The `/api` routes answer only
requests whose `Host` is a loopback address. The agents' MCP endpoints (`/mcp/{branch}/{agent}`) and the
model gateway are for shaked's own agents. The gateway's main routes are, under `/gateway/{branch}/{agent}/v1/`:
`chat/completions` (the canonical format of API mode, sent to any provider), and two pass-through routes for CLI
agents, `messages` (Anthropic Messages, streamed or not, to an `anthropic` provider) and `responses` (OpenAI
Responses, streamed or not, to `openai`, `openrouter` or a local server); an agent that names no provider gets
`anthropic` on `messages` and `openai` on `responses`. Each takes the agent's token, as a bearer token or as
`x-api-key` (as Claude Code sends its key), or the worker token of the branch (any other branch's is 401), and
picks the key as `chat/completions` does: your own key, then the environment's, then the layer's. A pass-through
request goes on as the agent sent it, with that key and, of the agent's headers, only `anthropic-version`,
`anthropic-beta` and `openai-beta`; the answer comes back as it came, a stream chunk by chunk. A body is read only
once the call is let through (its token checked, its branch not held), up to 32 MiB on `messages` and
`messages/count_tokens` and 64 MiB on `chat/completions` and `responses` (413 past it, `{error}` as every refusal).
It is recorded once, when it ends, as a `model_call` event holding the raw request, the response (a
streamed one made whole: a Messages stream's message, a Responses stream's final response), `usage` in the
canonical counts (`prompt_tokens` is the whole input; Messages also keeps `input_tokens`,
`cache_creation_input_tokens` and `cache_read_input_tokens`, Responses `prompt_tokens_details.cached_tokens`),
`provider`, `api` (`messages` or `responses`), `stream: true` when streamed, and `interrupted` (why) when the
provider's stream broke off, stayed silent for 5 minutes (a stream has no limit on its whole length), said `error`
on the way, or ended without its final event (`message_stop` or a `message_delta` with its stop reason;
`response.completed` or `response.incomplete`); it is
priced and metered as a `chat/completions` call is. A call whose cost is known answers `x-shakedown-cost-usd`, on
every route; a failed call, whose cost is unknown, answers none (never 0). A
provider that can't be found is a 502, recorded as a failed call. `messages/count_tokens` goes to the provider the
same way and is neither recorded nor charged, but refused like a model call while its branch is held. With a worker
token the control box holds the branch itself, whatever its worker does and whether it is connected or not:
- 503 while the worker is not linked (try again), or when the branch has no rules on record (`this branch has no
  limits on record`: a branch from before they were kept; never unlimited);
- 404 for an agent not in the branch's team;
- 429 while the branch's pool of calls in flight (4 times its agents, shared by all of them) has been in use
  for more than 10 s (`count_tokens` takes none);
- 409 while the branch is paused (with `x-shakedown-held: paused`) or stopping;
- 402 once the run's spend estimate on the control box passed its limit, or the branch is stopping for it (see
  concepts.md, "Spend limits");
- 400 for a model whose cost could not be known (no reported cost, no price in the table).
Each such call's event also keeps `charged_usd` (and `estimated: true` when that is the conservative estimate).
The control box keeps the branch's rules (`branches.rules`, its spend limit among them) and its hold
(`branches.hold`: `paused`, `stopping` or `budget_exceeded`) in the store, so a restart keeps both; it never reads a
limit from the worker's own records.
A cloud run (hosted) runs in a sandbox of its own, one per org at a time. A run started while the org's
sandbox is busy is recorded with status `queued` and starts by itself when the runs before it are done, first
come first served; at most 50 wait per org, beyond that `POST /api/runs` answers 429. A queued run can be
stopped like any other, and one still queued when the control box restarts ends `interrupted`, saying so.
`GET /worker` is the WebSocket a `shaked worker` running a branch in a sandbox links to: it says hello with
a one-use ticket (good for ten minutes, bound to its branch and sandbox) and gets the branch's plan and a
worker token, which it reconnects with, also after shaked restarted: each event keeps the worker's own
counter (`events.link_n`), so an event sent again is acknowledged with the seq it got, never appended twice.
The token only links its own branch and calls the gateway for its agents, and dies when the branch ends. It lives
a day of unpaused time: a pause does not count against it (the store keeps when the pause began, and the resume
extends the token by it), so the worker of a long-paused branch can still rejoin, and a restart keeps the branch. A
link must say hello within 10 s, frames are at most 8 MiB, and a link silent for 60 s (pings unanswered) is
closed. Pause, resume and stop on such a branch are held by the control box: the hold is stored first, enforced on
the branch's model calls, and told to the worker when it is connected and again whenever it (re)joins; a pause or
stop never answers 409 because the worker is away. A stop ends the branch on the control box: at once when no worker
is connected, else when the worker reports its end, or after 30 s when it doesn't; a stop pressed again ends it at
once when no worker is connected. The branch then ends `stopped` (or `budget_exceeded` for a stop for its limit)
whatever the worker reports, its tokens are revoked, so its worker's calls get 401, a connection still open is
told `Refused` (the branch ended) and closed, and the branch takes no more events: every end holds the branch's lock
over its status and the revoke. A restart loses no stop: at start, and every minute, the control box arms the 30 s
again for each branch it was stopping (and again when a worker rejoins one). The same sweep ends `infra_error` a
running, unpaused branch whose worker token died (a day of unpaused time after it was made; checked again as it
ends); at start, such a branch is marked `interrupted` first. A paused branch is left alone: its token's time stands
still while it is paused (`branches.paused_at`).

## Configuration

Environment variables, read once at startup by shaked:

- `SHAKEDOWN_PG_URL`: the Postgres server for shaked's records, passed on to integration programs (default
  `postgres://shakedown:shakedown@127.0.0.1:5544/postgres`, else the same server's old user `shakedown`).
- `SHAKEDOWN_PROJECT`: the project folder whose worlds, scenarios, setups and experiments shaked serves (each a
  YAML file: `worlds/<id>/world.yaml`, `scenarios/<id>.yaml`, `setups/<id>.yaml`, `experiments/<id>.yaml`,
  with `shakedown.yaml` at the root). Edits made outside the app are seen on the next request. Unset: the
  project routes answer 409.
- `SHAKEDOWN_BIND`: where shaked listens (default `127.0.0.1:4510`). An open-source shaked serves only the person
  at this machine and refuses a bind beyond loopback. The old `SHAKEDOWN_PORT=<port>` still binds
  `127.0.0.1:<port>`.
- `SHAKEDOWN_PUBLIC_URL`: the base of every URL shaked hands out (MCP, gateway, register answers, drivers;
  default `http://<bind>`). `SHAKEDOWN_AGENT_BASE`, its old name for HTTP agents, is still read. An
  open-source shaked uses it to shape those URLs only: `/api` still answers this machine's loopback names
  alone, so a tunnel reaches `/mcp` and `/gateway` (which take the run's token) and never the API.
- `SHAKEDOWN_ALLOWED_HOSTS`: more `Host` names `/api` answers, comma-separated, for a shaked that serves
  others (default: loopback only). An open-source shaked refuses to start with it set.
- `SHAKEDOWN_DB`: the database of shaked's records on that server (default `shakedown`, else an existing
  `shakedown`).
- `SHAKEDOWN_VAULT_KEY` (64 hex characters) or `SHAKEDOWN_VAULT_KEY_FILE`: the key your model keys (and the
  headers of your observability targets) are sealed with (AES-256-GCM, each bound to its row). Without either, `core/vault.key` is made (mode 0600, ignored by
  git) the first time a key is saved; a shaked whose layer serves others refuses to start without one of the
  two. A lost vault key makes the saved keys unreadable: add them again.
- `SHAKEDOWN_OBJECT_STORE`: object stores for saves, transcripts and exports, `name=url,name=url` or one URL
  (named `default`): `r2://<bucket>?account=<id>[&prefix=p]`, `s3://<bucket>?region=<r>[&endpoint=<url>][&allow_http=true][&prefix=p]`,
  `file:///dir` or `memory://`. Rows keep the store's name and the key, never a URL; any listed store serves
  reads, and `SHAKEDOWN_OBJECT_STORE_WRITE` names the one that takes writes (default: the first). Keys come only
  from `SHAKEDOWN_OBJECT_STORE_<NAME>_ACCESS_KEY_ID` and `_SECRET_ACCESS_KEY` (else `AWS_ACCESS_KEY_ID` and
  `AWS_SECRET_ACCESS_KEY`); a URL holding credentials is refused. Nothing is stored there yet: saves stay on
  the parts' disks.
- `SHAKEDOWN_BENCHMARK`: the benchmark new runs use, by package name (default: the only installed benchmark,
  not counting those whose manifest says `opt_in = true`; required when several are installed).
  A benchmark chosen with `POST /api/worlds/{name}/use` (or a world made current when it was built) is
  remembered in `SHAKEDOWN_WORLDS/.current`; at the next start `SHAKEDOWN_BENCHMARK`, when set, wins over it, and it wins
  over the only installed benchmark. A remembered benchmark that can't describe itself at start is logged and
  passed over.
- `SHAKEDOWN_WORLDS`: where built worlds are written; scanned at start like `SHAKEDOWN_PACKAGES` (default `core/worlds`,
  ignored by git).
- `SHAKEDOWN_TEMPLATES`: `:`-separated folders of more world templates, passed on to the world builder.
- `SHAKEDOWN_PACKAGES`: `:`-separated folders whose subfolders hold an `shakedown-package.toml` (default
  `integrations:benchmarks` in the repository root).
- `SHAKEDOWN_BIN_DIR`: what `${SHAKEDOWN_BIN_DIR}` in a manifest stands for (default `core/target/release`).
- `${SHAKEDOWN_DIR}` in a manifest's command stands for the repository folder (the one above `core`).
- `SHAKEDOWN_SAVES`: where saves are kept (default `core/saves`, ignored by git). Integrations that run in Docker or
  on their own servers also keep data elsewhere.
- `SHAKEDOWN_MAX_DB_BYTES`: disk limit, see [Disk](#disk) (default 5 GiB, `5368709120`).
- `SHAKEDOWN_MAX_COST_USD`: the model-spend limit of a run that names no `max_cost_usd` (a positive number of dollars;
  default none). See [concepts](concepts.md#spend-limits).
- `SHAKEDOWN_DISPATCH_MAX_MINUTES`, `SHAKEDOWN_DISPATCH_IDLE_SECS`, `SHAKEDOWN_DISPATCH_IDLE_WAKES`: a dispatch run's guard
  rails (defaults 60 minutes, 20 seconds, 3): the longest a run may go on (time paused left out), how long the
  whole team may wait with nothing to do before the lead is woken, and how many such wakes before the run ends
  as failed. Read each time a dispatch branch starts. A value that isn't a whole number counts as unset.
- `SHAKEDOWN_CONTEXT_TURNS`: how many of an API-mode dispatch agent's latest turns keep their tool results whole
  (default 4; `0` never trims); older results longer than 400 characters become a placeholder before each turn
  ([concepts](concepts.md#dispatch-runs)). Read when an agent's session starts.
- `SHAKEDOWN_AGENT_MAX_MODEL_CALLS`: how many model calls an API-mode agent's turn makes at most (default 40). A
  dispatch turn that reaches it ends with a note; a chain or concurrent agent fails ([concepts](concepts.md#dispatch-runs)).
  Read once, at the first turn. A value that isn't a whole number above 0 counts as unset.
- `SHAKEDOWN_CLOCK_SPEED`: speed of the world clock (default 1, real time; 0 stops it; anything that isn't a number
  of at least 0 counts as 1). Unlike the others, it is read each time a run or fork starts a branch, not once at
  startup.
- `OPENROUTER_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`: the keys of the OpenRouter, OpenAI and Anthropic
  providers of API mode. Each key turns its provider on. Pick the provider and model per agent on the Team page
  (or per fork in "Change and rerun").
- `SHAKEDOWN_LOCAL_MODELS`: OpenAI-compatible servers to use as providers, `name=url[,name=url…]`, each the provider
  `local:<name>`. The name matches `^[a-z0-9_-]+$` and the URL is `http` or `https`. A bad entry, or a name
  given twice after its first, is skipped with a warning on stderr that never shows the URL.
- `SHAKEDOWN_LOCAL_KEY_<NAME>`: an optional key for `local:<name>`, sent as a bearer token (`<NAME>` is the name
  upper-cased, with `-` as `_`).
- `SHAKEDOWN_LOCAL_PRICE_<NAME>`: an optional token rate for `local:<name>`, `<in>,<out>` in USD per million prompt and
  completion tokens (cached prompt tokens at the input rate), so its calls are priced (cost source "rate set for this
  local server") and count toward a run's `max_cost_usd`. Without it a local server's calls cost 0. A value that
  isn't two numbers at least 0 is reported at startup (never the value) and the server stays unrated.
- `SHAKEDOWN_ANTHROPIC_MAX_TOKENS`: the `max_tokens` of a Claude call whose request names none (default 16000). A
  value that isn't a positive whole number is reported at startup and the default is used.
- `SHAKEDOWN_REVIEW`: `on` or `off` (default on when the reviewer can run). Off: no review is started and nothing is
  sent to a model; stored reviews can still be read.
- `SHAKEDOWN_REVIEW_MODEL`: the reviewer, `claude_code:<model>` (default `claude_code:sonnet`, Claude Code headless,
  signed in as the CLI agents are) or `<provider>:<model>` for one of the providers below (`openrouter:<model>`,
  `anthropic:<model>`, `openai:<model>`, `local:<name>:<model>`). A reviewer that can't run (no `claude` on the
  PATH, a provider without its key) turns AI review off, with why in `/api/review/settings`.
- `SHAKEDOWN_RECORDINGS`: a folder of plant recordings fetched from a recording part (Isaac Sim writes
  `<record>-b_<branch>_plant.mp4` and its `.json` sidecar), which the world view shows as "Plant (Isaac Sim)" beside
  the twin, held at the Shakedown second of the step on screen. Optional.
- `SHAKEDOWN_UI_DIST`: a built UI folder for shaked to serve at `/`. Optional: shaked is fully usable without a UI
  (from code, see [sdk.md](sdk.md)), and the UI can be served from anywhere else.
- `SHAKEDOWN_CORS_ORIGIN`: web origins, comma-separated (`http://127.0.0.1:5173,https://ui.example`), whose pages
  may call `/api` from a browser, for a UI served from elsewhere. Exact matches only; `*` and anything that isn't
  `http(s)://host[:port]` is skipped with a warning (a wildcard would let any web page drive this machine's Shakedown).
  Unset: same origin only. The `/api` routes still answer only a loopback `Host`.
- `SHAKEDOWN_AGENT_BASE`: the base URL an `http` agent is told to reach its MCP endpoint at (`mcp_url`), for an agent
  on another machine reaching shaked through a tunnel. Unset: shaked's own address ([agents.md](agents.md)).

A blank key counts as unset. A key a header can't carry is reported at startup (by its variable's name) and
ignored. shaked never logs, records or returns a key. The CLI agents don't get them: each CLI keeps only its own
vendor's key (see [concepts.md](concepts.md#teams-and-agents)).

Default paths are relative to the repository root, fixed when shaked is built, so they hold wherever shaked is started
from. The retail example reads its data from `data/retail` (relative to its package folder, `../../data/retail`);
`SHAKEDOWN_RETAIL_DATA` overrides that when you run its program directly, but shaked does not pass it on.

## Commands

- `shaked mcp [--project <folder>]`: the control MCP server over stdio for a project folder (default
  `SHAKEDOWN_PROJECT`), one JSON-RPC message per line. It runs as the person at this machine on the folder
  alone: no database, no server.
- `shaked import <git-url-or-path> [--ref <ref>] [--id <id>] [--project <folder>] [--dry-run]`: imports a git
  repo into the project folder (default `SHAKEDOWN_PROJECT`; a folder that is not a project yet becomes one with
  the repo's `shakedown.yaml`), as `project.import_git` with `confirm` (the person typing is the confirmation):
  see [Import from git](#import-from-git). A relative path is taken from the working folder. It prints what was
  found and done. A running shaked registers the packages at its next start or on the next deploy;
  `--dry-run` prints the plan and changes nothing.
- `shaked token create --name <name> [--scope read|run|admin ...] [--expires-days N]`: prints a new API token
  (default scope `read`) and nothing else.
- `shaked store copy --from <store> --to <store> [--prefix <p>/] [--repoint]`: copies every object (under
  `--prefix`) from one store of `SHAKEDOWN_OBJECT_STORE` to another, each checked by sha256 once it lands; an
  object already there with the right checksum is skipped, so it reruns safely. `--repoint` points the saves
  whose objects all verified at the destination; a save whose object no longer matches its recorded checksum
  stays where it is. Exits 1 when any object failed.
- `shaked worker --link <ws(s)://host/worker> --ticket-file <path>`: runs one branch in a sandbox for the shaked
  at `--link` (cloud runs start it; you don't run it by hand). It waits for the ticket file, reads it and deletes
  it, links to `GET /worker`, stands the world up from the base save and drives the agents. Hand the ticket over
  by writing it, newline-terminated, to a temporary file in the same folder and renaming that to `--ticket-file`:
  the worker reads the file only once it ends with a newline. Every event goes over the link and counts as
  recorded once acknowledged; at most 64 wait for acknowledgement. It holds no database and no secrets (no vault,
  no model keys from its environment), and its agents reach its MCP endpoint, gateway and proxy on `127.0.0.1`
  only. Its gateway checks the agent's token, holds a paused branch's calls and makes the save point, then sends
  each model call on to the control box's gateway (`Start.gateway`, the same route) with the worker token as its
  only credential: the control box adds the run's key (chosen as above), calls the provider and records the call as
  its one `model_call` event (the worker records none). A call the control box held for a pause the worker had
  not heard of yet goes again once the pause ends; its 402 puts the run over its limit in the worker too, and its
  `OverBudget` frame stops the branch, which ends `budget_exceeded`. Claude Code there gets `ANTHROPIC_BASE_URL`
  (its agent's route on the worker's gateway) and Codex `OPENAI_BASE_URL` (the same, `…/v1`), each with the
  agent's token as its key (`ANTHROPIC_API_KEY`; `OPENAI_API_KEY` and `CODEX_API_KEY`). In a cloud sandbox
  (`SHAKED_AGENT_NETWORK`, `SHAKED_AGENT_IMAGE` and `SHAKED_AGENT_LISTEN` set, by the template in
  `deploy/sandbox`), each Claude Code or Codex process runs in a throwaway container of the agent image, as uid
  1000 with no capabilities, 1 GiB and 256 processes at most and a read-only root, on a Docker network that reaches only the worker's MCP endpoint and gateway at
  `SHAKED_AGENT_LISTEN`; it sees only its work folder and the variables above. An event too large for one frame is cut down first: strings over 16 KiB are cut to 16 KiB and the payload
  gets `truncated: true`; when that is still too large, the payload becomes `{truncated: true, bytes, head}` (its
  size and its first 64 KiB as JSON). A dropped link is tried again at once, then every 2 s, resending what was
  not acknowledged. The link counts as down until the control box shows it works again (an ack, a pause, resume
  or stop, or a ping while nothing waits for an ack), so a connection that is accepted and then closed, or that
  pings but never acknowledges, keeps the clock running; after 5
  minutes down the worker stops the world and exits 1. It exits 0 once the branch's end is recorded.
  `SHAKEDOWN_WORKER_RETRY_MS` and `SHAKEDOWN_WORKER_GIVE_UP_MS` change the 2 s and the 5 minutes (for tests).

## Import from git

A customer's repo becomes the world as it is: its packages run as they are written, and its worlds, scenarios,
setups and experiments become the project's (design: `docs/superpowers/specs/2026-11-22-git-import-design.md`).
From the app (**Import from git** on the Worlds page: Check, then Import), the API (`POST
/api/project/import-git`), MCP and the assistant (`project_import_git`) or the CLI (`shaked import`).

- **Fetch:** one ref (a branch, a tag or a full commit SHA; default `HEAD`) with depth 1, into
  `<project>/.shakedown/imports/<id>/<commit12>/`, hooks off, no prompts, no submodules. The id is the repo's
  name unless given. Only the https, http, ssh, git and file transports are allowed.
- **Found:** `shakedown.yaml` at the repo's root (its worlds with their folders, scenarios, setups and
  experiments, and each world's `views:`), every `shakedown-package.toml` in the tree (not under dot folders,
  `node_modules`, `target` or `__pycache__`), and Compose files outside packages, listed as candidates for the
  `service` integration with the part config a world would use. Nothing found is run.
- **Python:** a package whose command starts with `uv` runs in its uv project (`--project`, else its folder) or the
  workspace holding it. Each workspace gets one environment per commit,
  `<project>/.shakedown/imports/<id>/env/<commit12>-<root>/`, made with `uv sync --all-packages`; no `.venv` lands in
  the checkout. The programs get `UV_PROJECT_ENVIRONMENT` (that folder) and `UV_FROZEN=1`. Sources of Shakedown's own
  packages (`shakedown-part`, `arena-part`, `shakedown-sdk`) are pointed at this Shakedown's `kits/python` and
  `sdk/python`, whatever path the repo's author had; the edits are listed and show as `git diff` in the checkout.
  Another absolute path source is noted when it is on this machine and is a problem when it is not.
- **Registered:** the packages join the registry under the import's id. A name another package has is refused
  before anything is installed. shaked registers the configured project's imports at start, and `experiments.deploy`
  registers any that are not (an import made with `shaked import` while shaked runs).
- **Items:** copied under their own ids. An id the project has already is kept unless the last import of this repo
  wrote it and it is unchanged since; then it is replaced. Each item's action is in the plan.
- **Items the repo no longer has:** the import record lists what each import wrote, with a digest. On a re-import,
  an item the last import wrote that the repo no longer has is removed (action `remove`) when the project's copy is
  unchanged since, nothing left in the project uses it and no other import of the project holds it. It goes to the
  project's trash (`.shakedown/trash`), so it can be restored. One the project changed since is kept, the project's
  own from then on; one something still uses (an experiment of yours on an imported setup) is kept and removed by a
  later import once nothing does; one another import holds is kept. Items
  made in the project are never removed, and neither are runs: they stay where they are, and their pages still open.
  The answer lists what was removed under `removed`.
- **A new commit:** a new checkout and environment; the packages are swapped once they are ready (a run going keeps
  its program). The commit before is kept, older ones removed. Fingerprints follow the code and the world folders,
  so old runs keep the fingerprint they recorded.
- **Trust:** the repo's code runs on this machine as a package program, with the cleared environment every
  `command` package gets. The import itself runs only git and `uv sync` (which may run the repo's build backend). A
  shaked that serves others refuses imports (403) until a run's sandbox can run them.

Verified: the tests in `core/crates/shaked/tests/git_import.rs` (a repo made in the test, with one Python part run
through uv, is imported, registered and deployed; a new commit keeps the project's own edits; a re-import removes
what the repo dropped unless the project changed or uses it) and, on this machine,
`live_airline_world_imports` against the airline repo (11 packages, 2 worlds with their views, 2 scenarios, 2
setups, 2 experiments, no problems; its benchmark describes itself from the import's environment).

## Runtimes

A runtime is the package that runs a world (its parts, tasks and scoring; a world names it in `package`). From a fresh
Shakedown with no packages, everything is done in the app: **Settings › Advanced › Runtimes › Install a runtime** (also in a world's
Runtime section), then **New world** with the runtime picked. The API is
`/api/runtimes` ([HTTP API](#http-api)); MCP and the assistant have `runtimes_list`, `runtimes_install`,
`runtimes_check` and `runtimes_delete`.

- **Sources:** a git URL (as [Import from git](#import-from-git) takes it, with a ref) or an uploaded `.zip`, `.tar.gz`
  or `.tgz` of a package folder or of a repo holding packages. An archive is unpacked into
  `<project>/.shakedown/imports/<id>/<sha12>/`, named after its SHA-256 (which stands for the commit); only regular
  files and folders are unpacked, never outside that folder, at most 50,000 entries and 2 GiB, and an archive whose
  entries all sit in one top folder (as GitHub's downloads do) is that folder. **Folder** limits the install to the
  packages under one folder of the repo or archive; a package's uv workspace may still be above it.
- **Install:** the import machinery with packages only: each `shakedown-package.toml` is read and checked, the Python
  packages get their uv environment (Shakedown's own kit pointed at this Shakedown's), the packages are registered, and
  no world, scenario, setup or experiment is written. The id is the repo's (or archive's) name unless given; installing
  with an existing install's id updates it (a new commit or archive replaces its packages, the one before is kept for
  runs still going). An id that Import from git used with its project files is refused: update that one there.
- **Conformance check:** each package is checked on a hub of its own, its saves in a temporary folder. An integration
  gets `shakedown-part-check`'s check (`parts::check`) with an empty config (it needs at least one `[[example]]` in its
  manifest); a benchmark is described and must have a task and every part's integration installed. The reports are
  kept in `check.json` beside the install's record, for its commit, and shown on the Runtimes page (Check again reruns
  them). A failed check is reported, not enforced.
- **Listing:** each install with its source, packages, versions, check and the worlds that run on each package;
  packages installed from `SHAKEDOWN_PACKAGES` folders are listed as installed with Shakedown.
- **Remove:** refused (409) while a world's runtime is one of the install's packages, naming the worlds; otherwise the
  packages are unregistered and the install's checkout and environments deleted.
- **Startup:** shaked starts with no benchmark when none is chosen (none installed, as in a fresh Shakedown, or several
  and `SHAKEDOWN_BENCHMARK` names none), saying so; each project world names its own runtime.
- **Trust:** a runtime is code that runs on this machine, with your access: installing runs `uv sync` (and the
  packages' build backends), the check and every run run its programs. The app says so before every install. A shaked
  that serves others (hosted) refuses to install, check or remove one (403), as it refuses Import from git: hosted runs
  package code only in a run's sandbox, which can't install a runtime yet.

Verified: `core/crates/shaked/tests/runtimes.rs` (a folder of a repo installs its packages only and is checked; an
archive installs, and a world that uses it keeps it installed; bad archives and folders are refused; hosted refuses)
and, on this machine, installing `CopperTide/tau-world` from an archive and from its git URL on a fresh Shakedown
through the app.

## Use Shakedown from an agent (MCP)

Every project operation (the project's status, settings and import; listing, reading, writing and deleting worlds,
scenarios, setups and experiments, and what uses each; undoing a delete; renaming a world group; deploying an experiment and listing its deployments; reading a MADL setup) is
defined once in shaked's operation registry and served two ways: as an HTTP route and as a tool of the control MCP
server (tool names replace `.` with `_`: `worlds_put`, `experiments_deploy`), so the UI, the assistant, scripts and any
agent can do the same things. Runs, forks, compare and the other `/api` routes are HTTP only.
A test (`core/crates/shaked/tests/parity.rs`) fails if an operation is missing from either.

- On this machine, over stdio: `claude mcp add shakedown -- shaked mcp --project ~/work/my-project` (Codex:
  `[mcp_servers.shakedown] command = "shaked"`, `args = ["mcp", "--project", "…"]`).
- Over HTTP: `POST /mcp` with `Authorization: Bearer <token>`. A `read` token lists and reads; editing or
  deleting project files needs `admin`; starting runs needs `run`.
- Edits follow the project's `permissions` in `shakedown.yaml`: with `edit: diff_first` every operation that takes
  `confirm` (a put, `project_update`, `project_import`, `project_restore`, `worlds_rename_group`, and a cascading
  delete) returns what it would change and writes only when sent again with `confirm: true`. `spend` is stored but not enforced yet: the assistant always
  asks before a deploy, and an MCP client's token scope alone decides.
- `shaked mcp` (stdio) has no running engine, so `experiments_deploy` and `experiments_runs` answer that they need
  one; use `POST /mcp` on a running shaked for those.
- Hosted, the server is `https://app.coppertide.ai/sd/mcp` with an API token (Project settings → Use from an agent).

This is separate from the MCP endpoint an agent under test gets in a run (`/mcp/{branch}/{agent}`, the world's
tools).

## Model providers

An API-mode agent (`kind: "open"`) names its provider in `provider`; without one it uses `openrouter`, so teams
and runs made before providers keep their meaning. The agent loop speaks OpenAI chat completions to shaked's
gateway, and the gateway talks to the provider:

| Provider | Endpoint | Key | Models listed by `/api/providers` | Default model |
|---|---|---|---|---|
| `openrouter` | `https://openrouter.ai/api/v1/chat/completions` | `OPENROUTER_API_KEY`, bearer | not asked (type an OpenRouter model id) | `deepseek/deepseek-chat-v3.1` |
| `openai` | `https://api.openai.com/v1/chat/completions` | `OPENAI_API_KEY`, bearer | `GET /v1/models`, without embedding, audio, image, moderation and other non-chat models | none: name one |
| `anthropic` | `https://api.anthropic.com/v1/messages` (the Messages API, translated) | `ANTHROPIC_API_KEY`, `x-api-key` | `GET /v1/models` | none: name one |
| `local:<name>` | `<url>/chat/completions` of the `SHAKEDOWN_LOCAL_MODELS` entry | `SHAKEDOWN_LOCAL_KEY_<NAME>`, bearer, if set | `GET <url>/models` | none: name one |
| `bedrock:<region>` | `https://bedrock-runtime.<region>.amazonaws.com/model/<model>/converse` (the Converse API, translated; untested live) | the user's AWS credentials, SigV4 or a Bedrock API key ([agents.md](agents.md#bedrock-agents)) | not listed | none: name one |

Your own keys (Settings → Models, or `/api/models/keys`) come first: an `openai`, `anthropic` or `openrouter`
key replaces the environment's key for that provider (on its server, unless the key names a `base_url`), a
`bedrock` key is used for its region, and a `custom` server is the provider `custom:<name>` (taking over a
`local:<name>` of the same name). The newest key of a provider wins. Without one, the environment's key is
used. An agent can also name one key as its provider, `key:<name>`. Where keys are shared between people (a
hosted org), only your own keys apply by kind: another member's key serves your agent only when it names it
`key:<name>`, and never when that key names its own server. In a cloud run the same keys apply, the run
owner's: the sandbox holds none, and the control box adds the key to each model call.

A local provider is any server that speaks the OpenAI chat-completions API, such as Ollama, vLLM, LM Studio or
llama.cpp's server. For example:

```bash
cd core && SHAKEDOWN_LOCAL_MODELS="ollama=http://127.0.0.1:11434/v1,vllm=http://127.0.0.1:8000/v1" cargo run -p shaked
```

Then pick `local:ollama` or `local:vllm` as an agent's provider and name a model the server serves (for Ollama, a
model you pulled, such as `llama3.3`).

**`GET /api/providers`** asks each local server, and OpenAI and Anthropic when their key is set, for its models,
all at once, each within 3 s; a list over 1 MB is given up. `available` is false, with a plain `why`, when a key
is not set (`OPENAI_API_KEY is not set`), when a list was refused for its key (`the key was rejected`), or when a
local server doesn't answer (`<host:port> did not answer within 3 s`). When OpenAI's or Anthropic's list fails
otherwise, the provider stays available with the reason, and the model is typed in. `checked` is true when the
provider itself was asked. `why` names a host and port, never a URL's credentials. The web UI shows one entry per
provider under the top bar (glyph, state and reason), and its Check button asks this route.

What the gateway does per provider:

- **OpenRouter:** the request goes out as the agent sent it, plus `usage: {include: true}` so OpenRouter
  reports the cost. Only OpenRouter gets that field.
- **OpenAI and local servers:** the request goes out as the agent sent it.
- **Anthropic:** the request is translated to the Messages API (`anthropic-version: 2026-10-21`): system
  messages become `system`, tools get `input_schema`, tool calls and tool results become `tool_use` and
  `tool_result` blocks, and `max_tokens` is the request's own or `SHAKEDOWN_ANTHROPIC_MAX_TOKENS`. Nothing else is
  carried over, so sampling fields such as `temperature` are dropped, and a forced tool choice becomes `auto`. The
  answer is translated back: `stop_reason` `end_turn` is `stop`, `tool_use` is `tool_calls`, `max_tokens` is
  `length` and `refusal` is `content_filter`; usage becomes prompt (the whole input), completion and total
  tokens, with `input_tokens`, `cache_creation_input_tokens` and `cache_read_input_tokens` kept beside them (Bedrock's
  Converse counts under the same names), so cache writes and cache hits are priced at their own rates.
- CLI agents' calls through `messages` and `responses` (see "HTTP API" above) are not translated: they go to a
  provider that speaks that API as they came, and are recorded raw.
- Every call is recorded in the canonical (OpenAI) form, with `provider` (the id that answered) and, when the
  provider's own answer differs from the canonical one (Anthropic), `provider_response`. A 401 or 403 answer is
  recorded without its error message, which may quote the key.
- A provider's error status is passed on to the agent loop, with a `retry-after`: the provider's own, else for a
  429 the time until its rate limit resets (its `x-ratelimit-reset` header, or the one OpenRouter quotes in its
  error body); no answer is a 502. The loop tries a call that got 503 or 529 again up to 3 times, and one that
  got 429 up to 6 times (enough to outlast a per-minute limit), each after that `retry-after` or else 1 s,
  doubling each time, at most 30 s. Every try is recorded, and the timeline marks a rate-limited one as
  `Model call rate-limited (429)`. A call that still fails, or fails another way, ends the agent with the
  provider's message; a rate limit's says `rate-limited by the model provider (7 tries over 61 s)` first.
- A model that refuses (`content_filter`, Claude's `refusal`) ends the agent as failed: `the model refused`,
  with the category when Claude gives one.

Cost per provider: OpenRouter's is the cost it reports (`cost_source: "openrouter"`). OpenAI and Anthropic
report no price, so the call records tokens (`cost_usd: null`) and the metrics price them at `docs/prices.toml`'s list
prices for the models it lists (`list-price estimate`), else the UI shows `unknown`. A local server's cost is 0 with
`cost_source: "local (no API charge)"`; the UI shows `no API charge`.

## AI review

Optional. A real model reads a compact digest of the record and answers in JSON, which shaked checks:

- **What it reads.** For a branch: the task and its ticket, the team (each agent's kind, model and prompt) and
  runner, the fork's parent and point, the checks (each with its detail; safety flagged), refused and failed tool
  calls with their errors, each agent's start, end, reads per tool and writes, handoff notes and messages, the
  scenario's disruptions, the world's events (alarming ones first), the agents' writes (arguments and results
  trimmed; the world's own steps, such as an approval system's decisions, only counted) and the metrics. Every
  item carries its step (`seq`). It is deterministic and at most 40 000 bytes: long lists are capped (keeping how
  they start and end, and how many were left out) and texts trimmed, at three levels, until it fits; the two
  recorded pharma runs in the tests come to about 38 and 25 KB. A comparison sends both branches' digests (at
  most 26 000 bytes each) and what the comparison found: check counts, the checks whose outcome differs, the first
  divergent step, metrics side by side and the final worlds' differences per kind of row with a sample of changed
  fields. Two suites: each deployment's team, the figures B against A, and each check whose pass rate moved with
  example runs where it passed and failed.
- **The reviewer.** Claude Code is run as `claude -p --output-format json --tools "" --strict-mcp-config
  --setting-sources "" --no-session-persistence`, with the reviewer's system prompt, the answer's JSON schema
  (`--json-schema`) and the digest on stdin, in a fresh folder, with the same allowlisted environment as the CLI
  agents. A provider gets one chat-completions call. The cost is Claude Code's own (API-equivalent) figure, the
  provider's, or the list price of `docs/prices.toml`.
- **What is kept.** The answer is validated (unknown enum values read leniently and said in `issues`, lists
  bounded), and every cited step must be a step of the branch (of A for `steps_a`, of B for `steps_b`; a suite
  review's runs must be runs of that suite): others are dropped and listed in `issues`, and an item whose every
  citation was dropped is marked `unverified`. A review is stored per branch, pair of branches or pair of suites,
  and redone only when asked. Reviews running when shaked stops are marked failed when it starts again.
- **In the web UI.** ⋯ → AI review turns it off in that browser. On the Run screen and the run page a
  Review card shows under the result; a run watched to its end there is reviewed at once, an older one has a
  "Review this run" button. Each cited step selects it; a prompt or team suggestion has "Rewind and try this",
  which opens "Change and rerun" at the suggested step with the text added to the agent's prompt. The compare pages
  show "What matters" above the outcome, reviewed when the page opens.

### Ask about a step

On the run page's step panel, **Ask about this step** sends a question about that step ("Why did the agent make
this reasoning statement?") to a model, the *explainer*, which answers from what the agent had at that step:

- **What it reads.** The step; the ticket and the agent's prompt; the messages sent to the agent's model for the
  step (the model call a thought names, the step itself for a model call, else the agent's latest call before it:
  its system prompt, the conversation and the tool results), that call's response; the 40 steps before it and the
  run's scenario. Each is a *source* with an id (`s<seq>`, `m<seq>.<i>`, `m<seq>.response`, `ticket`, `prompt`,
  `scenario`), its step and its field (`request.messages[0] (system)`). Texts are cut (4 000 bytes; the step 8 000,
  earlier steps 1 200) and the sources kept to 60 000 bytes, the step first, then the ticket and prompt, then the
  call's first and last six messages and its response, then the earlier steps; `sources_left_out` counts the rest.
  The prompt is `core/crates/shaked/src/review/ask_prompt.md`.
- **What is kept.** `answer`, a few plain sentences; `evidence`, each a quote of one source with why it bears on the
  question, kept only when that source's full text has the quote word for word (whitespace aside), with the
  source's step and field: any other is dropped and said in `issues`; `speculation`, the explainer's hypotheses,
  labelled as inference, citing only sources there are. Each question is stored as a review row (key
  `ask:<branch>:<seq>:<id>`, kind `ask`) with who asked (`asked_by`, the layer's principal; `mine` when it was
  you), when, the model, its cost and tokens; hosted, rows are the org's under row-level security.
- **Whose key.** The explainer runs on your own key: the model picked (default the AI review model,
  `SHAKEDOWN_REVIEW_MODEL`) is resolved as agents' providers are, and refused (409) when it would run on the
  layer's fallback key, or, where the layer lends no environment keys (hosted), on the environment's key or
  Claude Code. Locally the environment's keys and Claude Code are yours.
- **From the assistant or MCP.** The operations `questions.ask` (spend: the assistant asks you to confirm; `wait`
  answers once done) and `questions.list`, authorized as the branch's routes are.

Checked on the pharma demo (2026-11-08) with Claude Code Sonnet: the failed six-agent haiku run ($0.11, 19 s),
the single Sonnet agent ($0.07, 9 s) and their comparison ($0.10, 13 s for the model after about 45 s comparing
the final worlds). The unit tests build digests, prompts and checks on those two recorded runs; the live test
(`live_review_of_a_recorded_run_is_valid_and_cites_real_steps`) asks haiku for a review and checks it.

## Disk

What uses space: every save of every part, and the databases or files a part keeps outside `SHAKEDOWN_SAVES`. How
big a save is depends on the integration (see its README): a `postgres` save is a full copy of the database
(about 21 MB for the retail example), a `mariadb` save is a binary-log position. Runs and forks add up until you
delete the run. The Rust build directory `core/target` also grows; `cd core && cargo clean` frees it.

- The top bar shows `Saves <used> / <limit> GB`: each integration's `usage` plus the size of `SHAKEDOWN_SAVES`.
  When it reaches the limit, new runs and forks are refused (HTTP 507) until you delete old runs. The limit is
  `SHAKEDOWN_MAX_DB_BYTES`.
- Delete a run from the Runs page (Delete run, then confirm), or
  `curl -X DELETE http://127.0.0.1:4510/api/runs/<run_id>`. This drops its saves, stops its live instances and
  removes its recorded events. The shared base and gold saves stay. A run with a running branch can't be
  deleted.
- At startup shaked asks each integration of the benchmark's world to drop saves that no recorded event names
  (left by crashes), and logs `gc: dropped N saves (X MB)`. An integration without `gc` is skipped.
  `POST /api/gc` runs the same cleanup. Neither has a deadline: an integration's `gc` may stop instances, so
  new runs and forks wait (409) until every integration has answered, however long that takes. Start shaked
  only when no test is running, since tests make unreferenced saves.
- The tests drop what they create, even when an assertion fails. `core/scripts/leakcheck.sh` (run from `core`)
  runs each of the core's test binaries and fails if one leaves databases behind: Postgres `pi_`, `ps_`,
  `branch_`, `snap_` and `shakedown_test` databases, and MariaDB `mi_` and `mx_` ones when `LEAKCHECK_MARIADB` names
  the server container.

## Tests

```bash
(cd core && cargo test)                        # needs the Postgres of deploy/compose.yml
cargo test                                     # at the repository root: kit, integrations, benchmarks (some need the Postgres)
cargo test -- --ignored live_                  # live integration tests: they start Docker containers or real servers
(cd integrations/ridgeline-hub && uv run --with pytest pytest -q)    # likewise integrations/isaac-sim and kits/python
(cd core && cargo test -- --ignored live_ --test-threads 1)   # live shaked tests: real models (see below); they cost money
(cd ui && npx vitest run && npm run build)
(cd sdk/python && uv run pytest -q)            # SDK: unit tests, and integration tests against a real shaked
```

The SDK's integration tests start a shaked on the retail example (`SHAKEDOWN_SDK_BIN_DIR`, default
`core/target/release`, then `core/target/debug`; Postgres at `SHAKEDOWN_SDK_PG_URL`, default the compose one) or use
the running one at `SHAKEDOWN_SDK_URL`, and are skipped without either. They drive the API directly and call no model.
The shaked they start runs the startup cleanup, so give them a Postgres no other test run uses at the time.

The tests of `integrations/ridgeline-hub` and `integrations/isaac-sim` read the Ridgeline factory example from
`CopperTide-Examples/factory` next to the `Shakedown` checkout (their READMEs say which folders). **That example is not
public yet**: without it every test of those two packages is skipped, and the packages can't start. The CopperTide
program that `integrations/sculptured` runs is not published yet either; its live test is skipped without it.

Tests never call a real model unless they are `live_`. No test supplies a scripted agent to a run, and nothing
client-facing uses one; some tests use a scripted OpenAI-compatible server standing in for a model
(`core/crates/shaked/tests/support/fake_llm.rs`, for the assistant, provider keys, vLLM-shaped answers and external
runs), a local server standing in for Bedrock (`src/providers/bedrock*`), or a stand-in for an agent's endpoint
(`sdk/python/tests/test_adapters.py`). shaked's `live_` tests
(`core/crates/shaked/tests/support/live.rs`) run on Claude Code (`claude`, signed in) with `SHAKEDOWN_LIVE_MODEL`
(default `haiku`), or on shaked's own agent loop when a provider key is set (`ANTHROPIC_API_KEY`,
`OPENAI_API_KEY`, `OPENROUTER_API_KEY`, or `SHAKEDOWN_LOCAL_MODELS` with `SHAKEDOWN_LIVE_MODEL`); `SHAKEDOWN_LIVE_AGENT=claude`
or `=api` picks one. A test that needs shaked's own loop (its model calls recorded, a fork resuming
mid-conversation, a provider's request and cost) skips without a key, and every live test prints `skipped: …` and
passes when no model is available. Models vary from run to run, so these tests check what Shakedown records and
enforces (every step and its fields, gates, checks, restores), never one path through a task; where a model did not
make the call a check needs, the test prints `note: …`. What the fakes used to cover that a real model can't be
made to do (arguments cut off, a refusal, an overloaded provider, a model list over the bound) is tested on the
functions that handle it. Never start shaked while tests run: the tests make databases that shaked's startup
cleanup would drop.

CI (`.github/workflows/ci.yml`) runs the format, Clippy and test checks of both Cargo workspaces, the Python
kit's tests, an import check of the two Python integrations, and the UI's tests and build, but no `live_`
test. See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full list of
checks before a pull request.

Table-backed item get responses also carry `revision`. Send it on the corresponding put to
reject an edit based on an older item (409); independent items can be saved concurrently.

The existing operation registry classifies history as edit access and export as spend access.
Project history and export perform no writes and invoke no models, but retain those access classes.

### Hosted project selection

Layers advertising the `projects` feature provide `GET /api/projects` (`projects: [{id,name}]`,
`current: id | null`), `POST /api/projects` (`{name}`; returns `{id}` and selects it), and
`POST /api/projects/{id}/select` (returns `{id}`). Selection is remembered per user. These routes
are provided by the hosted layer, not by a local shaked. The UI reloads after successful selection.
