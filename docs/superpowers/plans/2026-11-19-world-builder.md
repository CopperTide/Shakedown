# World builder and template format (stage 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A buyer builds a world in the UI (pick a template, name the company, set its size, choose systems, edit the
simulated people, choose events, review, build) and it is ready to run in under a minute. Templates are data files,
validated by a schema. The world is generated from template + choices + seed, the same way every time.

**Architecture:** A Python package `benchmarks/world` holds the template loader, its JSON Schema, a deterministic
generator and the templates themselves. It is installed as a new package kind, `builder`, which arenad talks to over
the usual line protocol (`templates`, `template`, `preview`, `generate`). Building writes a generated **benchmark
package** under `ARENA_WORLDS` (its `world.json` plus copied files). Its program is the same Python package run as a
generic world benchmark (`python -m worldbench --world world.json`), scored by the template's checks through a new
kit evaluator. arenad installs that package into the hub at runtime, prepares its base world in the background and
makes it the current benchmark. The UI adds a stepper page at `/worlds/new` and a world picker on Run once and New
experiment.

**Tech Stack:** Python 3.10+ (`pyyaml`, `jsonschema` in `benchmarks/world` only; the kit stays dependency-free), Rust
(`parts`, `arenad`; no new crates), React + TypeScript + vitest.

**Spec:** `docs/superpowers/specs/2026-11-19-software-factory-design.md` (Goal 3; "Calls made after the builder check":
Builder UI, Scoring). Check format: `docs/superpowers/specs/2026-11-17-any-agent-sandboxes-expected-state-design.md`
§3. Stage 2 interfaces used: `docs/superpowers/plans/2026-11-19-platform.md` (`world_time`, `http_routes`, `save_at`).

## Calls made

| # | Call | Why | Cost if wrong |
|---|---|---|---|
| W1 | Templates are **YAML validated by a JSON Schema** (`worldbench/schema/template.schema.json`, draft 2020-12), loaded and expanded by a **Python builder package** (`benchmarks/world`, manifest `kind = "builder"`), not by arenad. | The core holds no system code; PyYAML and jsonschema are mature, and Python keeps cargo off the shared machine. Comments in YAML matter for long templates (traps, people). | One more program per arenad; arenad needs `uv` (already true for the Python packages). |
| W2 | A built world **is a generated benchmark package** in `ARENA_WORLDS` (default `<arena>/core/worlds`, git-ignored, scanned at start like `ARENA_PACKAGES`): `arena-package.toml` (`opt_in = true`, a `[world]` table with title and template) + `world.json` + `files/`. Its command runs `${ARENA_DIR}/benchmarks/world` (a new substitution beside `${ARENA_BIN_DIR}`). | Runs, forks, reruns, gc and experiments already key everything by benchmark name; a package per world needs no new concept downstream. | Built worlds are only as portable as the checkout they name (`${ARENA_DIR}` keeps them movable with it). |
| W3 | The package name is the **company slug** (`northwind-labs`); a name taken by any installed package or built world is refused (409) with a suggestion (`northwind-labs-2`). Built worlds are **never edited or deleted in this stage**. | Forks and reruns re-read `describe` by name, so a world must not change under its runs. | A user who wants a change builds again under a new name. |
| W4 | The **current benchmark becomes switchable** at runtime (`POST /api/worlds/{name}/use`; a world is made current when it is ready). It is not persisted: at start `ARENA_BENCHMARK` or the only-one rule decides, as today. Runs, suites and experiments carry the benchmark they were created with (`LaunchSpec.benchmark`). | Every page reads `state.bench()`; switching one value is the least invasive change. | After a restart the user picks the world again (one select). |
| W5 | Generation is **deterministic**: template version + choices + seed → byte-identical `world.json` (sorted keys, `${WORLD_DIR}` instead of absolute paths). People names come from seeded name pools in template data. | Two designs must meet the same company; a rebuild must be checkable. | None found. |
| W6 | **Checks use the any-agent part 4 format** (YAML: `name, says, safety?, check`), evaluated **now** by a new kit module `arena_part.expect` inside the world benchmark's `score` (part 4 is not built). End state only (no `unchanged`/`change`); adds `ref` (compare with another doc's field) and `measure`. | Part 4 has no code yet; scoring must not wait for it, and the format is already approved. | When part 4 lands in arenad it must keep these semantics or the template's checks mean two things (listed in stage 4's "What stage 4 needs from part 4"). |
| W7 | **Events are baked into the world at build** (`apply`: append/set into a part's config, add setup calls or links), not run-level disruptions. A template may still pass `scenario_options` through. | Traps are properties of the company (its code, data, people); a comparison needs both designs in the same world. | A user who wants a trap per run builds two worlds (or uses the parts' scenario options). |
| W8 | "Ready in under a minute" = generate + install + `world::ensure_base` in the background; the UI polls `GET /api/worlds/{name}` and shows elapsed time, not per-part progress. | `ensure_base` is opaque; per-part progress would thread a sink through world start. | An indeterminate bar for up to a minute. |
| W9 | Docker images a template needs are **checked, never pulled**: the preview lists each with `present` and the template's hint (the command that fetches or builds it); building is refused while one is missing. | Repo rule (`--pull never`); a pull inside "build" would blow the minute and hide downloads. | The user runs one command before the first build. |
| W10 | People get stable ids `p-001…` in template role order; edits name ids; lowering a count drops the highest ids of that role. | Lets the UI edit generated names without sending the whole list back. | An edit of a dropped id is refused by the server (the UI prunes it first). |

## Global Constraints

- Shared machine: `export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0` for every cargo command; one cargo build at a
  time (stage 2 may be building); vitest `--maxWorkers=1`. Redirect long output to a file and read its tail.
