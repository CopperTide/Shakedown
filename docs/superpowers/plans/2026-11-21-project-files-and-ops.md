# Project Files and the Operation Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** shaked opens a project folder whose worlds, scenarios, setups and experiments are YAML files, and exposes every file operation through one operation registry that serves both HTTP routes and a control MCP server (HTTP and stdio).

**Architecture:** A new library crate `shakedown-project` owns the file formats (typed serde structs, strict parsing), the folder layout, validation with file + JSON-pointer locations, and atomic writes. shaked holds an `Arc<RwLock<Project>>` in `AppState`. A new `ops` module defines `Operation` records (name, area, access class, JSON input schema, handler); `ops::router()` mounts them as `/api/...` routes and `ops::mcp` serves them as MCP tools, so a parity test can prove every operation exists on both surfaces.

**Tech Stack:** Rust 2021, axum 0.8, serde/serde_json, `serde_yaml_ng` (maintained serde_yaml fork; pin the latest 0.10.x), `schemars` 1.x for input schemas, thiserror 2, tokio.

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` (§1 model, §11 files, §13 assistant and MCP, §14 setups, §14b credentials and paths).

## Plan series (this is plan 1 of 7)

1. **Project files and the operation registry** (this plan): backend only.
2. **UI foundation:** v7 design system (identity rules in spec §4), shell (sidebar, top bar, Ask slot), Home, Experiments list, Experiment and Run pages on the new API.
3. **Config editors:** World (connectors, flows, data files, database default state), Scenario (inputs, events, gates, checks, dry run), Setup (any team; MADL import, graph, validation via the `madl` crate), Connections and keys (secrets providers, signed-in sessions).
4. **Assistant:** the Ask drawer, diff and confirm cards, permission policy, chat history, all through the control MCP.
5. **Onboarding and system states:** first run, empty, loading, error, success.
6. **Observability and coordination:** observability stand-ins, simulated operators, "Needs a human" events, run coordination view.
7. **Migration:** existing teams, worlds and loose runs into project files and one-run experiments (spec §12).

Each plan ships working, tested software. Plans 2–7 are written after the previous one lands.

## Global Constraints

- **Code quality is a requirement** (owner: "Make sure the code is of high quality too"):
  - `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass in `core/`.
  - Functions stay under clippy's 30-line threshold; never `#[allow(clippy::too_many_lines)]`.
  - No `unwrap`/`expect` outside tests.
  - Errors are typed with `thiserror`.
  - One responsibility per file, and doc comments on every public item.
