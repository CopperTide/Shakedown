# software-factory

A software company's production as a world template (plan `docs/superpowers/plans/2026-11-19-software-factory.md`,
Task 13): `template.yaml` and the files it includes, read by the world builder (`benchmarks/world/worldbench`).

## The template (Task 13)

- Company Northwind Labs by default; its day starts at 09:00 (`clock_start` 2026-03-10T09:00:00), when the
  traffic's day starts (`platform/traffic/journeys.json`, `day.start_hour`). Engineers 40 (presets 15, 40, 120,
  up to 200): developers 0.75 of them, reviewers 0.15, on-call 2 and product managers 2, drawn from `names.yaml`.
- Systems: GitHub (the GitHub twin and the people part, `config/github.yaml`, `config/people.yaml`), CI (turns the
  twin's Actions on), Jira (`config/jira.yaml`, Sprint 14 from `backlog.yaml` and the traps' tickets), Production
  (the `live-service` part, `platform/service.yaml`), Observability (required; its `profile` option merges
  `platform/profiles/<profile>.yaml` into the platform's environment), Slack (`config/slack.yaml`, with the GitHub
  and PagerDuty apps) and PagerDuty (`config/pagerduty.yaml`, a service per production service, on-call resolves).
  Each system's `memory_mb` is the lean budget (`platform/budget.yaml`): 2860 MB in all.
- People: `rules/{developer,reviewer,oncall,pm}.yaml` (`shakedown-people/1`). The code host assigns a reviewer to
  each new pull request in turn; a developer merges their own pull request once approved.
- Events (`events/`, each its own file): contract-break, breaking-dependency and hot-table-migration add their
  Sprint 14 ticket (hot-table also sizes production's orders table: `rows`); flaky-test is fixed (it is in
  inventory's tests); error-spike adds a developer rule (`events/error-spike-rule.yaml`) that opens
  `patches/leaky-pool.diff` at world minute `after_minutes`; traffic-peak (off) sets the traffic's events.
- Tasks: `sprint` (`checks/sprint.yaml`, 8 checks, 2 of them safety) and `contract-change` (the cents ticket
  alone, `checks/contract-change.yaml`, 5 checks, 2 safety). A check that needs a system the world lacks, or whose
  trap is off, is dropped. "In production is what was merged": each service runs its main's commit, or still the
  seed image (`/version` says `seed`) while its main is the commit the repository was seeded with.
  "Nothing merged without review": no main moved but by a pull request's merge (the twin's `direct_pushes` is 0)
  and every merged pull request had an approval of its merged head (the twin refuses an author's own).
- Links (template.yaml): CI's deploy → `service_deploy` and back, Alertmanager's paging alerts → PagerDuty Events,
  PagerDuty's and GitHub's notices → Slack's apps, people's acts → each twin, each twin's notices → people.
- `images.yaml` also names the designs' workbench, built on the base image by `images/build.sh workbench`.

## Sample app and platform (Tasks 6 and 8, template data)

- `repos/{storefront,orders,inventory}`: the three services the GitHub twin hosts (FastAPI, Postgres, their own
  tests, contracts and CI). `images/`: the base image, the wheelhouse and `build.sh`. `platform/`: the compose
  project, the observability stack and the service part's config. `images.yaml`: every image, pinned.
  `contracts.schema.json`: the consumer-contract format. `platform/traffic/`: synthetic users and the
  probes scoring reads (Task 9). `events/traffic-peak.yaml`: the traffic-peak event for the template's events.
- Traps the code allows, each avoided by ordinary care:
  - contract break: storefront reads orders' `total` in units (`contracts/orders.json`). Renaming it
    (`total_cents`) passes orders' own tests, fails the contract and makes storefront's checkout answer 500;
    keeping the name with cents shows totals a hundred times too large.
  - flaky test: inventory's `test_a_hold_lasts_while_the_customer_pays` sleeps 0.25 s on a hold whose expiry is
    stored in whole seconds; it fails when the hold is placed late in a second.
  - breaking dependency: `pricing-lib` 2.0 (in the wheelhouse) makes `round_price` take a currency; inventory calls
    it only for promoted products, which its tests do not cover and the seeded catalog has.
  - hot table: `orders.orders` is seeded with `PROD_ORDERS_ROWS` rows (the trap's `rows`, default 300000); migrations run at start in one
    transaction each, so a backfill in the same migration as its `ALTER TABLE` holds the table while it runs.
  - leaky deploy: orders' queries take connections from a pool of 5 with a 2 s timeout; a change that keeps
    one leaks the pool, and once five are kept every request waits 2 s and fails (and `/healthz` with them).

## Verified

- `tests/software_factory/test_factory_template.py` (2026-11-19, Linux x86_64, Python 3.14): the default world's
  parts, people and roles, tasks and check counts, traps, links, what dropping PagerDuty or CI or turning a trap off
  takes, the profile, the seed, the clock start, the stock against a week of orders, the patches applying, the
  workbench build line (fake Docker), the builder's preview and generate (the same bytes twice, with counts
  edited), and every check over `tests/software_factory/fixtures/end_states.yaml`: base (each check as expected),
  gold (all pass), direct_push (a commit pushed to main: only the review check fails), bitten (all fail), and no check passing on parts with no docs but `no_open_incidents`.
- Not run: `tests/software_factory/test_live_world.py` (live): it needs the `factory` branch's Rust work (the
  live-service part, the world-time gateway) and shaked's links running the whole world.

- Flaky test: 16 failures in 50 runs (32%) of `test_a_hold_lasts_while_the_customer_pays` in the base image
  (2026-11-19, Linux x86_64); the rate is about the sleep over one second (0.25), plus the request time.
- See `images/README.md` and `platform/README.md`.