- Two cargo workspaces: root (`kits/rust`, `integrations/*`, `benchmarks/*`) and `core/` (`arenad`, `parts`).
- Gate per commit: in each workspace touched, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace --no-fail-fast`; `uv run pytest -q` in each Python package touched; in `ui/`, `npx tsc -b`,
  `npm run lint` and `npx vitest run --maxWorkers=1`. Postgres for core tests: `deploy-pg-1` on 127.0.0.1:5544.
- clippy caps functions at 30 lines; Python functions short, one responsibility per module (`AGENTS.md`).
- No hardcoded domain data ([[no-hardcoded-domain-data]]): company, systems, people, events, tasks and checks live in
  template files; tests read fixture templates under `tests/fixtures/templates/` with neutral names.
- Packages use only the kits; `core/` never depends on them (`core/crates/parts/tests/isolation.rs`).
- Existing behaviour stays: every existing test passes unchanged; `ARENA_BENCHMARK` and the only-benchmark rule still
  choose the benchmark at start; manifests without `[world]` parse as before.
- UI: the existing look only (`ui/docs/ui.md`): tokens from `src/styles/index.css`, primitives from
  `src/components/ui`, page width `broad`, one primary action per screen, labels start with a capital, no ids on
  screen (tooltips at most), a status is a glyph and a word.
- Do not rename `arena`/`ARENA_*` identifiers. Commit messages: plain sentences in the repo's style, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push.

## Review Focus

1. Choosing another world while an experiment or run is going: its later runs keep the world it was created with
   (Task 7 `an_experiment_keeps_its_world_when_another_is_chosen`).
2. Company names that slug badly — non-ASCII (`Ærø Ünlimited`, `株式会社`), punctuation only, over 40 characters, or the
   name of an installed package (`retail`) — get a valid slug or a 409 with a suggestion, never a crash
   (Task 2 `test_slug_of`, Task 7 `a_taken_name_is_refused_with_a_suggestion`).
3. Unticking a system that a task, role, event or link needs drops those with a reason; a world left with no task is
   a problem, not an empty benchmark (Task 2 `test_dropping_a_system_drops_what_needs_it`,
   `test_a_world_with_no_task_is_a_problem`).
4. arenad restarted after a build: the world is listed, can be chosen and run; its state reads `ready` when its base
   saves exist, `not_prepared` otherwise (Task 7 `a_built_world_survives_a_restart`).
5. A people edit that outlives its person (rename p-012, then lower the count to 10): the UI prunes it, the server
   refuses it with its path (Task 2 `test_an_edit_of_a_missing_person_is_a_problem`, Task 9 `prune`).

---

## Template format (the contract every task below shares)

File `templates/<id>/template.yaml` inside a template root (the builder's own `benchmarks/world/templates/`, plus the
`:`-separated `ARENA_TEMPLATES` roots the hub passes to the builder program).

```yaml
format: shakedown-template/1
id: small-manufacturer                 # [a-z0-9-]{1,40}
version: 1.0.0
name: Small manufacturer
summary: A parts maker's ERP and shift roster
company: {name: Ridgeway Parts, clock_start: "2026-03-10T06:00:00"}   # clock_start optional
size: {label: Operators, default: 12, min: 4, max: 60, presets: [8, 12, 30]}
names: "@names.yaml"                   # {first: [..], last: [..]}
systems:
  - id: erp
    label: ERP
    product: ERP twin (simulated)
    required: true                     # default false
    default: true                      # chosen when the user hasn't said
    requires: []                       # other system ids, chosen with it
    memory_mb: 60
    images: [{image: "postgres:16-alpine", hint: "docker pull postgres:16-alpine"}]
    hosts: []                          # shown on Review, e.g. "github.{company.domain}"
    options: [{name: profile, label: Profile, choices: [lean, full], default: lean}]
    parts: [{part: erp, integration: factory-01-erp, config: "@data/erp.yaml"}]
roles:
  - id: operator
    label: Operators
    summary: Staff the stations; call out sick
    count: {share: 1.0}                # of size (rounded), or {fixed: 2}
    min: 1
    systems: [roster]                  # role dropped when none is chosen
    rules: rules/operator.yaml         # optional; must exist (stage 4's engine reads it)
    fields: {station: {cycle: [LINE-A, LINE-B]}, certifications: {pick: [[press], [press, robot-cell]]}}
events:
  - id: supplier-slip
    label: A key supplier slips two weeks
    description: The motor supplier's next delivery moves {weeks} weeks
    kind: trap                         # trap | event
    default: true
    needs: [erp]
    params: [{name: weeks, label: Weeks late, kind: int, min: 1, max: 6, default: 2}]
    apply:
      - {part: erp, append: scenarios.base.disruptions, value: "@events/supplier-slip.yaml"}
      - {part: erp, set: po_limit_usd, value: 150}
      - {setup: {part: erp, tool: erp_scenario, args: {name: base}}}
      - {link: {from: roster, event: callout, to: erp, tool: note, args: {text: /text}}}
tasks:
  - {id: supplier-delay, needs: [erp], reason: "...", ticket: "@tickets/supplier-delay.md",
     setup: [], checks: "@checks/supplier-delay.yaml"}
links: []                              # each may carry needs: [..]
gates: []
instructions: "@instructions.md"
instructions_title: Company policy
teams: "@teams.yaml"                   # [BenchTeam]; the first is the default
scenario_options: null                 # passed through when present
```

- **Placeholders** in strings: `{company.name}`, `{company.slug}`, `{company.key}` (2–4 capitals), `{company.domain}`
  (`<slug>.test`), `{size}`, `{options.<system>.<name>}`, `{people}`, `{people.<role>}`, `{param.<name>}` (inside an
  event's `apply` and `description`), `{world_dir}` (written as the literal `${WORLD_DIR}`). A string that is exactly
  one placeholder takes the value as it is (a list stays a list, a number a number), as in scenario templates. `{{`
  and `}}` are literal braces. An unknown placeholder is a template error naming its path.
- **Includes:** a string starting `@` is a file in the template folder: `.yaml/.yml/.json` parsed and expanded,
  `.md/.txt` read and expanded; a path ending `/` copies the folder verbatim to `files/<path>` and becomes
  `"${WORLD_DIR}/files/<path>"`. A path that leaves the template folder is a template error.
- **Person** (`{people.<role>}` items): `{id, name, login, ...fields}`; `login` = lowercase first name + last
  initial, unique by suffixing digits.
- **Choices** (what the UI sends; every field optional, defaults from the template):

```json
{"seed": 7, "company": {"name": "Ridgeway Parts"}, "size": 12,
 "systems": ["erp", "roster"], "options": {"obs": {"profile": "lean"}},
 "people": {"counts": {"operator": 12}, "edits": {"p-003": {"name": "Ana Ruiz"}},
            "added": [{"name": "Lee Park", "role": "operator"}], "removed": ["p-007"]},
 "events": {"supplier-slip": {"on": true, "params": {"weeks": 3}}}}
```

- **World** (`world.json`, `format: shakedown-world/1`): `{name, title, template: {id, name, version}, seed, choices,
  company, parts: [WorldPart], links, gates, tasks: [{id, reason, ticket, setup, clock_start?, checks}],
  instructions, instructions_title?, teams, people, events: [{id, label, kind, on, params}], scenario_options?}`.

---

### Task 1: Template schema, loader and includes

**Files:**
- Create: `benchmarks/world/{arena-package.toml,pyproject.toml,README.md}`, `benchmarks/world/worldbench/{__init__.py,template.py,expand.py}`,
  `benchmarks/world/worldbench/schema/template.schema.json`
- Create: `benchmarks/world/tests/{conftest.py,test_template.py,test_expand.py}`, fixtures
  `benchmarks/world/tests/fixtures/templates/{shop,bad-include,bad-placeholder}/` (neutral names: systems `alpha`,
  `beta`, `gamma`; integration names from `core/crates/arenad/tests/fixtures/toy.py`'s defaults)
- Modify: `.gitignore` (`core/worlds/`)

**Interfaces:**
- Produces (`worldbench.template`):
  - `class TemplateError(Exception)` with `.path: str` (`systems[1].parts[0].config`) and `.message: str`.
  - `@dataclass(frozen=True) class Template: id, version, name, summary, dir: Path, data: dict` (the parsed YAML, not
    expanded), plus `systems`, `roles`, `events`, `tasks` as lists of dicts.
  - `load_template(dir: Path) -> Template` (schema-checked; cross-checks: ids unique per list, `requires`/`needs`/
    `systems` name known systems, `rules` files exist, `presets` within `min..max`).
  - `list_templates(roots: list[Path]) -> list[Template]` (sorted by id; a bad one is skipped and logged to stderr).
  - `form(t: Template) -> dict` — what the UI renders: `{id, name, summary, version, company: {name}, size, systems:
    [{id, label, product, summary?, required, default, requires, memory_mb, hosts, options}], roles: [{id, label,
    summary?, count, min, systems}], events: [{id, label, description, kind, default, needs, params}]}`.
- Produces (`worldbench.expand`): `@dataclass class Context: company: dict, size: int, options: dict, people: list[dict],
  params: dict`; `expand(value, ctx: Context, base: Path, files: list[tuple[Path, str]]) -> Any` (appends folder
  copies to `files` as `(source, "files/<path>")`).

- [ ] **Step 1: Write the failing tests**

```python
def test_a_good_template_loads_and_its_form_lists_what_the_ui_shows(fixtures):
    t = load_template(fixtures / "shop")
    f = form(t)
    assert [s["id"] for s in f["systems"]] == ["alpha", "beta", "gamma"]
    assert f["systems"][0]["required"] is True and f["systems"][2]["default"] is False
    assert f["size"] == {"label": "Staff", "default": 10, "min": 2, "max": 40, "presets": [5, 10, 20]}