- Machine limits: `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, one cargo build at a time.
- Tests never call a real model and never touch the user's databases. Every test drops the databases and folders it creates; use `tempfile` dirs for projects.
- Secrets never go in files. Auth fields hold a key-store name (`key: larkspur-mes`) or `env: NAME`. Parsing rejects values that look like credentials.
- The UI never shows file paths (§14b). API responses carry ids and names. The project directory appears only in `project.status` for the CLI.
- Every new `/api` route has a row in `api/routes.rs` `ROUTES` (generated from the registry, Task 5); `tests/routes.rs` stays green.
- Open-source schema stays personal-use: no org/tenant fields in any file format (D16).
- File format ids: `shakedown-project/1`, `shakedown-world/3`, `shakedown-scenario/1`, `shakedown-setup/1`, `shakedown-experiment/1`.

## Review Focus

1. **Ids that escape the folder.** An id like `../x`, `a/b`, an empty id, or one over 64 characters must be rejected before any filesystem call (Task 2 test `rejects_unsafe_ids`).
2. **Inline secrets.** A file with `key: sk-ant-...` or `token: ghp_...` must fail validation, naming the file and pointer, and must never be echoed back in the error text (Task 1 test `inline_secret_rejected_without_echo`).
3. **Crash mid-write.** A write must never leave a half-written YAML file (Task 2 test `write_is_atomic`).
4. **Files edited outside the app.** A hand edit that breaks YAML must surface as a project error on `project.status`, not take shaked down. Other files must still load (Task 3 test `broken_file_reported_others_load`).
5. **Dangling references.** A scenario naming a deleted world is reported as a validation error on that scenario. Listing still works (Task 2 test `dangling_reference_reported`).

---

### Task 1: File formats crate

**Files:**
- Create: `core/crates/project/Cargo.toml` (package `shakedown-project`, `[lints] workspace = true`), `core/crates/project/src/lib.rs`
- Create: `src/format/mod.rs`, `src/format/project.rs`, `src/format/world.rs`, `src/format/scenario.rs`, `src/format/setup.rs`, `src/format/experiment.rs`, `src/secrets.rs`, `src/error.rs`
- Modify: `core/Cargo.toml` (workspace members, add `serde_yaml_ng`, `schemars`, `tempfile` dev)
- Test: `core/crates/project/tests/formats.rs`, fixtures in `core/crates/project/tests/fixtures/larkspur/`

**Interfaces:**
- Produces:
  - `Kind { World, Scenario, Setup, Experiment }`, with `Kind::dir(self) -> &'static str` (`worlds`, `scenarios`, `setups`, `experiments`) and `Kind::format(self) -> &'static str`.
  - File structs, all `#[serde(deny_unknown_fields)]` + `JsonSchema`, each with a `format: String` field:
    - `ProjectFile { format, name, defaults: Defaults, permissions: Permissions }`
    - `WorldFile { format, label, connectors: Vec<Connector>, flows: Vec<Flow>, data: Vec<DataSource>, people: Vec<Person> }`
    - `ScenarioFile { format, label, world: String, task: String, changes: Vec<Change>, events: Vec<Event>, gates: Vec<Gate>, checks: Vec<Check> }`
    - `SetupFile { format, label, team: Team }`, where `enum Team { Builtin{agents: Vec<AgentSpec>}, Code{command: String}, Http{url: String, auth: Option<Auth>}, ClaudeCode, Codex, Madl{file: String} }` is serde-tagged by `kind`.
    - `ExperimentFile { format, label, world: String, scenario: String, setups: Vec<String>, repeats: Repeats, budget: Budget, parent: Option<String> }`
  - `Auth { key: Option<String>, env: Option<String> }`.
  - `Permissions { edit: EditPolicy /*DiffFirst|ApplyThenTell*/, spend: SpendPolicy /*AlwaysAsk|UnderCap{usd_per_day: f64}*/ }`, default `DiffFirst` + `AlwaysAsk`.
  - `fn parse<T: DeserializeOwned>(kind_name: &str, text: &str) -> Result<T, FormatError>` and `fn to_yaml<T: Serialize>(v: &T) -> Result<String, FormatError>`.
  - `FormatError { file: String, pointer: String, message: String }`.
  - `secrets::scan(value: &serde_json::Value) -> Vec<String /*json pointer*/>`, which flags strings matching credential prefixes (`sk-`, `sk-ant-`, `ghp_`, `xox`, `AKIA`, `-----BEGIN`) or any `password`/`secret`/`token`/`api_key` field holding a literal.

Field vocabulary comes from spec §1, §11 and the mockups:
- `Connector { id, label, mode: stand-in|real|replay, product?, mcp?, openapi?, auth? }`
- `Flow { id, trigger, steps: Vec<String> }`
- `DataSource` is an enum: `File{file, seeds?}` | `Database{name, engine, version, snapshot?, masked: bool, default_state: bool}` | `Import{name, from, snapshot}`. No topology.
- `Event { name, at?: String, after_calls?: u32, effects?: Vec<Value>, db?, fault?, params?: Value }`
- `Gate { rule, scope, on_breach: block|flag }`
- `Check { text, kind: goal|safety, query?: String, ends_run: bool }`

Keep each struct minimal; plans 3 and 6 extend them.

- [ ] **Step 1: Write fixtures and failing tests** in `tests/formats.rs`:
  - `round_trip_each_kind`: parse each larkspur fixture, re-serialize, re-parse, and assert equal.
  - `unknown_field_rejected`: `topology:` on a database source returns `Err` whose `pointer == "/data/0/topology"`.
  - `wrong_format_rejected`: a world file with `format: shakedown-world/2` gives an error naming the expected `shakedown-world/3`.
  - `inline_secret_rejected_without_echo`: a connector with `auth: {key: "sk-ant-abc123"}` gives an error with pointer `/connectors/0/auth/key`, and its message does not contain `abc123`.
  - `key_name_accepted`: `auth: {key: larkspur-mes}` parses.
- [ ] **Step 2:** `cd core && CARGO_BUILD_JOBS=2 cargo test -p shakedown-project --test formats`. Expected: FAIL (crate empty).
- [ ] **Step 3: Implement the formats.** `parse` runs serde through `serde_json::Value` first, so `secrets::scan` and the format check run before typed deserialization. Map serde's path into a JSON pointer using `serde_path_to_error`, adding that dependency.
- [ ] **Step 4:** Run the tests. Expected: PASS. Then run clippy and fmt for the crate.
- [ ] **Step 5: Commit:** `feat(project): typed file formats for worlds, scenarios, setups and experiments`

