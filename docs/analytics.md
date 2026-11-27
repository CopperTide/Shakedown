# Analytics

The Analytics page (sidebar, under Experiments) shows how your agent setups do across every run you can see:
how often they pass, what they cost and how long they take. The same figures are `GET /api/analytics/summary`
and the `analytics.summary` operation, so the assistant and any agent over MCP can read them too
([reference.md](reference.md)). Its **Search** tab finds any step of any run by its words
([Search](#search)), and its **Explore** tab answers your own questions: measures by dimensions, or read-only SQL
([Explore](#explore)).

What was checked: the aggregates on fixture runs (pass rate, cost per passed run, the leaderboard, every filter,
figures worked out again when a run ends again) and the operation over HTTP
(`core/crates/shaked/tests/analytics.rs`); the page's filters in the address, its empty states and its drill-down
links (`ui/src/features/analytics/*.test.*`). Search: what a step is found by, the snippet, every filter, pages, the
planner using the index and the operation over HTTP (`core/crates/shaked/tests/search.rs`); the address, the API
query, snippets, grouping by run and the links to steps (`ui/src/features/analytics/search/*.test.*`).

## What counts

- **A run is counted once**: its first branch. Reruns (forks) are left out, since a rerun starts part-way and
  often with changes.
- **Only finished runs count.** A run still going counts once it ends; the page says how many are going.
- **Judged runs** are the ones that passed, failed or went over budget. Stopped runs, interrupted runs and
  platform failures (`infra_error`) are counted as runs but left out of the pass rate and the run time: they
  say nothing about the agents.
- **A run's model** is the model whose agents used the most tokens in it, else its setup's first agent's model.
- **Cost** is the run's cost as its run page shows it: what the provider reported, else a list-price estimate.
  A run whose cost is not known is left out of the cost figures, and the Total cost tile says how many were.

## The overview

| Part | What it shows |
|---|---|
| Runs | Finished runs in the filter, and how many of them were not judged. |
| Passed | Runs that passed, of the judged ones. |
| Pass rate | Passed ÷ judged. |
| Total cost | The runs' cost added up. |
| Cost per passed run | Every run's cost (failures included) ÷ the runs that passed: what one pass costs. |
| Median run time | The median wall-clock time of the judged runs. |
| Pass rate over time | Pass rate per day (per week when the range is over 90 days), in UTC, one line per setup, or per model with the toggle. The five setups (or models) with the most runs are drawn; pick others in the filter. While there are few days each one is a dot; hover for the passes and judged runs behind a dot. |
| Cost against pass rate | One dot per setup (or model): cost per run across, pass rate up, so up and to the left is better. A bigger dot has more runs. Hover for its figures, click it to open its runs. A setup with no known cost or no judged run is listed under the chart instead. |
| Leaderboard | Each setup on each scenario (with its world): runs, pass rate, cost per run, median time and cost per pass. Click a heading to sort, a row to open its runs. |

Colours follow the setup or model, not its rank: each keeps its colour when a filter hides the others.

With no runs yet the page says to deploy an experiment; with no runs in the range or the filter it offers to show
all time or clear the filters. While shaked is still working out the figures of runs that just ended (or, after
an upgrade, of older runs), the page says so and fills in by itself.

## Filters

All of them stay in the page's address, so a link shows the same view.

| Filter | |
|---|---|
| Time range | 7, 30 (the default) or 90 days back to midnight UTC, or all time; by when the run was made. |
| World, Scenario, Setup, Model | Pick one or more values of each (any of them matches); the menus list the values in the time range. |
| One line per | Setup or Model, for the pass-rate lines and the cost dots. |

The API also takes `status` (`passed`, `failed`, `budget_exceeded`, `stopped`, `interrupted`, `infra_error`) and
`to`.

A dot or a leaderboard row opens the runs list narrowed to it (`/runs?setup=…&scenario=…&from=…`), with each run's
reruns under it. The list matches a model by the setup's agents, so a run whose most-used model differs from its
setup's may appear there but not under that model here.

## How it is computed

Reading every event of a long run at query time takes seconds (its model calls' bodies are most of its record),
so shaked works out each ended branch's figures once, from its record without those bodies, and keeps them on the
branch (`branches.stats`). A background job does this for every branch that ends, and for older runs after an
upgrade; a branch that ends again (resumed, or its status changed) is worked out again. The summary then
aggregates those small rows in SQL, in the caller's own transaction, so it counts only the runs the caller may see.

## Search

Analytics › **Search** (or type in the ⌘K palette and pick *Search every run's steps for …*) finds any step of
any run you can see by its words, newest first, grouped by run. Click a step to open its run page at that step
(`/runs/<run>/<branch>?step=N`). The same search is `GET /api/analytics/search` and the `analytics.search`
operation, so the assistant and any agent over MCP can search too.

| Kind of step | Found by |
|---|---|
| Reasoning | What the agent thought or said it would do. |
| Message, Report, Delegation, Handoff | The text sent, the task given, the note left. |
| Tool call | The tool's name, its arguments, its result and its error (a refused call is found by why it was refused). |
| Wake, Turn end, Finish, Agent end | The input a turn started with, what an agent said as it ended its turn, the lead's summary, an agent's error. |
| Check | The checks' names and details. |

Model calls (whose text the reasoning steps already carry) and world events are not searched.

- **Words** match their other forms (*crew* finds *crews*). `"exact phrase"` for a phrase, `or` for either word,
  `-word` to leave one out. Identifiers such as flight numbers and ids are found as written.
- **Kind and Agent** chips narrow the results; each shows how many steps match, whatever else is picked. They stay
  in the address (`/analytics?tab=search&q=crew&kind=reasoning&agent=ops-controller`), so a link shows the same search.
- **Pages:** 50 steps at a time, *Show more* for the next. Matches are counted up to 10,000.
- The API also takes `run` and `experiment` (ids) to search within some runs.

Each step is found through a full-text index on the events (migration shakedown 8): `shakedown.step_text(kind,
payload)` picks the text above (at most 60,000 characters of a step) and a GIN index covers
`to_tsvector('english', …)` of it. It is an expression index, not a stored column, so adding it rewrites no event;
building it reads the events table once while shaked starts (about 4 seconds for 40,000 events, a 5 MB index). The
snippet is made only for the page of results shown, and the search runs in the caller's own transaction, so it
finds only the runs the caller may see.

## Explore

Analytics › **Explore** answers the questions the overview doesn't: *what does each agent cost*, *which tools get
refused*, *how many tokens does each model use per day*. It has three modes, all kept in the address so a link
reopens the same view: **Builder**, **SQL** and **Saved**.

### Builder

Pick one or more **measures**, up to two **dimensions** (*By*, *then by*) and the filters; the table and the chart
update as you pick. It opens on cost and runs by agent over the last 30 days, with one-click presets beside it
(cost by agent, refusals by tool, tokens by model, pass rate by setup, errors by step kind, cost per day by agent).

| Measure | What it adds up |
|---|---|
| Runs | Runs counted. By a step-level dimension (agent, model, tool, step kind): the runs it took part in. |
| Pass rate | Runs that passed, of those judged (passed, failed, over budget). |
| Cost | What the model calls cost in USD: as the provider reported it, else at list price. |
| Input / Output / Cached tokens | The model calls' prompt tokens (cached ones included), output tokens, and prompt tokens read from the provider's cache. |
| Model calls, Tool calls, Steps | How many there were (steps leave out bookkeeping: world views, save points, imported spans). |
| Refusals | Tool calls a gate or decision right refused (the error says *refused*, or *not available to this agent*). |
| Errors | Tool calls that failed otherwise, and turns or agents that ended in an error. |
| Avg run time | Mean wall-clock seconds of a run. |
| Thinking time | Estimated seconds waiting on models: a call is recorded when its answer comes back, so the time since its agent's previous step is taken as the call's (as on the run page's Agent tab). |

| Dimension | Level | What it is |
|---|---|---|
| Setup, Scenario, World, Experiment, Day | run | The run's setup, scenario, world, experiment, and the day it started (UTC). |
| Agent | step | The agent that took the step; *(none)* for the world's own steps. |
| Model | step | The model that made the call; for other steps, the model the agent's setup gives it. |
| Tool | step | A tool call's tool; *(none)* for other steps. |
| Step kind | step | model_call, tool_call, message, reasoning, … |

Any measure goes with any dimension. Filters: time range (7, 30, 90 days or all), experiment, setup and agent (the
runs an agent took part in, and only its steps). The same runs count as on the overview: each run's first branch,
finished, with its figures worked out.

The chart draws one measure (pick it when there are several) as bars, stacked bars (the second dimension stacked)
or a line; the table shows every measure and downloads as CSV. **Every value drills through**: click a cell (or a
bar) to list the runs behind it, or, for a step-level measure or dimension, the steps behind it, each opening its
run page (`/runs/<run>/<branch>`, or at the step with `?step=N`). At most 200 are listed, newest first.

### SQL

For anything the builder can't say, write one read-only `SELECT` (or `WITH … SELECT`) over five views of schema
`explore`. Click a view or column in the reference to insert it, start from an example, and press Ctrl/⌘+Enter to
run. The result shows as a table (charted with a column for x and numeric columns for y) and downloads as CSV.

| View | One row per |
|---|---|
| `runs` | run branch: `run_id`, `branch_id`, `parent_branch_id` (set for a rerun or fork), `name`, `world`, `scenario`, `setup`, `model`, `experiment_id`, `experiment`, `status`, `passed`, `started_at`, `ended_at`, `seconds`, `cost_usd`, `tokens` |
| `steps` | step: `run_id`, `branch_id`, `step`, `at`, `agent`, `kind`, `tool`, `outcome` (ok, refused, error), `text` (up to 500 characters; never a model call's body), `inherited` |
| `model_calls` | model call of an ended branch: `agent`, `provider`, `model`, `input_tokens`, `output_tokens`, `cached_tokens`, `reasoning_tokens`, `cost_usd`, `think_ms` (usage only: no prompt or answer) |
| `tool_calls` | tool call: `agent`, `tool`, `outcome`, `error` (up to 500 characters), `duration_ms`, `part` |
| `agents` | agent of a run branch's setup: `agent`, `name`, `kind`, `model`, and its own `model_calls`, `input_tokens`, `output_tokens`, `cached_tokens`, `cost_usd`, `think_seconds` |

Every column is described in the database (`COMMENT ON`), and `GET /api/explore/schema` lists them with the
functions a query may call and the limits. Steps join on `(branch_id, step)`; the run page is
`/runs/<run_id>/<branch_id>?step=<step>`.

A query runs only behind several walls:

1. **A real SQL parser** (sqlparser, PostgreSQL dialect) must read it as exactly one `SELECT`: no writes, DDL,
   `COPY`, `SET`, `EXPLAIN`, `SELECT INTO`, row locks or second statement. It may name only the five views (or its
   own CTEs) and call only allowlisted functions (aggregates, window functions, and number, text, time and JSON
   functions; nothing that reads files, settings or the catalog, sleeps or changes anything). What runs is the
   parsed query printed back, never the text as sent.
2. **A read-only transaction** (`SET TRANSACTION READ ONLY`) with a **5 s statement timeout** and a 1 s lock
   timeout.
3. **The search_path is pinned** to `explore, pg_catalog`.
4. **The role `shakedown_explore`** (`SET LOCAL ROLE`), which may read the explore views and nothing else: the
   views read the tables as their owner. Migration shakedown 9 makes it when the migrating role may make roles;
   without it shaked runs SQL as itself, behind walls 1–3.
5. **Caps**: at most 5,000 rows, 8 MB of results and 100 columns; a longer result is cut and says so.

The query runs in the caller's own transaction, so a hosting layer's row-level security applies to it as to
every other read.

### Saved views

*Save view* keeps the builder's query (with its relative time range and chart) or the SQL query under a name;
**Saved** lists them, last changed first, to reopen or delete. They live in the `saved_views` table.

### How it is computed

Model-call bodies are most of a long run's record, so neither the builder nor SQL reads them. The analytics job
that works out each ended branch's figures also keeps one lean row per model call in `call_usage` (tokens, cost,
think time), in the same transaction; the builder joins it to the run's steps. Migration shakedown 9 adds
`call_usage`, `saved_views`, the functions `shakedown.step_tool` and `shakedown.step_outcome`, the `explore`
views and the role. The figures' version went from 2 to 3, so every ended branch is worked out once more after
the upgrade (the Overview says so while it happens). A builder query or drill runs in the caller's transaction
with a 10 s timeout; on the airline demo (6 runs, 43,000 steps) cost and refusals by agent and tool take about
50 ms.

| Operation | Route | Access |
|---|---|---|
| `explore.schema` | `GET /api/explore/schema` | read |
| `explore.query` | `POST /api/explore/query` | read |
| `explore.drill` | `POST /api/explore/drill` | read |
| `explore.sql` | `POST /api/explore/sql` | read |
| `explore.list`, `explore.get` | `GET /api/explore/views`, `GET /api/explore/views/{id}` | read |
| `explore.save` | `POST /api/explore/views` | write |
| `explore.delete` | `DELETE /api/explore/views/{id}` | delete |

What was checked: per-call usage, cost at list price and think time (`analytics/usage_tests.rs`); the builder's SQL
and the guard (`analytics/explore/*_tests.rs`, `docs.rs`: every example passes the guard); measures by run and
step dimensions, drill-through to runs and steps, SQL over the views, the read-only transaction, the timeout and
the role behind the guard, saved views, and usage deleted with its run, all through the operations over HTTP
(`core/crates/shaked/tests/explore.rs`); the Explore tab's address, CSV, chart data and drill dialog
(`ui/src/features/analytics/explore/*.test.*`).

## Additional charts (Slice 2)

The overview also shows these views, using the same URL filters and the same first-branch population:

| View | Meaning |
|---|---|
| Failed checks | Number of runs whose final scoring result explicitly failed each named check. Repeated occurrences within a run count once. A run can fail several checks, so bars are not additive. |
| Refused tools / gates | Number of runs with a recorded refusal for each tool, counted once per tool per run. The event record does not identify individual gate IDs. Refusals may occur in runs that ultimately pass. |
| Non-passing outcomes | Recorded terminal status categories for runs that did not pass; these are outcomes, not inferred root causes or raw error messages. |
| Cost over time | Total known cost per UTC day/week across the selected runs. Unknown costs are excluded and counted explicitly; an all-unknown bucket is missing, not zero. |
| Tokens over time | Total recorded tokens per UTC day/week across the selected runs. |
| Run-time distribution | Judged runs in labeled duration ranges: under 10 seconds, 10–30 seconds, 30–60 seconds, 1–5 minutes, 5–15 minutes, 15–60 minutes, and one hour or longer. Bounds are lower-inclusive and upper-exclusive. Bar length represents run count, not time-range width. |

Cost/token timelines total across the selected setups/models; the Setup/Model toggle still controls the
original per-group pass-rate and scatter charts. Exact chart values are available without hover through
labels and expandable data tables. On narrow screens the new panels form one column.

The response extends each `series` point with `runs`, nullable `cost_usd`, `uncosted`, and `tokens`, and adds
`failures` (`checks`, `gates`, `reasons`, each containing `name`/`runs`) and `duration` bins
(`lower_seconds`, nullable `upper_seconds`, `runs`). No new endpoint or database migration is needed.
Cached branch stats are versioned so old runs are recomputed by the existing background job after upgrade.
The summary continues to aggregate in the caller's database transaction under row-level security.

### Slice 2 verification

Analytics extraction unit tests and Postgres integration tests cover final check names, repeated refusals,
first-branch counting, shared filters, empty results, duration boundaries, unknown/zero/mixed costs, and
version-1 cache backfill. Rust checks run on the build box. UI tests cover bucket totals, missing costs,
labels and responsive geometry; TypeScript, lint and a production build also pass. Desktop (1280 px)
and mobile (390 px) browser checks used explicitly labeled synthetic fixtures, including the expandable
exact-value tables. Hosted organization RLS is preserved through the existing caller transaction; a
closed-source multi-tenant test harness was not run as part of this slice.