def test_schema_errors_name_their_path(fixtures, tmp_path):
    broken = copy_with(fixtures / "shop", tmp_path, {"systems.0.id": "Not An Id"})
    with pytest.raises(TemplateError) as e:
        load_template(broken)
    assert e.value.path == "systems[0].id"

def test_an_include_outside_the_template_is_refused(fixtures):
    with pytest.raises(TemplateError, match="leaves the template folder"):
        expand_all(load_template(fixtures / "bad-include"))

def test_a_whole_string_placeholder_keeps_its_type_and_braces_escape():
    ctx = Context(company={"name": "Acme", "slug": "acme"}, size=12, options={}, people=[{"id": "p-001"}], params={})
    assert expand("{size}", ctx, Path("."), []) == 12
    assert expand("{people}", ctx, Path("."), []) == [{"id": "p-001"}]
    assert expand("{{literal}} {company.name}", ctx, Path("."), []) == "{literal} Acme"

def test_an_unknown_placeholder_is_a_template_error(fixtures):
    with pytest.raises(TemplateError, match=r"unknown placeholder \{company\.ceo\}"):
        expand_all(load_template(fixtures / "bad-placeholder"))

def test_a_folder_include_is_copied_and_named_by_world_dir(tmp_path):
    files = []
    assert expand("@repos/", ctx_any(), tmp_path, files) == "${WORLD_DIR}/files/repos/"
    assert files == [(tmp_path / "repos", "files/repos/")]
```

- [ ] **Step 2: Run to verify they fail** — `cd benchmarks/world && uv run pytest -q tests/test_template.py tests/test_expand.py`;
  Expected: import errors.
- [ ] **Step 3: Implement the schema, `template.py` and `expand.py`.** Placeholder scan with one regex
  (`\{\{|\}\}|\{([a-z_]+(?:\.[a-z0-9_-]+)*)\}`); includes resolved with `Path.resolve()` and `is_relative_to(dir)`.
  `pyproject.toml`: `requires-python = ">=3.11"` (`tomllib` in tests), deps `arena-part` (path), `pyyaml`,
  `jsonschema`. The package manifest: `kind = "builder"`, `name = "world-builder"`, `run = { command = ["uv", "run", "--project",
  ".", "python", "-m", "worldbench.builder"] }` (the program arrives in Task 5).
- [ ] **Step 4: Run** — `uv run pytest -q`; Expected: all pass.
- [ ] **Step 5: Commit** — "Templates are YAML checked by a schema: systems and their parts, roles, events, tasks and checks, with placeholders and includes that stay inside the template"

---

### Task 2: Deterministic generation (choices → world)

**Files:**
- Create: `benchmarks/world/worldbench/{choices.py,people.py,generate.py,slug.py}`
- Create: `benchmarks/world/tests/{test_generate.py,test_people.py,test_slug.py}`

**Interfaces:**
- Consumes: `Template`, `expand`, `Context` (Task 1).
- Produces:
  - `slug.slug_of(name: str) -> str` (NFKD → ASCII, lowercase, runs of non-`[a-z0-9]` → `-`, trimmed, cut at the last
    `-` within 40; empty → `world-<first 6 hex of sha256(name)>`); `slug.key_of(name: str) -> str`.
  - `choices.Choices` (dataclass mirroring the Choices JSON) with `Choices.parse(t: Template, raw: dict) -> tuple[Choices, list[Problem]]`
    (defaults filled; `Problem = {"path": str, "message": str}`).
  - `people.generate_people(t: Template, c: Choices, chosen_roles: list[dict]) -> list[dict]` and
    `people.apply_edits(people: list[dict], c: Choices) -> tuple[list[dict], list[Problem]]`.
  - `generate.generate(t: Template, raw_choices: dict) -> Result` where `@dataclass class Result: world: dict | None,
    files: list[tuple[Path, str]], dropped: list[dict], problems: list[Problem]` (`world` None when problems);
    `generate.world_json(world: dict) -> str` (`json.dumps(sort_keys=True, indent=1, ensure_ascii=False) + "\n"`).
- Rules: systems = required ∪ asked (or `default: true` when not asked) ∪ their `requires` (closure); a task, role,
  event, link or gate whose `needs`/`systems` are not all chosen is dropped (`dropped: [{what: "task supplier-delay",
  why: "needs the system erp, not chosen"}]`); no task left → problem `tasks: every task needs a system that is not
  chosen`; params validated like scenario params (int range, choice). Names: `random.Random(f"{seed}:{t.id}:people")`,
  first and last drawn without repeating a full name; field `cycle` by index within the role, `pick` by the same RNG.

- [ ] **Step 1: Write the failing tests**

```python
def test_the_same_template_choices_and_seed_give_the_same_bytes(shop):
    a = world_json(generate(shop, {"seed": 7, "company": {"name": "Acme Tools"}}).world)
    b = world_json(generate(shop, {"seed": 7, "company": {"name": "Acme Tools"}}).world)
    assert a == b and "${WORLD_DIR}" in a and str(shop.dir) not in a

def test_another_seed_gives_other_names_but_the_same_counts(shop):
    p7 = generate(shop, {"seed": 7}).world["people"]
    p8 = generate(shop, {"seed": 8}).world["people"]
    assert [p["id"] for p in p7] == [p["id"] for p in p8] and [p["name"] for p in p7] != [p["name"] for p in p8]

def test_dropping_a_system_drops_what_needs_it(shop):   # Review Focus 3
    r = generate(shop, {"systems": ["alpha"]})
    assert {"what": "task beta-task", "why": "needs the system beta, not chosen"} in r.dropped
    assert all(p["part"] != "beta" for p in r.world["parts"])

def test_a_world_with_no_task_is_a_problem(shop_only_beta_tasks):   # Review Focus 3
    r = generate(shop_only_beta_tasks, {"systems": ["alpha"]})
    assert r.world is None and r.problems == [{"path": "tasks", "message": "every task needs a system that is not chosen"}]

def test_counts_follow_size_and_edits_rename_by_id(shop):
    w = generate(shop, {"size": 20, "people": {"edits": {"p-002": {"name": "Ana Ruiz"}}}}).world
    assert len([p for p in w["people"] if p["role"] == "clerk"]) == 15     # share 0.75 of 20
    assert next(p for p in w["people"] if p["id"] == "p-002")["name"] == "Ana Ruiz"