### Task 2: Project folder: open, validate, write

**Files:**
- Create: `core/crates/project/src/project.rs`, `src/ids.rs`, `src/validate.rs`, `src/write.rs`
- Test: `core/crates/project/tests/project.rs`

**Interfaces:**
- Consumes: Task 1 formats.
- Produces:
  - `Id(String)` with `Id::parse(&str) -> Result<Id, ProjectError>`. The pattern is `^[a-z0-9][a-z0-9-]{0,63}$`.
  - `Project { root: PathBuf, config: ProjectFile, items: BTreeMap<(Kind, Id), Item>, problems: Vec<Problem> }`
  - `Item { kind: Kind, id: Id, label: String, body: ItemBody /*enum of the four file structs*/, hash: String /*sha256 of file bytes*/ }`
  - `Problem { kind: Option<Kind>, id: Option<String>, pointer: String, message: String }`
  - `Project::open(root: &Path) -> Result<Project, ProjectError>`. Fails only if the root or `shakedown.yaml` is unreadable. Per-file errors go to `problems`.
  - `Project::init(root: &Path, name: &str, git: bool) -> Result<Project, ProjectError>`. It creates `shakedown.yaml`, the four dirs, and `.gitignore` with `.shakedown/`; with `git`, it runs `git init` via `std::process::Command`.
  - `Project::put(&mut self, kind: Kind, id: &Id, body: ItemBody) -> Result<Item, ProjectError>` and `Project::remove(&mut self, kind: Kind, id: &Id) -> Result<(), ProjectError>`.
  - `Project::list(&self, kind: Kind) -> Vec<&Item>`

Layout: worlds live in `worlds/<id>/world.yaml`, everything else in `<dir>/<id>.yaml`. A MADL setup's team file is `setups/<id>/team.madl.yaml`, referenced by `Team::Madl{file}`. Plan 3 reads it.

- [ ] **Step 1: Write failing tests:**
  - `init_creates_layout`: the dirs, `shakedown.yaml` and `.gitignore` exist; `git: false` creates no `.git`.
  - `open_lists_fixture_items`: the larkspur fixture has 1 world, 1 scenario, 3 setups and 1 experiment.
  - `rejects_unsafe_ids`: each of `"../x"`, `"a/b"`, `""`, `"A"` and a 65-character id returns `Err`, and no file is created.
  - `write_is_atomic`: `put` writes through a temp file in the same dir and renames it. The test asserts no `*.tmp` remains and the content re-parses. It also simulates a failed serialize (body with NaN budget) and asserts the original file is unchanged.
  - `dangling_reference_reported`: removing the world gives one `Problem` on the scenario with pointer `/world`; `list(Scenario)` still returns it.
  - `duplicate_world_dir_and_file`: `worlds/x.yaml` is ignored and reported as a problem, because worlds must be folders.
- [ ] **Step 2:** Run `cargo test -p shakedown-project --test project`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `validate.rs` checks cross-references: scenario→world, experiment→world/scenario/setups/parent, and the experiment's world must equal its scenario's world.
  - `write.rs` does the temp-and-rename write plus fsync of the file.
- [ ] **Step 4:** Run the tests, then clippy and fmt. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(project): open, validate and atomically write a project folder`

### Task 3: shaked holds a project

**Files:**
- Modify: `core/crates/shaked/Cargo.toml` (dependency `shakedown-project`), `src/app.rs` (`Config.project_dir: Option<PathBuf>`, `AppState.project: Arc<RwLock<Option<Project>>>`), `src/serve.rs` (read `SHAKEDOWN_PROJECT`), `src/main.rs` module doc (document the env var)
- Create: `core/crates/shaked/src/project_state.rs`
- Test: `core/crates/shaked/tests/project.rs`

**Interfaces:**
- Produces:
  - `project_state::load(dir: Option<&Path>) -> Option<Project>`. It logs and returns `None` if `open` fails.
  - `AppState::project_read(&self) -> Result<RwLockReadGuard<'_, Option<Project>>, ApiError>`, which answers 409 `"no project is open"` when `None`.
  - `AppState::with_project_mut<R>(&self, f: impl FnOnce(&mut Project) -> Result<R, ProjectError>) -> Result<R, ApiError>`. It reloads from disk first, so outside edits are seen.

