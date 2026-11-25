# World builder (`world-builder`)

Builds Shakedown worlds from templates. A template is a folder of YAML and Markdown that describes a company: the
systems it runs, the simulated people who work in them, the events and traps that can happen, and the tasks a team
of agents is given, each with named checks on the world's end state. The builder fills a template from a user's
choices (company name, size, systems, people, events) and a seed, and writes the result as a benchmark package that
`shaked` installs and runs. It is a builder, not a benchmark with fixed tasks: what a world measures comes from its
template. This package ships two templates, [`small-manufacturer`](library/small-manufacturer/README.md) and
[`software-factory`](templates/software-factory/README.md).

| | |
|---|---|
| Program | `python -m worldbench.builder --templates library` (Python, package `worldbench`, run with `uv`). A built world runs `python -m worldbench --world world.json --version <version>` |
| World | the parts of the systems a user chose, with the template's links between them |
| Tasks | the template's tasks whose systems were chosen: `small-manufacturer` has 1, `software-factory` 2 |
| Scoring | named checks per task, written as data and evaluated on every part's final `inspect` (`scored: true`, no gold world). No model judges. |
| Suggested team | the template's first team, if it has any (`teams`). The rest are offered as alternatives. |

## What it measures

A built world measures what its template's tasks ask. Each task has a ticket (the agents' brief), an optional
setup (world-tool calls made before the agents start) and a list of named checks. When a run ends, `shaked` saves
every part, sends each part's `inspect` to the world's program, and the program evaluates the task's checks with
`shakedown_part.expect.evaluate` ([`docs/protocol.md`](../../docs/protocol.md), "Scoring by named checks"). A check
is `{name, says?, safety?, measure?, check}`, where `check` compares one key's field, counts the keys matching a
glob, or combines checks with `all` and `any`. The run passes when there is at least one check and every check
passes. Checks marked `safety: true` are the ones a team must not break even when it fails the goal.

The world's program answers `describe` with the template's parts, tasks, instructions, teams, links and gates, and
no agent tools of its own (`tools: []`): agents use the tools and HTTP hosts of the parts.

## Where the tasks come from

From templates. The builder reads, on every call:

- its own `templates/` (`software-factory`),
- each folder its command names with `--templates` (the manifest names `library`, which holds `small-manufacturer`),
- each folder in `SHAKEDOWN_TEMPLATES` (`:`-separated).

Each template is a folder `<id>/` with a `template.yaml`. Both shipped templates are written by CopperTide. Their
companies, people and data are fictional: `small-manufacturer` reuses the data and checks of the
[`factory-01`](../factory-01/README.md) benchmark, and `software-factory` is a three-service shop written for this
template.

### The template format

`template.yaml` (`format: shakedown-template/1`) is checked by `worldbench/schema/template.schema.json` (JSON Schema
draft 2020-12) and by the cross-checks in `worldbench/validate.py`: ids are unique per list, the systems a list
names exist, `rules` files exist, sizes and defaults are in range, and an event that is not `fixed` applies
something.

- **Systems** list their parts (`{part, integration, config}`), the Docker images they need, their `memory_mb`, the
  hosts agents reach, and options. A system may be `required`, `default`, or `requires` others. It may also have
  an `apply`: ops run on the world when the system is chosen, before the events' ops.
- **Roles** draw simulated people from the template's name pools by a seeded generator. A role's count is a share
  of the size or a fixed number, never below its `min`. Users may change counts, rename, add and remove people. A
  role is kept when one of its `systems` is chosen.
- **Events** are traps or ordinary events, on or off by default, with params. An event's `apply` ops `set`,
  `append` or `merge` into a part's config, add a `setup` call to every task, or add a `link`. An event may live in
  its own file (`events: ["@events/x.yaml"]`). A `fixed` event is a fact of the template's data (a flaky test in a
  repository, say): it is always on, may apply nothing, and turning it off is a problem.
- **Tasks** have `needs` (systems), a `reason`, a `ticket`, a `setup` and `checks`. A check may name the systems it
  `needs` and the `events` it belongs to. Without them, or with one of those events off, it is dropped.
- **Links** and **gates** go into the world as written, kept only when the systems in their `needs` are chosen.
  Links are filled like the rest. Gates and `scenario_options` are passed through unfilled: their `{arg}`
  placeholders are `shaked`'s.

Tasks, events, links and gates whose `needs` are not all chosen are dropped. Everything dropped is listed, with
why, in the preview's `dropped`.