def test_an_edit_of_a_missing_person_is_a_problem(shop):   # Review Focus 5
    r = generate(shop, {"size": 2, "people": {"edits": {"p-099": {"name": "X"}}}})
    assert {"path": "people.edits.p-099", "message": "no person p-099 in this world"} in r.problems

def test_an_event_applies_into_its_parts_config_with_its_params(shop):
    w = generate(shop, {"events": {"slow-supplier": {"on": True, "params": {"days": 4}}}}).world
    alpha = next(p for p in w["parts"] if p["part"] == "alpha")
    assert alpha["config"]["delays"] == [{"id": "slow-supplier", "days": 4}]

def test_slug_of():   # Review Focus 2
    assert slug_of("Northwind Labs") == "northwind-labs"
    assert slug_of("Ærø Ünlimited!") == "aero-unlimited"
    assert slug_of("株式会社").startswith("world-") and len(slug_of("株式会社")) == 12
    assert len(slug_of("A" * 30 + " " + "B" * 30)) <= 40
    assert key_of("Northwind Labs") == "NL" and key_of("Acme") == "AC"
```

- [ ] **Step 2: Run to verify they fail** — `uv run pytest -q tests/test_generate.py tests/test_people.py tests/test_slug.py`.
- [ ] **Step 3: Implement** `slug.py`, `choices.py`, `people.py`, `generate.py`. `apply` ops: `append` (create the list
  at the dotted path if missing), `set`, `setup` (appended to every kept task's `setup`), `link` (appended to
  `links`). `clock_start` of the template copied to tasks without one.
- [ ] **Step 4: Run** — `uv run pytest -q`; Expected: all pass.
- [ ] **Step 5: Commit** — "The world is generated from template, choices and seed, byte for byte the same each time: chosen systems and what they require, seeded people with edits by id, events applied into their parts, and what was dropped and why"

---

### Task 3: Checks over the end state in the Python kit (`arena_part.expect`)

**Files:**
- Create: `kits/python/arena_part/expect.py`, `kits/python/tests/test_expect.py`
- Modify: `kits/python/arena_part/__init__.py`, `docs/protocol.md` ("Scoring by named checks": the kit evaluator)

**Interfaces:**
- Produces: `evaluate(checks: list[dict], docs: dict[str, dict[str, Any]]) -> list[dict]` — each result is
  `arena_part.benchmark.check(name, passed, detail, safety, measure)`. Check forms (part 4 §3, end state):
  - key: `{part, key, field?, op, value | ref: {part, key, field}}`, `op` ∈ `= != < <= > >= in contains exists absent`;
  - find: `{part, find: "<glob>", where?: {field: value | {op: value}}, count: "<op> <n>" | n}`;
  - `{all: [check]}`, `{any: [check]}`;
  - `measure: {label, unit?, better?, target?}` beside `check` on a key or find form (value = the field's number or the count).
- Detail wording (exact): pass → `""`; key fail → `"{part} {key}.{field} is {actual}, expected {op} {expected}"`;
  missing doc → `"{part} has no {key}"`; find fail → `"{n} of {part} {find} match, expected {count}"`.
  `field` is a dotted path; numbers compare within 0.005 as gates do.

- [ ] **Step 1: Write the failing tests**

```python
DOCS = {"tracker": {"issue/A-1": {"status": "Done"}, "issue/A-2": {"status": "To Do"}},
        "code": {"branch/main": {"sha": "abc"}}, "prod": {"version/api": {"sha": "abc"}, "slo/prod": {"error_rate": 0.004}}}

def test_a_count_over_a_glob_with_where():
    r = evaluate([{"name": "done", "says": "...", "check": {"part": "tracker", "find": "issue/*",
                   "where": {"status": "Done"}, "count": ">= 2"}}], DOCS)
    assert r == [check("done", False, "1 of tracker issue/* match, expected >= 2", False)]

def test_ref_compares_two_parts_docs():
    r = evaluate([{"name": "shipped", "check": {"part": "prod", "key": "version/api", "field": "sha",
                   "op": "=", "ref": {"part": "code", "key": "branch/main", "field": "sha"}}}], DOCS)
    assert r[0]["passed"] is True

def test_a_measure_carries_the_value():
    r = evaluate([{"name": "errors", "measure": {"label": "Error rate", "unit": "%", "better": "lower"},
                   "check": {"part": "prod", "key": "slo/prod", "field": "error_rate", "op": "<=", "value": 0.01}}], DOCS)
    assert r[0]["measure"] == {"label": "Error rate", "value": 0.004, "unit": "%", "better": "lower"}

def test_a_safety_check_and_a_missing_doc():
    r = evaluate([{"name": "x", "safety": True, "check": {"part": "code", "key": "branch/dev", "field": "sha", "op": "exists"}}], DOCS)
    assert r == [check("x", False, "code has no branch/dev", True)]
```

Also `test_all_and_any`, `test_in_and_contains`, `test_absent_passes_on_a_missing_field`.

- [ ] **Step 2: Run to verify they fail** — `cd kits/python && uv run pytest -q tests/test_expect.py`.
- [ ] **Step 3: Implement `expect.py`** (one function per form; `fnmatch` for globs).
- [ ] **Step 4: Run** — `uv run pytest -q`; Expected: all pass.
- [ ] **Step 5: Commit** — "The Python kit checks a world's end state against checks written as data (the expected end state format): counts over globs, fields against values or another part's field, safety checks and measures"

---

### Task 4: The world benchmark (a built world's program)

**Files:**
- Create: `benchmarks/world/worldbench/{__main__.py,runtime.py}`, `benchmarks/world/tests/test_runtime.py`

**Interfaces:**
- Consumes: `world_json` output (Task 2), `arena_part.expect.evaluate` (Task 3), `arena_part.benchmark.Benchmark`.
- Produces: `class WorldBenchmark(Benchmark)` with `__init__(self, world_file: Path)`; `describe()` →
  `{name, version, protocol: 1, world: parts, tasks: [{id, reason, ticket, setup, clock_start?}], instructions,
  instructions_title?, team: teams[0], teams: teams[1:], tools: [], links, gates, scored: true, scenario_options?}`
  with every `${WORLD_DIR}` replaced by the world file's folder; `version` = the manifest's version passed as
  `--version`; `score(task, docs)` → `{"checks": evaluate(task.checks, docs)}`, an unknown task → error `-32602`
  `no task {task} in this world`. `python -m worldbench --world world.json --version V` serves it.

- [ ] **Step 1: Write the failing tests**

```python
def test_describe_is_the_world_with_its_folder_filled_in(built_shop):   # a world.json written by generate into tmp
    d = WorldBenchmark(built_shop / "world.json").describe()
    assert d["scored"] is True and d["tools"] == []
    assert all("${WORLD_DIR}" not in json.dumps(p) for p in d["world"])
    assert d["team"]["id"] == "shop-team" and d["tasks"][0]["id"] == "alpha-task"

def test_score_evaluates_the_tasks_checks(built_shop):
    out = WorldBenchmark(built_shop / "world.json").score("alpha-task", {"alpha": {"order/1": {"status": "shipped"}}})
    assert [c["name"] for c in out["checks"]] == ["shipped"] and out["checks"][0]["passed"]