- [ ] **Step 1: Write failing tests** with `AppFx` plus a builder `.project(dir)` added to `tests/support/mod.rs`:
  - `no_project_answers_409`
  - `broken_file_reported_others_load`: one scenario file contains `:::`. The project still opens with one problem, and the world still lists.
- [ ] **Step 2:** Run `cargo test -p shaked --test project`. Expected: FAIL.
- [ ] **Step 3: Implement.** Reload-on-write uses `Project::open` on the same root. Skip file watching (YAGNI); reads reload when the root's newest mtime changes, using a cached `(mtime, Project)`.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): open a project folder from SHAKEDOWN_PROJECT`

### Task 4: Operation registry and the file operations

**Files:**
- Create: `core/crates/shaked/src/ops/mod.rs` (types, `OPS`, lookup), `src/ops/project_ops.rs` (`project.status`, `project.init`), `src/ops/item_ops.rs` (generic list/get/put/delete over `Kind`)
- Test: `core/crates/shaked/tests/ops.rs`

**Interfaces:**
- Produces:
  - `Access { Read, Edit, Spend, Delete }`
  - `Operation { name: &'static str, area: &'static str, summary: &'static str, access: Access, method: Method, path: &'static str, input_schema: fn() -> serde_json::Value, run: for<'a> fn(&'a AppState, Value) -> BoxFuture<'a, Result<Value, OpError>> }`
  - `pub static OPS: &[Operation]` and `pub fn find(name: &str) -> Option<&'static Operation>`
  - `OpError { status: StatusCode, message: String }`, with `From<ProjectError>` (404 not found, 422 validation, 409 no project) and `From<OpError> for ApiError`.
- Operations, with their names, methods and paths:

| Name | Method + path | Access |
|---|---|---|
| `project.status` | `GET /api/project` | Read |
| `project.init` | `POST /api/project` | Edit |
| `worlds.list` | `GET /api/worlds` | Read |
| `worlds.get` | `GET /api/worlds/{id}` | Read |
| `worlds.put` | `PUT /api/worlds/{id}` | Edit |
| `worlds.delete` | `DELETE /api/worlds/{id}` | Delete |

  `scenarios.*`, `setups.*` and `experiments.*` follow the same pattern. Item JSON responses have the shape `{kind, id, label, body, hash, problems}`.
  - If `/api/worlds` already exists for built worlds, rename the new routes to `/api/project/worlds…`. Check `api/worlds.rs` and record the choice in the ledger.
  - `project.status` returns `{name, counts: {worlds, scenarios, setups, experiments}, problems, root}`.
  - `put` takes `{id, body}`. With `Permissions.edit == DiffFirst` and no `"confirm": true`, it returns `{proposed: true, diff: <unified diff of YAML>}` without writing. With `confirm`, it writes and returns the item. HTTP callers from the UI send `confirm: true`; plan 4 builds the assistant flow on this.

- [ ] **Step 1: Write failing tests:**
  - `names_unique_and_dotted`: every name matches `^[a-z]+\.[a-z_]+$` and no two names repeat.
  - `schemas_are_objects`: every `input_schema()` has `"type":"object"`.
  - `put_diff_first_does_not_write`: the file is unchanged and the response has a `diff` containing `+label:`.
  - `put_confirm_writes`, and `delete_removes_file`.