**Placeholders** in any string: `{company.name}`, `{company.slug}`, `{company.key}`, `{company.domain}`,
`{company.clock_start}`, `{size}`, `{seed}`, `{options.<system>.<name>}`, `{people}`, `{people.<role>}`,
`{people.<role>.<field>}` (that field of each of the role's people, a list), `{param.<name>}` (inside an event) and
`{world_dir}`. `{{` and `}}` are literal braces. A string that is exactly one placeholder takes its value as it is
(a list stays a list).

**Includes**: a string `@<path>` naming a `.yaml`, `.yml` or `.json` file is replaced by the parsed file, a `.md`,
`.txt`, `.diff` or `.patch` file by its text, and a folder (`@folder/`) is copied into the world under `files/` and
replaced by its path there. The path may hold placeholders (`@platform/profiles/{options.observability.profile}.yaml`).
An include never leaves the template folder. A copied folder holds no symbolic links and never holds or sits inside
another copied folder.

**Limits** (`worldbench/limits.py`): each file at most 1,000,000 bytes, 100,000 values and 64 levels deep. Includes
nest at most 8 deep and never in a cycle. One value expands to at most 200,000 values. A copied folder holds at most
2,000 files and 50,000,000 bytes. The schema bounds the size, the name pools and list lengths. A template that cannot be read,
whatever is wrong with it, is logged to stderr and left out of the list.

### Builder calls

The builder answers `templates`, `template`, `preview` and `generate` over the line protocol
([`docs/protocol.md`](../../docs/protocol.md), "Builder calls"). `preview` answers even when the world cannot be
built and lists the `problems`. Images are checked with `docker image inspect` and never pulled: a missing one is a
problem that carries the template's hint (how to pull or build it). `generate` refuses the same problems with
`-32602` and writes nothing.

The same template version, choices and seed give a byte-identical `world.json`. The built package's name is the
company's slug, its version is the template's plus `+` and the first 8 hex digits of `world.json`'s sha256, and its
`shakedown-package.toml` has `opt_in = true` and a `[world]` table (`title`, `template`, `template_name`).

## What it needs

- `uv` and Python 3.11 or later. The package depends on the Python kit at `kits/python` (a path dependency).
- Docker, only for templates whose systems name images. The builder never pulls or builds them.
- The integrations a template's parts name, installed in the same `shaked`.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_TEMPLATES` | unset | `:`-separated folders of more templates. `shaked` passes it to the builder. |
| `SHAKEDOWN_WORLDS` | `core/worlds` | where `shaked` writes built worlds (read by `shaked`, not by the builder) |

A built world's command runs in `${SHAKEDOWN_DIR}/benchmarks/world`, so it needs this checkout of the repository.

## Run it

`shaked` asks the first installed package of `kind = "builder"` (by name) for templates and previews, writes a built
world into `SHAKEDOWN_WORLDS`, installs it, and makes its base saves in the background
([`docs/reference.md`](../../docs/reference.md), `/api/templates` and `/api/worlds`, and
[`docs/packages.md`](../../docs/packages.md), "Templates and built worlds"). Shakedown's UI builds worlds through the
same routes. With `shaked` on its default address:

```bash
curl -s 127.0.0.1:4510/api/templates
curl -s -X POST 127.0.0.1:4510/api/templates/small-manufacturer/preview \
  -H 'content-type: application/json' -d '{"company": {"name": "Live Parts"}}'
curl -s -X POST 127.0.0.1:4510/api/worlds \
  -H 'content-type: application/json' \
  -d '{"template": "small-manufacturer", "choices": {"company": {"name": "Live Parts"}}}'
curl -s 127.0.0.1:4510/api/worlds/live-parts      # state: preparing, then ready
```

Once ready, the world is the current benchmark (unless another was chosen meanwhile), and runs name it by its name
like any benchmark. The choices are `{seed, company: {name}, size, systems, options, people, events}`, every field
optional (the seed defaults to 1).

## How it's tested

```bash
cd benchmarks/world
uv run --with pytest pytest -q                                   # everything but the live tests
(cd ../../core && cargo test -p shaked --test worlds)            # shaked's side, with a fixture builder
(cd ../../core && cargo test -p shaked --test worlds -- --ignored live_)   # this builder, live
```

- `tests/test_*.py` run over the fixture templates in `tests/fixtures/templates/` (`shop`, and two broken ones):
  schema and cross-check errors with their paths, placeholders and includes, the limits, people drawn and edited,
  dropping what a system or event takes away, byte-identical generation, the four builder calls (and `templates`
  over the line protocol), the package `generate` writes, and the built world's `describe` and `score`.
- `tests/test_small_manufacturer.py` scores that template's checks over the real `factory-01-erp` part, after
  Factory-01's gold actions and untouched. See its [README](library/small-manufacturer/README.md).
- `tests/software_factory/` checks the software-factory template's files and world. Its live tests (marker `live`,
  run with `-m live`) need Docker, the template's images and, for some, a running `shaked`. See its
  [README](templates/software-factory/README.md).
- The `shaked` tests need its Postgres (`SHAKEDOWN_PG_URL`, as `shaked` finds it). The non-live ones use
  `core/crates/shaked/tests/fixtures/builder.py`, not this package. The live one builds `small-manufacturer` through
  `shaked` and requires it ready in under a minute.

## Limits

- Checks read only the end state (each part's `inspect`), not the order of events or what agents said.
- The builder checks that images are present but never pulls or builds them. Some images are built from this
  repository (each template's README says how).
- A built world depends on this checkout: its command runs this package from `${SHAKEDOWN_DIR}/benchmarks/world`.
- `shaked` uses one builder: the first of `kind = "builder"` by name.
- A template that fails to load is left out of `templates` with only a line on stderr.