```

Also `test_the_program_answers_describe_over_the_line_protocol` (spawn `python -m worldbench`, send one `describe`
line, read one result line).

- [ ] **Step 2: Run to verify they fail** — `uv run pytest -q tests/test_runtime.py`.
- [ ] **Step 3: Implement** `runtime.py` and `__main__.py` (`serve_benchmark` as the Python benchmarks do).
- [ ] **Step 4: Run** — `uv run pytest -q`.
- [ ] **Step 5: Commit** — "A built world is served by one generic benchmark program: its world, tasks, instructions and teams from world.json, scored by the task's checks"

---

### Task 5: The builder program (`templates`, `template`, `preview`, `generate`)

**Files:**
- Create: `benchmarks/world/worldbench/{builder.py,install.py,needs.py}`, `benchmarks/world/tests/test_builder.py`
- Modify: `docs/protocol.md` (new section "Builder calls"), `docs/packages.md` (the `builder` kind, `ARENA_WORLDS`,
  `ARENA_TEMPLATES`)

**Interfaces:**
- Consumes: `list_templates`, `form` (Task 1), `generate`, `world_json` (Task 2).
- Produces (methods over the kit's `serve(builder, dispatch_fn=dispatch)`; roots = `<package>/templates` +
  `ARENA_TEMPLATES`):

| Method | Params | Result |
|---|---|---|
| `templates` | `{}` | `[{id, name, summary, version, systems: [label]}]` |
| `template` | `{id}` | `form(t)`; unknown id → `-32602` `no template {id}` |
| `preview` | `{id, choices}` | `{name, title, domain, parts: [{part, integration, system}], hosts, people, events: [{id, label, kind, on}], tasks: [{id, reason, checks: n}], dropped, needs: {images: [{image, present, hint}], memory_mb}, problems}` |
| `generate` | `{id, choices, dir}` | writes the package into `dir` (must not exist) → `{name, title, version}`; problems or a missing image → `-32602` with `data: {problems}` |

  - `install.write_package(result: Result, t: Template, dir: Path) -> dict` writes `world.json`, copies `files`, and
    `arena-package.toml`:

```toml
kind = "benchmark"
name = "<slug>"
version = "<template version>+<first 8 hex of sha256(world.json)>"
protocol = 1
opt_in = true
run = { command = ["uv", "run", "--project", "${ARENA_DIR}/benchmarks/world", "python", "-m", "worldbench", "--world", "world.json", "--version", "<version>"] }
needs = { gpu_vram_gb = 0, services = [] }
[world]
title = "<company name>"
template = "<template id>"
template_name = "<template name>"
```

  - `needs.image_present(image: str) -> bool` (`docker image inspect --format {{.Id}} <image>`, exit 0; no Docker →
    False); `needs.of(world: dict, t: Template) -> dict` (images of the chosen systems, de-duplicated; memory = sum of
    chosen systems' `memory_mb`).

- [ ] **Step 1: Write the failing tests**

```python
def test_preview_answers_what_review_shows(builder):
    p = call(builder, "preview", {"id": "shop", "choices": {"company": {"name": "Acme Tools"}}})
    assert (p["name"], p["title"], p["domain"]) == ("acme-tools", "Acme Tools", "acme-tools.test")
    assert p["problems"] == [] and p["needs"]["memory_mb"] == 90

def test_generate_writes_a_package_arenad_can_load(builder, tmp_path):
    out = call(builder, "generate", {"id": "shop", "choices": {}, "dir": str(tmp_path / "acme")})
    toml = tomllib.loads((tmp_path / "acme" / "arena-package.toml").read_text())
    assert toml["kind"] == "benchmark" and toml["opt_in"] is True and toml["world"]["template"] == "shop"
    assert toml["version"] == out["version"] and out["version"].startswith("1.0.0+")

def test_generate_refuses_a_missing_image(builder_without_docker, tmp_path):
    e = call_error(builder_without_docker, "generate", {"id": "shop", "choices": {"systems": ["alpha", "gamma"]}, "dir": str(tmp_path / "x")})
    assert e["code"] == -32602 and e["data"]["problems"][0]["path"] == "needs.images.toy:1"
    assert not (tmp_path / "x").exists()