- [ ] **Step 2:** Run `cargo test -p shaked --test ops`. Expected: FAIL.
- [ ] **Step 3: Implement.** `item_ops` builds the four kinds from one generic set of functions parameterized by `Kind`; no copy-pasted handlers. The unified diff uses the `similar` crate, adding that dependency.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): operation registry with project file operations`

### Task 5: HTTP surface generated from the registry

**Files:**
- Create: `core/crates/shaked/src/ops/http.rs`
- Modify: `src/api.rs` (`modules()` merges `ops::http::router()`), `src/api/routes.rs` (`ROUTES` gains registry rows; add `ResourceKind::ProjectFile`), `src/auth/can.rs` (the rule for `ProjectFile`: the local owner may do everything; mirror how `World` is handled)
- Test: extend `core/crates/shaked/tests/routes.rs` and `tests/ops.rs`

**Interfaces:**
- Produces:
  - `ops::http::router() -> Router<Arc<AppState>>`, which mounts each op at its method and path. It extracts `{id}` into the input JSON, merges the query and body, and calls `run`.
  - `ops::http::route_rows() -> impl Iterator<Item = RouteCase>`, which maps `Access` to `Action`: Read→Read, Edit→Create, Spend→Start, Delete→Delete.

- [ ] **Step 1: Write failing tests:**
  - In `tests/routes.rs`: `every_op_has_a_route_row`.
  - In `tests/ops.rs`: `http_put_then_get_round_trips` against an `AppFx` with a temp project. It does PUT `/api/…/scenarios/x` with confirm, then GET, and asserts the label.
- [ ] **Step 2:** Run `cargo test -p shaked --test routes --test ops`. Expected: FAIL.
- [ ] **Step 3: Implement the router and the rows.** Keep `ROUTES` a static table for the hand-written routes and chain the registry rows where `authorize_route` looks routes up.
- [ ] **Step 4:** Run the tests. Expected: PASS. Then run the full shaked suite to file: `cargo test -p shaked > /tmp/claude-1000/ops-suite.log 2>&1; tail -5` the log. Expected: all pass.
- [ ] **Step 5: Commit:** `feat(shaked): serve registry operations over HTTP`

### Task 6: Control MCP over HTTP and stdio

**Files:**
- Create: `core/crates/shaked/src/ops/mcp.rs` (JSON-RPC dispatcher: `initialize`, `ping`, `tools/list`, `tools/call`), `src/ops/stdio.rs`
- Modify: `src/app.rs` router (mount `POST /mcp` behind authenticate), `src/cli.rs` (`Some("mcp") => stdio`), `src/mcp.rs` (move the envelope helpers `rpc_error`/`ok`/`tool_result` into `src/jsonrpc.rs`, shared by both MCP servers)
- Test: `core/crates/shaked/tests/control_mcp.rs`

**Interfaces:**
- Consumes: `OPS`, `find`, `TokenScope`.
- Produces:
  - `ops::mcp::handle(state: &AppState, caller: &Caller, req: Value) -> Option<Value>`
  - Tool names equal op names with `.` replaced by `_` (`worlds_put`), because MCP clients reject dots. Tool `inputSchema` is `input_schema()`, and `annotations.readOnlyHint` is true for Read.
  - Scope rule: Read needs `read`; Spend needs `run`; Edit and Delete need `admin`. The local owner (no token, loopback) has all.
  - `shaked mcp` reads newline-delimited JSON-RPC on stdin and writes on stdout, with `SHAKEDOWN_PROJECT` or `--project <dir>`. It runs the dispatcher in-process as the local owner, and no network server starts.

- [ ] **Step 1: Write failing tests:**
  - `tools_list_matches_registry`: the tool count equals `OPS.len()`.
  - `call_worlds_list`
  - `read_token_cannot_put`: a JSON-RPC error with code -32001 and message `"needs the admin scope"`.
  - `stdio_round_trip`: spawn the built binary with a temp project, send `initialize` then `tools/list`, and assert that `worlds_list` is present.
- [ ] **Step 2:** Run `cargo test -p shaked --test control_mcp`. Expected: FAIL.
- [ ] **Step 3: Implement the dispatcher, then the stdio loop, then move the helpers.** The per-run world MCP at `/mcp/{branch}/{agent}` is unchanged; its tests (`tests/mcp.rs`) stay green.
- [ ] **Step 4:** Run `cargo test -p shaked --test control_mcp --test mcp`. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): control MCP server over HTTP and stdio from the registry`

### Task 7: Parity gate, docs and the full suite

**Files:**
- Create: `core/crates/shaked/tests/parity.rs`
- Modify: `README.md` and `docs/` page for the API/MCP (add "Project folder" and "Use Shakedown from an agent (MCP)" sections: `SHAKEDOWN_PROJECT`, `shaked mcp`, `claude mcp add shakedown -- shaked mcp --project …`), `core/crates/shaked/src/main.rs` module doc

- [ ] **Step 1: Write the test `every_op_on_both_surfaces`:** for each op, `find(name)` works, a ROUTES row matches its method and path, and `tools/list` contains its MCP name.
- [ ] **Step 2:** Run it. Expected: PASS. If it fails, fix the surface, not the test.
- [ ] **Step 3: Run the full gate** in `core/`:
  - `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
  - `cargo test --workspace --no-fail-fast > /tmp/claude-1000/core-suite.log 2>&1`, then tail the log.
  - Expected: all clean and all pass. Run `core/scripts/leakcheck.sh`; it should report nothing left behind.
- [ ] **Step 4: Commit:** `test(shaked): every operation is served over HTTP and MCP; document the project folder`
