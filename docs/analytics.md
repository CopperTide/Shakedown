# Analytics

The Analytics page (sidebar, under Experiments) shows how your agent setups do across every run you can see:
how often they pass, what they cost and how long they take. The same figures are `GET /api/analytics/summary`
and the `analytics.summary` operation, so the assistant and any agent over MCP can read them too
([reference.md](reference.md)).

What was checked: the aggregates on fixture runs (pass rate, cost per passed run, the leaderboard, every filter,
figures worked out again when a run ends again) and the operation over HTTP
(`core/crates/shaked/tests/analytics.rs`); the page's filters in the address, its empty states and its drill-down
links (`ui/src/features/analytics/*.test.*`).

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