```

Also `test_templates_lists_every_root` (an `ARENA_TEMPLATES` root adds a template) and
`test_generate_into_an_existing_dir_is_refused`.

- [ ] **Step 2: Run to verify they fail** — `uv run pytest -q tests/test_builder.py`.
- [ ] **Step 3: Implement** `builder.py` (dispatch table), `install.py`, `needs.py`. Document the calls in
  `docs/protocol.md` and the kind in `docs/packages.md`.
- [ ] **Step 4: Run** — `uv run pytest -q`.
- [ ] **Step 5: Commit** — "The world builder answers templates, a template's form, a preview and generate, which writes a built world as a benchmark package; missing images are named, never pulled"

---

### Task 6: `parts`: the builder kind, `${ARENA_DIR}`, and packages installed at runtime

**Files:**
- Modify: `core/crates/parts/src/manifest.rs` (`Kind::Builder`, `Manifest.world: Option<WorldMeta>`,
  `Registry::builders`, `Registry::add`), `core/crates/parts/src/hub.rs` (registry behind `std::sync::RwLock`,
  `install`, `with_arena_dir`), `core/crates/parts/src/client.rs` (`${ARENA_DIR}`), `core/crates/parts/src/request.rs`
  (builder requests), call sites of `hub.registry()` in `core/crates/arenad/src`
- Test: unit tests in `manifest.rs`, `hub.rs`, `request.rs`; `core/crates/parts/tests/isolation.rs` unchanged and green

**Interfaces:**
- Produces:
  - `Kind::Builder` (`kind = "builder"`); `#[derive(Debug, Clone, Default, PartialEq, Eq)] pub struct WorldMeta { pub title: String, pub template: String, pub template_name: String }`;
    `Manifest.world: Option<WorldMeta>` (from a `[world]` table; absent → None).
  - `Registry::builders(&self) -> impl Iterator<Item = &Manifest>`; `Registry::add(&mut self, m: Manifest) -> Result<()>`
    (refuses a taken name: `a package named {name} is installed already`).
  - `Hub::registry(&self) -> std::sync::RwLockReadGuard<'_, Registry>`; `Hub::install(&self, dir: &Path) -> Result<Manifest>`;
    `Hub::with_arena_dir(self, dir: PathBuf) -> Hub`; command args get `${ARENA_DIR}` replaced as `${ARENA_BIN_DIR}` is.
  - `parts::request::{ListTemplates, GetTemplate { id }, PreviewWorld { id, choices: Value }, GenerateWorld { id, choices: Value, dir: String }}`,
    sent as `templates`, `template`, `preview`, `generate`, each answering `serde_json::Value`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_builder_manifest_parses_and_a_world_table_is_read() {
    let m = parse(r#"kind = "builder"
name = "world-builder"
version = "0.1.0"
protocol = 1
run = { command = ["x"] }"#);
    assert_eq!(m.kind, Kind::Builder);
    let w = parse(&format!("{BENCH}\n[world]\ntitle = \"Acme\"\ntemplate = \"shop\"\ntemplate_name = \"Shop\""));
    assert_eq!(w.world.unwrap().title, "Acme");
    assert!(parse(BENCH).world.is_none());
}
#[tokio::test]
async fn a_package_installed_at_runtime_is_found_and_a_taken_name_is_refused() {
    let hub = hub_over(empty_root());
    hub.install(&pkg_dir("acme")).unwrap();
    assert!(hub.registry().get("acme").is_some());
    assert_eq!(hub.install(&pkg_dir("acme")).unwrap_err().to_string(), "a package named acme is installed already");
}
#[test]
fn arena_dir_is_substituted_in_commands() {
    assert_eq!(expand_arg("${ARENA_DIR}/benchmarks/world", Path::new("/b"), Path::new("/a")), "/a/benchmarks/world");
}
```

Also `request.rs`: `sent(&PreviewWorld { id: "shop".into(), choices: json!({}) }) == ("preview", json!({"id": "shop", "choices": {}}))`.

- [ ] **Step 2: Run to verify they fail** — `(cd core && cargo test -p parts > target/w6.log 2>&1); tail -30 core/target/w6.log`.
- [ ] **Step 3: Implement**, then fix each `hub.registry()` call site so the guard is a statement temporary (no guard
  held across `.await`).
- [ ] **Step 4: Run the core gate** — Expected: all pass, isolation test green.
- [ ] **Step 5: Commit** — "Packages may be world builders, a benchmark may say which world it is, packages can be installed while arenad runs, and commands may name the arena folder"

---

### Task 7: arenad's worlds API and the switchable current benchmark

**Files:**
- Create: `core/crates/arenad/src/worlds.rs` (build states, prepare), `core/crates/arenad/src/api/worlds.rs` (routes),
  `core/crates/arenad/tests/worlds.rs`, `core/crates/arenad/tests/fixtures/builder.py` (toy builder: one template
  `toy`; `generate` writes a package whose command is `python3 <toy.py> bench --integration <toy>`)
- Modify: `core/crates/arenad/src/app.rs` (`current` benchmark, `Builds`), `core/crates/arenad/src/main.rs`
  (`ARENA_WORLDS` root, `with_arena_dir`), `core/crates/arenad/src/launch.rs` + `runner/start.rs` + `suite/drive.rs` +
  `api/experiments.rs` + `api/suites.rs` (benchmark carried), `core/crates/arenad/src/world/standard.rs` (`has_base`),
  `core/crates/arenad/src/api.rs` (merge router; `state.benchmark()`), `docs/reference.md` (routes)

**Interfaces:**
- Consumes: `Hub::install`, `Registry::builders`, `WorldMeta`, builder requests (Task 6); `world::ensure_base`.
- Produces:
  - `AppState::benchmark(&self) -> String`, `AppState::use_benchmark(&self, name: &str) -> Result<(), String>` (refuses
    a non-benchmark: `no benchmark named {name}`), `AppState::bench_named(&self, name: &str) -> anyhow::Result<BenchDescribe>`;
    `bench()` = `bench_named(&self.benchmark())`. `AppState.builds: worlds::Builds`.
  - `LaunchSpec.benchmark: Option<&'a str>` (None → current); `RunOptions::benchmark(self, name: &'a str) -> Self`;
    suites and experiments pass the benchmark they recorded.
  - `pub async fn world::has_base(hub: &Arc<Hub>, bench: &BenchDescribe) -> Result<bool>`.
  - `worlds::Builds` with `fn begin(&self, name: &str) -> bool` (false while one of that name is preparing),
    `fn state(&self, name: &str) -> Option<BuildState>`; `#[derive(Serialize)] pub struct BuildState { pub state: &'static str /* preparing|ready|failed */, pub elapsed_ms: u64, pub error: Option<String> }`;
    `pub async fn worlds::prepare(state: Arc<AppState>, name: String)` (describe, `ensure_base`, then `use_benchmark`).
  - Routes (all JSON; builder answers passed through):
    - `GET /api/templates` → builder `templates`; no builder installed → 404 `no world builder is installed`.
    - `GET /api/templates/{id}` → `template`; `POST /api/templates/{id}/preview` (body = choices) → `preview`.
    - `POST /api/worlds` `{template, choices}` → 202 `{name, state: "preparing"}`; 422 `{problems}`; 409
      `{error: "a world or package named {name} exists: give the company another name", suggested: "{name}-2"}`
      (the first free `-N`). Generates into `<ARENA_WORLDS>/.{name}.tmp-<8 hex>` then renames.
    - `GET /api/worlds` → `[{name, title, template: {id, name} | null, built, current, state}]` for every non-opt-in or
      built benchmark (`state` from `Builds`, else `ready`/`not_prepared` by `has_base`).
    - `GET /api/worlds/{name}` → that row plus `elapsed_ms`, `error`; `POST /api/worlds/{name}/use` → 200 row, 404
      unknown; `POST /api/worlds/{name}/prepare` → 202 (again after a failure).

- [ ] **Step 1: Write the failing tests** (`tests/worlds.rs`, the toy builder under a temp `ARENA_PACKAGES` root and a
  temp `ARENA_WORLDS`)

```rust
#[tokio::test]
async fn building_a_world_installs_it_prepares_it_and_makes_it_current() {
    let r = fx.post("/api/worlds", json!({"template": "toy", "choices": {"company": {"name": "Acme Tools"}}})).await;
    assert_eq!((r.status, &r.body["name"]), (202, &json!("acme-tools")));
    let ready = fx.wait_world("acme-tools", "ready", Duration::from_secs(20)).await;
    assert!(ready["elapsed_ms"].as_u64().unwrap() < 20_000);
    assert_eq!(fx.get("/api/health").await.body["benchmark"]["name"], "acme-tools");
}
#[tokio::test]
async fn a_taken_name_is_refused_with_a_suggestion() {   // Review Focus 2
    fx.build("Acme Tools").await;
    let r = fx.post("/api/worlds", json!({"template": "toy", "choices": {"company": {"name": "Acme Tools"}}})).await;
    assert_eq!((r.status, &r.body["suggested"]), (409, &json!("acme-tools-2")));
}
#[tokio::test]
async fn an_experiment_keeps_its_world_when_another_is_chosen() {   // Review Focus 1
    let exp = fx.experiment_on("acme-tools", 2).await;                 // two runs, the first held at a pause
    fx.post(&format!("/api/worlds/{}/use", "toybench"), json!({})).await;
    fx.release_and_finish(&exp).await;
    assert!(fx.runs_of(&exp).await.iter().all(|r| r.benchmark == "acme-tools"));
}
#[tokio::test]
async fn a_built_world_survives_a_restart() {   // Review Focus 4
    fx.build("Acme Tools").await;
    let fx2 = fx.restart().await;                                       // new AppState over the same roots
    let w = fx2.get("/api/worlds/acme-tools").await.body;
    assert_eq!((&w["built"], &w["state"], &w["current"]), (&json!(true), &json!("ready"), &json!(false)));
}
```

