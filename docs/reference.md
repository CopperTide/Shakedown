# Reference

shaked's HTTP API, its settings, what uses disk, and how to run the tests. For the ideas behind them see
[concepts.md](concepts.md).

## HTTP API

All JSON on `http://127.0.0.1:4510`, except the event stream. Errors are `{error}` with a status code.

| Route | What it does |
|---|---|
| `GET /api/health` | `{open_available, providers, claude, codex, benchmark, disk: {bytes, limit_bytes}}`. `providers` is each model provider as the configuration alone tells it, in the shape of `/api/providers` without `models`; a local server is not asked, so it is `available: false, checked: false` with why `not checked (GET /api/providers asks the server)`. `open_available` is true when some provider can take calls, judged from the configuration (a configured local server counts, up or not) |
| `GET /api/providers` | `[{id, kind, label, available, checked, why?, models?}]`, one per model provider, your own keys applied; see [Model providers](#model-providers) |
| `GET /api/me` | `{mode, user: {name}, features}`: `mode` is `local` and `user.name` `You` for an open-source shaked; a deployment's layer may add `account` (the UI's account menu) |
| `GET /api/project` | `project.status`: `{name, counts: {worlds, scenarios, setups, experiments}, problems, root, imports}` for the project folder `SHAKEDOWN_PROJECT` names; 409 when none is open. `problems` lists unreadable files and broken references (`{kind, id, pointer, message}`); a problem never stops the rest of the project from loading. `imports` lists the project's git imports, `[{id, url, ref, commit, imported_at, packages, items}]` |
| `POST /api/project` | `project.init`: `{name, git?}` creates a project in the configured folder; 409 when one is already there |
| `POST /api/project/import-git` | `project.import_git`: `{url, ref?, id?, confirm?}` imports a git repo, its packages and its project items (see [Import from git](#import-from-git)). Without `confirm` the ref is fetched and the plan answered, and nothing is installed, registered or written: `{proposed: true, id, url, ref, commit, checkout, project: {name, problems}, packages: [{name, kind, dir, python}], items: [{kind, id, label, action: write\|replace\|same\|keep, reason}], views: [{world, file}], compose: [{file, services, volumes, part}], python: [{root, env}], edits, notes, problems, conflicts, previous_commit}`. With `confirm` the same, plus `registered` (true when shaked runs) and `written`. 400 for a URL other than https, ssh, `git@host:path`, `file://` or an absolute path, or a ref starting with `-`; 409 when a package name is installed already; 422 when git or uv fails; 403 on a shaked that serves others |
| `GET /api/project/{worlds\|scenarios\|setups\|experiments}` | `{items: [{kind, id, label, hash, problems}]}` |
| `GET /api/project/{kind}/{id}` | one item with its `body` (the file's contents as JSON) |
| `PUT /api/project/{kind}/{id}` | `{body, confirm?}` creates or replaces the item's file. `body` is checked exactly like the file: unknown fields, a wrong `format:` and credentials written inline are 422 with a JSON pointer (a credential's value is never echoed). Unless `confirm` is true and while the project's `permissions.edit` is `diff_first` (the default), nothing is written and the answer is `{proposed: true, diff}`. Writes are atomic: a file is never left half-written. Needs the `admin` token scope |
| `DELETE /api/project/{kind}/{id}` | deletes the item's file (a world keeps its data folder) |
| `POST /mcp` | the control MCP server (JSON-RPC, protocol `2025-06-18`): every operation above as a tool (`worlds_list`, `scenarios_put`, …), each checked against the caller's token like its route. See [Use Shakedown from an agent](#use-shakedown-from-an-agent-mcp) |
| `GET /api/tokens` | the live API tokens, `[{id, name, prefix, scopes, created_at, last_used_at, expires_at}]`: never a token or its hash |
| `POST /api/tokens` | `{name, scopes: ["read"\|"run"\|"admin"], expires_days?}`: `{id, token, prefix}`, the token (`sd_…`) shown this once. `read` views; `run` also starts, forks, stops and records; `admin` does everything, model keys and tokens included |
| `DELETE /api/tokens/{id}` | revokes a token; calls with it then get 401 |
| `GET /api/models/keys` | your model keys and servers, `[{id, name, kind, base_url, model, key_last4, checked_ok, checked_at, check_detail, created_at}]`: never the key |
| `POST /api/models/keys` | `{name, kind, secret, base_url?, model?}`: adds one (`kind` `anthropic`, `openai`, `openrouter`, `bedrock` or `custom`). A `custom` server needs `base_url` and is picked by agents as `custom:<name>`; Bedrock's `secret` is JSON `{access_key_id, secret_access_key, session_token?}` and `base_url` its region. 409 when the name is taken |
| `POST /api/models/keys/{id}/secret` | `{secret}`: replaces a key's secret |
| `POST /api/models/keys/{id}/test` | a one-token call with the key; the key with `checked_ok` and `check_detail` (the provider's reason, never the key) |
| `DELETE /api/models/keys/{id}` | deletes a key |
| `GET /api/tasks` | the benchmark's tasks, `[{id, reason, ticket, instructions, clock_start?}]` (`instructions` is the benchmark's, the same for every task; `clock_start`, the world's date and time at clock 0 when its world keeps its own calendar: agents are told the world's date, see [protocol](protocol.md#a-tasks-clock)) |
| `GET /api/benchmark` | `{name, version, tasks: [{id, reason}], scenario_options, view}`: `scenario_options` is the benchmark's `describe.scenario_options` (null when it offers none), the disruptions a run's scenario may add ([protocol](protocol.md#a-runs-scenario)); `view` is true when the benchmark draws the world's view |
| `GET /api/templates` | the world builder's templates, `[{id, name, summary, version, systems: [label]}]` ([protocol](protocol.md#builder-calls)); 404 `no world builder is installed` when no package of `kind = "builder"` is (with several, the first by name answers) |
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
| `POST /api/runs` | `{task_id, team, runner?, scenario?, seed?, benchmark?}`: start a run; returns `{run_id, branch_id}`. `benchmark` is the benchmark it runs on (the page's world; absent, the current one, read once when the run is asked for), its task and scenario checked against it; 409 when no benchmark of that name is installed. `max_cost_usd` (optional, a positive number) is the run's model-spend limit ([concepts](concepts.md#spend-limits)); absent, `SHAKEDOWN_MAX_COST_USD` applies; 400 for zero or less. `world_minutes` (optional, more than 0 and at most 1440; else 400 "world_minutes must be from 0 to 1440 minutes of world time") is an external or Code run's world time window ([concepts](concepts.md#world-time-windows)), recorded in `RunStart.world_minutes`; shaked's own runners ignore it. `seed` (0 to 2^53 - 1) is the seed the run's world starts its parts with, recorded in `RunStart.seed`; random when absent (every run starts from the same seeded base world either way; the seed reaches parts that draw from it, such as simulators). `scenario` is `{disruptions: [{id, params?: {name: value}}], clock?}`, options of `GET /api/benchmark`'s `scenario_options` (params left out take their defaults): shaked expands each with its template, adds it to the task's setup and records the expanded scenario in `RunStart` (`scenario`); 400 for an unknown option or param, a value out of range or not among the choices, or an option chosen twice. `clock` (`{read_ms, write_ms, tools?: {name: ms}, epoch_ms?, horizon_ms?, team_start_ms?}`) runs the branch on a stepped clock ([concepts](concepts.md#stepped-clock)), recorded with the scenario so forks keep it; 400 "stepped clock needs a team with a lead (dispatch runner)" for any other runner (also for an experiment's setup, the error naming it). `timed` (`[{part, disruption}]`) appends each `disruption` as it is to `args.disruptions` of that part's setup call, and `generators` (a list) is set as `args.generators` on every part's setup call ([protocol](protocol.md#a-runs-scenario)). `runner` is `chain` (default: one agent after another), `concurrent` (all at once), `dispatch` (a lead delegates to the team; see concepts, "Who drives the agents") or `external` (see below; the team may be empty and the answer adds `agents`, each agent's endpoints). `connection` (a Code connection's id) makes shaked launch that connection's driver for the run (the runner is then `external`; the answer adds `connection` and `driver`: `launched`, or `failed` when it couldn't start, the run then ending `failed` saying why); 404 for an unknown connection, 415/403 as for `POST /api/connections`. Each agent is `{id, name, kind, model, prompt, provider?, read_only?, lead?, tools?}`, or an `http` agent `{…, url, headers?}`, or a Bedrock agent `{…, kind: "bedrock", model_id, region}` ([agents.md](agents.md)) (`read_only`, default false, gives the agent only read tools; `tools`, a list of world tool names, gives it only those (the runner's talk tools always come), and a call of another world tool is refused; `lead` marks the agent that leads a dispatch run, else the first does); 400 when a dispatch team marks two leads or has a Codex or `http` agent (neither can keep a session across turns), or when an API-mode agent's provider is unknown or unavailable, or has no default model and the agent names none |
| `POST /api/runs/{id}/name` | `{name}` sets an optional display name (1–300 bytes after trimming); 404 for a missing run. `GET /api/runs` includes `name` when set; task ids and event records are unchanged |
| `DELETE /api/runs/{id}` | delete a run and what it saved (409 while a branch runs) |
| `GET /api/branches/{id}` | a branch with its events, `budget` (`{max_cost_usd, estimate_usd}`: its spend limit, and its running estimate while it runs) and `metrics`: `{measured_s, overhead_ms: {save?, restore?, record?, gateway?, gate?, link?, mcp?, wait?, score?, pause?}, overhead_s, overhead_recorded, estimated_s, tokens, cost, agents: [{agent, kind, model, provider, tokens, prompt_tokens, completion_tokens, cost, model_calls, peak_prompt_tokens, compactions, overhead_s}]}`, each `cost` `{usd, label, source, as_of}` (see concepts, "Overhead and estimates"); `peak_prompt_tokens` is the largest `usage.prompt_tokens` of the agent's model calls, `overhead_s` the overhead recorded on its own events |
| `POST /api/branches/{id}/agents` | external runs: `{id, name?, kind?, model?, provider?, prompt?, read_only?}` registers an agent (kind `external` by default), or starts a registered one again; returns `{agent, mcp_url, gateway_url, tools, spec}`. 404 when the branch isn't live, 409 when shaked runs its agents |
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
| `POST /api/branches/{id}/stop` | stops a live branch: its agents are dropped where they are (a CLI agent's process is killed), calls that wait are refused, and it ends `stopped`, unscored; returns `{stopping}` (false when it was stopping already). The stream's final `status` says when it has ended |
| `GET /api/branches/{id}/stream` | Server-Sent Events: each `event`, then the final `status` |
| `GET /api/branches/{id}/rows?seq=N\|final&keys=<kind>/<id>,...[&part=]` | the state of those keys just after step N, or at the branch's end with `seq=final` (its check's final saves; a step at or after the check reads them too); at most 50 keys. A clock part saved before step N is shown moved on to step N's clock: its save is started in a scratch instance, advanced and inspected ([concepts](concepts.md#a-clock-parts-state-at-a-step)) |
| `POST /api/branches/{id}/fork` | `{at_seq, changes: {<agent id>: {prompt?, model?, kind?, provider?, read_only?}}, restore: "exact"\|"approximate", team?}` (default exact); `team` (a run an external system drives only, and not with `changes`; else 400) gives the new branch another team, such as a single agent in place of a team: its agents' `history` holds the events of any agent of the same id, and the fork note says the team changed; returns `{branch_id, start_seq, moved_from, reason}`, and for an external run `agents`: each agent's `{agent, mcp_url, gateway_url, spec, history}` on the new branch, which stays live for the external system (`history` is the agent's recorded events before `start_seq`). The changes are checked on the team the fork would run, as `POST /api/runs` checks a team |
| `GET /api/branches/{id}/views` | `[{seq, event_seq, view}]`: the world's views recorded on the branch, in order (a fork's include those before its fork point, which its record starts with). `seq` is the step the view follows (the `world_view` event's `seq_of`), `event_seq` the view's own event, `view` the benchmark's `TwinView` ([protocol](protocol.md#the-worlds-view)); empty for a benchmark that draws none; 404 for an unknown branch |
| `GET /api/branches/{id}/world-views` | `{world, views: [{id, label, part, styles, panels, screens}], problems: [{file, message}]}`: the declarative views of the project world the run played (the world whose `package` is the run's benchmark), as parsed; `world: null` and no views without one ([protocol](protocol.md#declarative-views-world-files)) |
| `GET /api/branches/{id}/world-views/{view}?screen=<id>&at=live\|final\|<seq>` | one screen of a view evaluated at a point: `{view, screen, at, panels}` ([protocol](protocol.md#declarative-views-world-files)). Without `at`: live while the branch runs, else its end. 400 for a point with no state, 404 for an unknown view or screen, 409 for `live` on a branch that is not running |
| `GET /api/branches/{id}/recording` | `{video, of, sidecar}`: the branch's plant recording, if `SHAKEDOWN_RECORDINGS` holds one (an MP4 whose name has the branch id in it; a fork without one shows its parent's, `of` says whose), with its JSON sidecar; `video` is null when there is none. A recording part writes its frames on Shakedown time, so the video's second t is the branch's Shakedown second t |
| `GET /api/branches/{id}/recording.mp4` | that MP4, with byte ranges (so a player can seek); 404 when there is none |
| `GET /api/branches/{id}/restore-plan?at_seq=N&restore=exact\|approximate` | `{parts: [{part, how, estimate_seconds?}]}`, or 409 with the refusal |
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
| `GET /api/review/settings` | `{enabled, model, why?}`: whether AI review is on and with which reviewer (see [AI review](#ai-review)) |
| `POST /api/branches/{id}/review[?again=1]` | starts an AI review of an ended branch, or returns the stored one (a failed one is tried again; `again=1` always makes a new one); answers at once with the stored row, `status: running` until the model answers. 404 for an unknown branch, 409 while it runs or when AI review is off |
| `GET /api/branches/{id}/review` | the stored review: `{key, kind, status, model, review, issues, error, cost_usd, cost_source, tokens, seconds, digest_bytes, created_at, finished_at}` (`status` `running`, `done` or `error`); 404 before any. `review` is `{verdict, went_wrong: [{title, why, steps, severity, unverified?}], suggestions: [{change, why, kind, agent?, prompt_text?, step?}], went_well, confidence}` |
| `POST`/`GET /api/compare/review?a=&b=[&again=1]` | the same for two ended branches; `review` is `{headline, differences: [{title, a, b, why_it_matters, steps_a, steps_b, unverified?}], which_is_better, why}` |
| `POST`/`GET /api/suites/compare/review?a=&b=[&again=1]` | the same for suite `b` against `a`; `steps_a` and `steps_b` are example runs (branch ids) of each suite |
| `GET /api/world` | the benchmark's parts with what each integration reports (version, restore, clock use, tool names, and `system` when it says what kind of system it is) and whether it answered (each gets 5 s; a silent one shows `did not answer within 5 s`) |
| `GET /api/usage` | `{integrations: [{integration, parts, bytes, error?}], total_bytes, limit_bytes}`: the bytes each integration keeps (its own `usage` plus its save folders) and the world parts that use it. An integration whose `usage` fails or doesn't answer within 5 s has `bytes: 0` and an `error`, and adds nothing to `total_bytes` |
| `POST /api/gc` | runs the startup cleanup and returns `{removed, freed_bytes, errors}`; it keeps every save a recorded run names and the shared base and gold saves; 409 while agents are running, and new runs and forks answer 409 while it runs. It has no deadline: it answers when every integration's `gc` has (an integration that never answers keeps the cleanup on until it exits), and a client that stops waiting does not end it |

New runs and forks answer 507 once the saves reach the disk limit. The `/api` routes answer only
requests whose `Host` is a loopback address. The agents' MCP endpoints (`/mcp/{branch}/{agent}`) and the
API-mode gateway (`/gateway/{branch}/{agent}/v1/chat/completions`) are for shaked's own agents.

## Configuration

Environment variables, read once at startup by shaked (each also under its old name `ARENA_*`, see
[Old names](../README.md#old-names)):

- `SHAKEDOWN_PG_URL`: the Postgres server for shaked's records, passed on to integration programs (default
  `postgres://shakedown:shakedown@127.0.0.1:5544/postgres`, else the same server's old user `arena`).
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
  `arena`).
- `SHAKEDOWN_VAULT_KEY` (64 hex characters) or `SHAKEDOWN_VAULT_KEY_FILE`: the key your model keys are sealed
  with (AES-256-GCM, each bound to its row). Without either, `core/vault.key` is made (mode 0600, ignored by
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
- **A new commit:** a new checkout and environment; the packages are swapped once they are ready (a run going keeps
  its program). The commit before is kept, older ones removed. Fingerprints follow the code and the world folders,
  so old runs keep the fingerprint they recorded.
- **Trust:** the repo's code runs on this machine as a package program, with the cleared environment every
  `command` package gets. The import itself runs only git and `uv sync` (which may run the repo's build backend). A
  shaked that serves others refuses imports (403) until a run's sandbox can run them.

Verified: the tests in `core/crates/shaked/tests/git_import.rs` (a repo made in the test, with one Python part run
through uv, is imported, registered and deployed; a new commit keeps the project's own edits) and, on this machine,
`live_airline_world_imports` against the airline repo (11 packages, 2 worlds with their views, 2 scenarios, 2
setups, 2 experiments, no problems; its benchmark describes itself from the import's environment).

## Use Shakedown from an agent (MCP)

Every operation shaked offers is defined once in its operation registry and served two ways: as an HTTP route
and as a tool of the control MCP server, so the UI, the assistant, scripts and any agent can do the same things.
A test (`core/crates/shaked/tests/parity.rs`) fails if an operation is missing from either.

- On this machine, over stdio: `claude mcp add shakedown -- shaked mcp --project ~/work/my-project` (Codex:
  `[mcp_servers.shakedown] command = "shaked"`, `args = ["mcp", "--project", "…"]`).
- Over HTTP: `POST /mcp` with `Authorization: Bearer <token>`. A `read` token lists and reads; editing or
  deleting project files needs `admin`; starting runs needs `run`.
- Edits follow the project's `permissions` in `shakedown.yaml`: with `edit: diff_first` a put returns a diff and
  writes only when sent again with `confirm: true`.

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
`key:<name>`, and never when that key names its own server.

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
  `length` and `refusal` is `content_filter`; usage becomes prompt, completion and total tokens.
- Every call is recorded in the canonical (OpenAI) form, with `provider` (the id that answered) and, when the
  provider's own answer differs from the canonical one (Anthropic), `provider_response`. A 401 or 403 answer is
  recorded without its error message, which may quote the key.
- A provider's error status is passed on to the agent loop; no answer is a 502. The loop tries a call that got
  429, 503 or 529 again, up to 3 times, after the provider's `retry-after` or else 1 s, doubling each time, at
  most 20 s; every try is recorded. A call that still fails, or fails another way, ends the agent with the
  provider's message.
- A model that refuses (`content_filter`, Claude's `refusal`) ends the agent as failed: `the model refused`,
  with the category when Claude gives one.

Cost per provider: OpenRouter's is the cost it reports (`cost_source: "openrouter"`). OpenAI and Anthropic
report no price and shaked keeps no price table, so their tokens are recorded and the cost is unknown
(`cost_usd: null`); the UI shows `unknown`. A local server's cost is 0 with
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
  (`--json-schema`) and the digest on stdin, in a fresh folder, with the same environment scrubbing as the CLI
  agents. A provider gets one chat-completions call. The cost is Claude Code's own (API-equivalent) figure, the
  provider's, or the list price of `docs/prices.toml`.
- **What is kept.** The answer is validated (unknown enum values read leniently and said in `issues`, lists
  bounded), and every cited step must be a step of the branch (of A for `steps_a`, of B for `steps_b`; a suite
  review's runs must be runs of that suite): others are dropped and listed in `issues`, and an item whose every
  citation was dropped is marked `unverified`. A review is stored per branch, pair of branches or pair of suites,
  and redone only when asked. Reviews running when shaked stops are marked failed when it starts again.
- **In the web UI.** Advanced → AI review turns it off in that browser. On the Run screen and the run page a
  Review card shows under the result; a run watched to its end there is reviewed at once, an older one has a
  "Review this run" button. Each cited step selects it; a prompt or team suggestion has "Rewind and try this",
  which opens "Change and rerun" at the suggested step with the text added to the agent's prompt. The compare pages
  show "What matters" above the outcome, reviewed when the page opens.

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
  `branch_`, `snap_` and `arena_test` databases, and MariaDB `mi_` and `mx_` ones when `LEAKCHECK_MARIADB` names
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

Tests never call a real model unless they are `live_`, and no test uses a fake model server or a scripted
agent: everything an agent does in a test is done by a real model. shaked's `live_` tests
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