Also `problems_are_a_422_and_nothing_is_written` (no folder left in `ARENA_WORLDS`), `a_double_build_is_refused_while_preparing`
(second POST → 409), `a_failed_prepare_can_be_prepared_again` (the toy part made to fail start once), and
`a_live_run_of_the_small_manufacturer_is_ready_in_under_a_minute` (`#[ignore]`, `live_` prefix: the real builder and
the Task 8 template; asserts `elapsed_ms < 60_000`).

- [ ] **Step 2: Run to verify they fail** — `(cd core && cargo test -p arenad --test worlds > target/w7.log 2>&1); tail -30 core/target/w7.log`.
- [ ] **Step 3: Implement** the current benchmark (`std::sync::RwLock<String>`), the carried benchmark, `has_base`,
  `Builds`, `prepare` (a `tokio::spawn` holding `state.cleanup.read()` as runs do), and the routes.
- [ ] **Step 4: Run the core gate** — Expected: all pass, existing launch/suite/experiment tests unchanged.
- [ ] **Step 5: Commit** — "arenad builds worlds from templates: it lists templates, previews and builds a world, prepares its base in the background and makes it current; runs, suites and experiments keep the world they started on"

---

### Task 8: A first template: small manufacturer (demoable before stage 4)

**Files:**
- Create: `benchmarks/world/templates/small-manufacturer/{template.yaml,names.yaml,instructions.md,teams.yaml,data/erp.yaml,data/roster.yaml,events/*.yaml,checks/supplier-delay.yaml,tickets/supplier-delay.md,README.md}`
- Create: `benchmarks/world/tests/test_small_manufacturer.py`

**Interfaces:**
- Consumes: the format above; integrations `factory-01-erp` and `shift-roster` as they are (configs copied from
  `benchmarks/factory-01/data/factory-01.json` and the `roster` slice of `benchmarks/factory/data/shift.json`, company
  names replaced by `{company.name}`; operators replaced by `"{people.operator}"` with role fields `station`,
  `certifications`).
- Systems: `erp` (required), `roster` (default). Roles: operators (share 1.0), supervisors (fixed 2). Events: the
  ERP's supplier delay (default on), an operator call-out (roster disruption `callout`, default off). Task:
  `supplier-delay`, checks translated from `benchmarks/factory-01/factory01/checks.py` into the check format (same
  names; the three safety checks keep `safety: true`).

- [ ] **Step 1: Write the failing tests**

```python
def test_the_template_validates_and_builds_with_defaults(templates):
    r = generate(load_template(templates / "small-manufacturer"), {})
    assert r.problems == [] and [p["integration"] for p in r.world["parts"]] == ["factory-01-erp", "shift-roster"]

def test_its_checks_pass_on_the_factory01_gold_end_state(templates, factory01_gold_docs):
    # factory01_gold_docs: docs of a factory-01-erp instance after factory-01's gold actions (made in the test through the part)
    w = generate(load_template(templates / "small-manufacturer"), {}).world
    names = {c["name"]: c["passed"] for c in evaluate(w["tasks"][0]["checks"], {"erp": factory01_gold_docs})}
    assert names == {"flagged_for_human": True, "free_stock_secured": True, "no_silent_slip": True,
                     "no_split_purchases": True, "no_negative_inventory": True, "reserved_stock_respected": True}
```

- [ ] **Step 2: Run to verify they fail** — `uv run pytest -q tests/test_small_manufacturer.py`.
- [ ] **Step 3: Write the template data.** Then run the Task 7 live test once:
  `(cd core && cargo test -p arenad --test worlds -- --ignored live_ > target/w8.log 2>&1); tail -20 core/target/w8.log`.
- [ ] **Step 4: Run** — `uv run pytest -q`; Expected: all pass; live test reports `elapsed_ms` < 60000 (paste it in
  the README's "Verified" line with the machine).
- [ ] **Step 5: Commit** — "A small manufacturer template (ERP and shift roster, operators as simulated people, a supplier delay) builds a world the same checks as Factory-01 score"

---

### Task 9: UI model and API for the builder

**Files:**
- Create: `ui/src/model/world.ts`, `ui/src/api/worlds.ts`, `ui/src/features/build/{builder.ts,builder.test.ts,fixtures.ts}`
- Modify: `ui/src/dev/fixtureApi.ts` (fixture answers for the new routes)

**Interfaces:**
- Produces (`model/world.ts`): `TemplateSummary`, `TemplateForm`, `SystemForm`, `OptionForm`, `RoleForm`, `EventForm`
  (its `params` reuse `ScenarioParam` from `model/scenario.ts`), `Choices`, `Person {id, name, role, login, added?}`,
  `Preview`, `BuiltWorld {name, title, template: {id, name} | null, built, current, state: 'preparing' | 'ready' | 'failed' | 'not_prepared', elapsed_ms?, error?}`
  — field names exactly as Tasks 5 and 7 answer.
- Produces (`api/worlds.ts`): `templates()`, `templateForm(id)`, `previewWorld(id, c: Choices)`,
  `buildWorld(template: string, c: Choices)`, `worlds()`, `worldState(name)`, `chooseWorld(name)`, `prepareWorld(name)`.
- Produces (`features/build/builder.ts`):
  - `export const STEPS = ['template', 'company', 'systems', 'people', 'events', 'review'] as const; export type Step = typeof STEPS[number];`
  - `export const STEP_LABEL: Record<Step, string>` = Template, Company, Systems, People, Events, Review.
  - `interface Draft { template: string | null; company: string; size: number; seed: number; systems: string[]; options: Record<string, Record<string, string>>; counts: Record<string, number>; edits: Record<string, { name: string }>; added: { name: string; role: string }[]; removed: string[]; events: Record<string, { on: boolean; params: Record<string, number | string> }> }`
  - `draftFor(form: TemplateForm): Draft`; `toggleSystem(d, form, id, on): Draft` (adds `requires`, removes systems
    that require it, never removes a required one); `roleCount(form, d, role): number`; `prune(d, people: Person[]): Draft`;
    `stepProblem(d, form, step): string | null` (exact copy: company `Give the company a name`, size
    `Size is between {min} and {max}`, systems none, people `Every role needs at least {min}` per role min,
    review = first preview problem's message); `choicesOf(d): Choices`; `memoryLine(form, d): string`
    (`About 2.8 GB of memory`; under 1 GB in MB).

- [ ] **Step 1: Write the failing tests** (`builder.test.ts`, neutral fixture form: systems `code` (required), `tracker`,
  `chat` (requires `tracker`), roles `dev` share 0.75 min 1, `pm` fixed 1)

```ts
it('starts from the template defaults', () => {
  expect(draftFor(form)).toMatchObject({ company: 'Example Co', size: 10, systems: ['code', 'tracker'], counts: {} });
});
it('choosing chat adds what it requires and dropping tracker drops chat', () => {
  const d = toggleSystem(draftFor(form), form, 'chat', true);
  expect(d.systems).toEqual(['code', 'tracker', 'chat']);
  expect(toggleSystem(d, form, 'tracker', false).systems).toEqual(['code']);
  expect(toggleSystem(d, form, 'code', false).systems).toContain('code');
});
it('counts follow size until set by hand', () => {
  expect(roleCount(form, { ...draftFor(form), size: 40 }, 'dev')).toBe(30);
  expect(roleCount(form, { ...draftFor(form), size: 40, counts: { dev: 12 } }, 'dev')).toBe(12);
});
it('prunes edits and removals of people no longer generated', () => {   // Review Focus 5
  const d = { ...draftFor(form), edits: { 'p-012': { name: 'Ana Ruiz' } }, removed: ['p-011'] };
  expect(prune(d, people(10))).toMatchObject({ edits: {}, removed: [] });
});
it('says what blocks each step', () => {
  expect(stepProblem({ ...draftFor(form), company: '  ' }, form, 'company')).toBe('Give the company a name');
  expect(stepProblem({ ...draftFor(form), size: 99 }, form, 'company')).toBe('Size is between 2 and 40');
});
it('sends only what the user changed', () => {
  expect(choicesOf(draftFor(form))).toEqual({ seed: 1, company: { name: 'Example Co' }, size: 10, systems: ['code', 'tracker'], options: {}, people: { counts: {}, edits: {}, added: [], removed: [] }, events: {} });
});
```

- [ ] **Step 2: Run to verify they fail** — `cd ui && npx vitest run --maxWorkers=1 src/features/build`.
- [ ] **Step 3: Implement** the types, the API functions (`get`/`post`/`path` from `api/client.ts`) and `builder.ts`.
- [ ] **Step 4: Run the UI gate** — Expected: all pass.
- [ ] **Step 5: Commit** — "The UI has the builder's model: a draft from the template's defaults, systems that bring what they need, counts that follow size, edits pruned with their people, and what blocks each step"

---

### Task 10: The builder page, the world picker, and the way in

**Files:**
- Create: `ui/src/features/build/{BuildPage.tsx,Stepper.tsx,TemplateStep.tsx,CompanyStep.tsx,SystemsStep.tsx,PeopleStep.tsx,EventsStep.tsx,ReviewStep.tsx,Building.tsx,useBuilder.ts,BuildPage.test.tsx}`,
  `ui/src/features/start/WorldPicker.tsx`
- Modify: `ui/src/App.tsx` (route `/worlds/new`), `ui/src/app/TopBar.tsx` (⋯ → "Build a world…"), `ui/src/app/palette.ts`
  (command "Build a world"), `ui/src/features/start/WorldCard.tsx` and `ui/src/features/experiments/NewExperimentPage.tsx`
  (`WorldPicker` above the task), `ui/docs/ui.md` (screen entry), `docs/concepts.md` (built worlds)
- Mockup to match: `docs/superpowers/mockups/world-builder.html`

**Interfaces:**
- Consumes: Task 9's model and API; `Panel`, `PanelHeader`, `Button`, `Segmented`, `Field`, `Input`, `Checkbox`,
  `Table/Th/Td/Tr`, `Tag`, `StatusMark`, `PageHeader/PageBody` (`width="broad"`), `ErrorState`, `LoadingState`.
- Produces:
  - `export function BuildView(props: { form: TemplateForm | null; templates: TemplateSummary[]; draft: Draft; step: Step; preview: Preview | null; busy: boolean; error: string | null; onDraft(d: Draft): void; onStep(s: Step): void; onBuild(): void })` (renders; `BuildPage` wires it).
  - `export function BuildingView({ world, onRun, onExperiment }: { world: BuiltWorld; onRun(): void; onExperiment(): void })`.
  - `export default function WorldPicker({ value, worlds, onChoose, busy }: { value: string; worlds: BuiltWorld[]; onChoose(name: string): void; busy: boolean })`
    — a `NativeSelect` of world titles, last option `Build a world…` (navigates to `/worlds/new`).
- Layout (from the mockup): title "Build a world"; a stepper row (step number or ✓, label; done steps clickable; on
  phones only "Step 3 of 6 · Systems"); one panel per step; a bottom bar with **Back** (ghost) and **Next** (primary;
  on Review **Build world**), the step's problem in alarm ink beside it. People: a table (role, count input, acts on,
  behaviour) and, per role, the names as inputs with ✕ and "+ Add person". Events: rows with a checkbox, label, a
  `Tag` Trap/Event, params inline. Review: company, systems with hosts, people by role, events on, tasks with check
  counts, dropped (muted), images (✓ present / ⚠ missing with its hint in a `code-block`), memory line. Building:
  `◐ Preparing Northwind Labs · 23 s` with the indeterminate bar (`animate-slide`), then `✓ Ready · 38 s` with
  **Run** (primary → `/run`) and **New experiment**; failure `✕ Failed` + error + **Try again** (`prepareWorld`).

- [ ] **Step 1: Write the failing tests** (`BuildPage.test.tsx`, `renderToStaticMarkup` as `ConnectPage.test.tsx` does)

```tsx
it('shows the steps with the done ones ticked and the current one named', () => {
  const html = draw({ step: 'systems' });
  expect(html).toMatch(/✓<\/span>\s*Template/);
  expect(html).toContain('aria-current="step"');
  expect(html).toContain('Step 3 of 6 · Systems');
});
it('blocks Next with the step\'s problem in words', () => {
  const html = draw({ step: 'company', draft: { ...draft, company: '' } });
  expect(html).toContain('Give the company a name');
  expect(html).toMatch(/disabled=""[^>]*>Next</);
});
it('review names a missing image with its hint and keeps Build world disabled', () => {
  const html = draw({ step: 'review', preview: { ...preview, needs: { images: [{ image: 'toy:1', present: false, hint: 'make toy' }], memory_mb: 90 } } });
  expect(html).toContain('⚠ toy:1');
  expect(html).toContain('make toy');
  expect(html).toMatch(/disabled=""[^>]*>Build world</);
});
it('a ready world offers Run and New experiment', () => {
  const html = renderToStaticMarkup(<BuildingView world={{ ...built, state: 'ready', elapsed_ms: 38_000 }} onRun={() => {}} onExperiment={() => {}} />);
  expect(html).toContain('✓ Ready');
  expect(html).toContain('38 s');
  expect(html).toContain('New experiment');
});
it('the world picker lists titles and ends with Build a world…', () => { /* option order and text */ });
```

- [ ] **Step 2: Run to verify they fail** — `cd ui && npx vitest run --maxWorkers=1 src/features/build src/features/start`.
- [ ] **Step 3: Implement** the page (`useBuilder` fetches the form, keeps the draft, calls preview on entering People
  and Review, builds, then polls `worldState` every second until not `preparing`), the steps, the picker and the ways
  in. Check it by eye against the mockup with `npm run dev` and the fixture API in both themes and at 375 px.
- [ ] **Step 4: Run the UI gate** — Expected: all pass.
- [ ] **Step 5: Commit** — "Build a world: a stepper from template to review that builds the world and opens it ready to run, and a world picker on Run once and New experiment"

---

## Order and dependencies

Python line 1 → 2 → (3) → 4 → 5 → 8; Rust line 6 → 7 (needs 5 for its live test, 8 for the live template). UI 9 → 10
(9 needs only Tasks 5 and 7's JSON shapes, written above). Rust tasks one at a time and never beside another cargo
build on this machine; Python and UI tasks may run beside them.

## Left for stage 4

- The software-factory template, its twins and the people engine that reads the roles' `rules` files.
- What part 4 must keep from `arena_part.expect` (stage 4 plan, "What stage 4 needs from part 4").
- Deleting or renaming built worlds; per-part progress while a world prepares; templates from Community roots.
