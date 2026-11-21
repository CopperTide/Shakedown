# Hosted Shakedown M1, revised for D16: a personal-use engine in Shakedown, the cloud layer in Cloud — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open-source `shaked` keeps only personal-use tables and code, names no org anywhere, and exposes hooks;
the CopperTide-Cloud repo plugs a cloud layer into those hooks (identity, org columns, RLS, `can()`, signing v2,
quotas, usage, the trial credit, org routes) and builds the hosted binary.

**Architecture:** Part A reworks branch `factory` (Shakedown): the migrator becomes generic (streams are names),
the base `shakedown` stream drops every org, the engine's caller is a person, an API token or a run token, and a
`Layer` trait is called at fixed points (authenticate, begin, authorize, admit, meter, fallback provider, audit,
routes, me, object prefix) with `LocalLayer` as the default. Part B, on branch `hosted-shakedown` of
CopperTide-Cloud, adds `crates/coppertide-schema` (identity, cloud and `shakedown_hosted` streams) and
`crates/shaked-hosted` (`HostedLayer` and the `shaked-hosted` binary), path-depending on Shakedown's crates. The cloud
schema is a strict superset of the base, checked mechanically.

**Tech Stack:** Rust (axum 0.8, tokio, tokio-postgres, deadpool-postgres, ring, object_store), Postgres 17,
React 18 + TypeScript + vitest, Python SDK (httpx).

**Spec:** in `~/MLSys/CopperTide/CopperTide-Cloud/docs/superpowers/specs/` (uncommitted in the main checkout; read
them there, never copy them into a worktree):
- `2026-11-19-accounts-and-schema-design.md`: "The subset rule", "The engine and the cloud layer", §3, §5, §7.
- `2026-11-19-schema-base.sql` (Part A's DDL) and `2026-11-19-schema-cloud.sql` (Part B's DDL).
- `2026-11-19-hosted-shakedown-design.md`: §M1 and Security S1–S6.
- `2026-11-19-sandbox-provider-analysis.md` §6: `SandboxBackend` is M2; nothing here assumes a live fork.

## Where things stand

Branch `factory` at a217411 has the old plan's Tasks 1–10 and the final review's fixes (ledger:
`.superpowers/sdd/2026-11-20-hosted-m1-foundation/progress.md`; every ruling and deferred minor is there). Tasks 11–14
were not started. On 2026-11-20 the owner decided (D16): "The idea of organizations and stuff should only be in the
cloud, the open source is a subset that only focuses on personal use consumption. Ideally its a strict subset of the
schema." Nothing of `factory` is on `main` or released, so its migrations can be rewritten.

**Keep, move, remove** (sources for "move" are read with `git -C ~/MLSys/CopperTide/Shakedown/.worktrees/factory
show a217411:<path>`):

| Old task | Keep in Shakedown | Move to Cloud (Part B) | Remove from Shakedown |
|---|---|---|---|
| 1 schema crate | migrator, `Conn`, checksums, advisory locks, `requires`, `also_accept`, `tests/files.rs`, `shakedown` 1, the legacy-move test and `shaked-bb8e7e8.sql` fixture | `identity` 1–2 and `bootstrap.sql` → `coppertide-schema` (B1, edited: no `local` org kind); the Cloud-0015 drift test (against Cloud's own `migrations/`); `a_missing_setting_matches_no_row` | the `Stream` enum (→ names, A2); the copied `fixtures/cloud-0015` |
| 2 pool, seed | deadpool pool, template test databases, `jobs::interrupt_at_start`, the test guard (`SHAKEDOWN_DB`, `SHAKEDOWN_TEST_GUARD`) | — | the local seed (`LOCAL_USER_ID`, `LOCAL_ORG_ID`), the roles check and bootstrap run, `set_role`, the identity search-path entries |
| 3 bind, modes | `SHAKEDOWN_BIND`, `SHAKEDOWN_PUBLIC_URL`, allowed hosts, the package and connection refusals (now by `Policy`) | the hosted gate (B3, B4) | `--hosted`, `SHAKEDOWN_MODE`, `Mode::Hosted` (A1) |
| 4 `can()` | run-token reach and token-scope caps, as the engine's own checks (A3) | the org matrix, levels, seats, grants, principals → `HostedLayer` (B3, B5) | `OrgRole`, `SeatRole`, `OrgFlags`, `Grant` in shaked |
| 5 RLS, `tx` | `Store::tx`, `system_tx`, `Tx` (now with the `begin` hook) | org columns, RLS, grants, quotas, usage, sandboxes, org settings → `shakedown_hosted` 1 (B2); `tests/rls.rs` (B2); the GUCs → `begin` (B3) | `Scope`, `test_scope`, `shakedown/0002_orgs.sql` |
| 6 scoped store | `Store::at`, the caller extractor, child-through-root joins, `BranchCtx` carrying its starter | — | every `org_id` in engine SQL, `shakedown/0003_validate.sql`, `tests/support/orgs.rs`, `tests/fixtures/orgs.toml` |
| 7 hosted auth | `/api/me` (base shape) | `auth/signing.rs` (v2 and its vector), `auth/identity.rs`, `open_hosted`, `DbUrls`, the hosted-login check, `tests/auth.rs` → B3, B4 | — |
| 8 API tokens | `sd_` tokens, hashing, scopes, list and revoke, `shaked token create`, SDK `token=` | the token's org and holder, `auth/audit.rs` rows, the viewer rules, the hosted cases of `tests/tokens.rs` → B4, B5 | the ruling "a Bearer header is ignored locally" |
| 9 agent and run tokens | all of it: carriage per kind, roster, ticket expiry, pause extension, output redaction, run-token reach | run tokens' org (a cloud column), the deleted-org check (B4) | — |
| 10 quotas, usage | the per-run spend cap (`budget.rs`, older than this plan) | `quotas.rs`, `store/usage.rs`, `api/usage.rs`, `docs/quotas.toml`, `tests/quotas.rs` → B7 | the quota and meter calls in `api.rs` and `gateway.rs` (→ `admit` and `meter`, A4) |

## Calls made

| Call | Why | Cost if wrong |
|---|---|---|
| `factory`'s `identity` 1–2 and `shakedown` 2–3 are retired, not reverted by a migration. `shakedown` 1 stays byte-identical. A database that applied a retired file is refused at start with a message naming step O1. | Unreleased; a reverting migration would put org DDL into open source. | The few dev and test databases that applied them are converted by hand once (O1). |
| The crate becomes `shakedown-schema` (`core/crates/shakedown-schema`). Cloud's crate of the cloud streams keeps the name `coppertide-schema`. The advisory-lock key stays `'coppertide-schema:' \|\| stream`. | It now holds only Shakedown's stream and a generic migrator; D22 "one crate" is Cloud's crate, which lists every stream of the one database. The key keeps mixed revisions locking alike. | A rename in two Cargo files. |
| Hosted-only tables live in Postgres schema `shakedown_hosted`; in schema `shakedown` the cloud adds only `org_id`, `team_id`, `created_by` (and two `api_tokens` holder columns), constraints, indexes, policies. | The subset check stays a plain catalog diff. | One more schema on the search path. |
| The base declares no UNIQUE on a name. shaked checks name clashes under `pg_advisory_xact_lock(hashtext('<table>:name:' \|\| name))`; the cloud adds `UNIQUE (org_id, name)`. | A base UNIQUE (name) would forbid two orgs the same key name, and the cloud may not drop base constraints. | A code check instead of a constraint locally (single user). |
| The cloud fills its columns with `DEFAULT authz.org_id()` etc. (NULL when unset), not `current_setting('app.org_id')::uuid`. | Same fail-closed insert, without "unrecognized parameter" or `''::uuid` errors after an earlier transaction on the same connection. | None. |
| The layer is `Arc<dyn Layer>` with boxed-future methods, held by `Store` (every path reaches it); `AppState::layer()` delegates. | No generic parameter through `AppState`, and no `async-trait` crate (`futures-util` is already a dependency). | One allocation per hook call. |
| Engine queries carry no org filter; the twice-enforced rule is the layer's `authorize` at every route plus RLS, as the owner approved. The engine's own checks (token scope, run-token reach) run before the layer's. | The engine has no org. | A forgotten `authorize` is caught by the deny-everything test (A4), a forgotten filter by RLS. |
| A local Bearer token, when sent, is checked and capped (401 bad or revoked; `read` cannot write); no Bearer is the person at the keyboard. | Makes `read` tokens meaningful locally and lets the base test run-token reach without a cloud. Owner decision needed (accounts design, question 3). | Revert one branch of `LocalLayer::authenticate`. |
| Provider keys are sealed to `provider_key:<id>:key` (row id), replacing "bound to org and name". | The base cannot name an org; a row id is unique in the cloud and never changes. Owner question 2. | Re-seal on first use if the owner wants the old context. |
| The hosted binary is built in Cloud from `crates/shaked-hosted`, which calls `shaked::serve::run`; the open-source binary has no hosted code path and refuses a non-loopback bind. | D16. | None. |
| Cloud's crates form a virtual workspace at `crates/Cargo.toml` (own `Cargo.lock`, `crates/target`), not members of the gateway package. | The gateway's Docker build copies only its own files; a workspace change there would break `main`'s deploy. | One more lockfile. |
| Cloud path-depends on `../../../shakedown/core/crates/{shaked,shakedown-schema}` (the server layout, `/opt/sculptured/repos/shakedown`). Locally a symlink `CopperTide-Cloud/.worktrees/shakedown` → this worktree. | Same pattern as the gateway's `../sculptured/crates/signing`. | M3 adds the Shakedown repo to update.sh's `REPOS`. |
| `signing` v2 lives in `crates/shaked-hosted/src/signing.rs` (moved from shaked), exported for the gateway's later `/sd/*` route, with the same fixed vector. | Both signer and verifier are Cloud code now; no change to the CopperTide repo's `signing` crate. | The gateway imports it from there in M3. |
| D23's trial is one $30 credit per org (each user's personal org gets one). Preset `trial_usd_micros = 30000000`, period `none`, hard. | "Per org or user": per org covers every user through their personal org. Owner question 1. | One preset line. |

## Global Constraints

- **Part A** works in `~/MLSys/CopperTide/Shakedown/.worktrees/factory`, branch `factory`. **Part B** works in
  `~/MLSys/CopperTide/CopperTide-Cloud/.worktrees/hosted-shakedown`, branch `hosted-shakedown`, created from
  `origin/main` (B1, step 1). Never edit, stage or commit in the main Cloud checkout: it holds someone else's
  uncommitted edits.
- Gate, Part A: in `core/`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace
  --no-fail-fast`; UI `npx tsc -b && npx vitest run --maxWorkers=1`; SDK `uv run --all-extras pytest -q` in
  `sdk/python`. Gate, Part B: in `crates/`, the same two cargo commands. Postgres for tests is container
  `deploy-pg-1` on 127.0.0.1:5544 (`docker start deploy-pg-1`); root-workspace and Cloud tests use
  `SHAKEDOWN_PG_URL=postgres://arena:arena@127.0.0.1:5544/postgres` (ledger baseline ruling).
- Shared machine: `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, one cargo build at a time, output to a file and
  read its tail. Never start `shaked` while tests run. When Part B ends, `rm -rf crates/target` in the Cloud worktree.
- clippy `too_many_lines` = 30, enforced by `-D warnings`. Never `#[allow]` it. Cloud's crates copy Shakedown's
  `clippy.toml` into `crates/clippy.toml`.
- New dependencies, ruled: `object_store` with features `aws` + `fs` (A6), `http` (already through axum, for
  `Extensions`). Cloud crates: only what the moved code already used (`ring`, `hex`-free hex helpers as in shaked,
  `toml`, `chrono`, `uuid`, `reqwest`). Nothing else without a ledger line.
- Open source names no org: `core/crates/shaked/tests/no_orgs.rs` (A3) fails on `org_id`, `\borgs?\b`, `tenant`,
  `membership`, `ROW LEVEL SECURITY` or `set_config('app.` in `core/crates/shaked/src`,
  `core/crates/shakedown-schema/{src,migrations}`, `ui/src` or `sdk/python/src`, except lines listed in
  `core/crates/shaked/tests/no_orgs_allow.txt`.
- The subset rule (accounts design): no cloud migration changes or removes a base table, column, constraint or
  index; `crates/coppertide-schema/tests/subset.rs` (B2) checks it.
- Migrations are append-only. `DROP`, `RENAME`, `SET NOT NULL` or `TYPE` needs `-- contract: unused since <sha>`.
  Every migration sets no timeouts itself (the migrator sets `lock_timeout = '5s'`, `statement_timeout = '60s'`).
- Tokens and sealed values never reach events, logs, error messages, recorded driver output or exports. Lists show
  `token_prefix` and `key_last4` only.
- Tests: never a real model; Docker-only tests are `#[ignore]` and named `live_*`; every database, container, login
  and folder a test makes is dropped, even when it fails; no test opens the default databases (the guard stays).
  Cloud tests connect through `SET ROLE shaked_app`.
- No hard-coded domain data: quota presets, fixtures and org names go in data files.
- Commits: plain sentences in the repo's style, `git add <paths>` only (another agent commits in the Shakedown
  worktree), ending with the session's attribution lines. No push, no merge to `main`.

## Review Focus

1. **Old local databases.** Records in `public`, in a database named `arena` or `shakedown`, made by any released
   `shaked`, move into schema `shakedown` on the first start; every run still lists, loads and forks, and a second
   start changes nothing. Test: A3, `an_old_public_database_moves_to_shakedown_and_keeps_its_runs` (kept).
2. **A database migrated by the retired `factory` build.** It is refused before anything is written, with a message
   that names step O1 and no connection string. Test: A3,
   `a_database_from_the_retired_factory_build_is_refused_naming_the_fix`.
3. **A route added later that never asks the layer.** With a layer that denies everything, every `/api` route but
   the public ones answers 404, and the route table names every route. Test: A4, `every_api_route_asks_the_layer`.
4. **A base change the cloud was not written for** (a new table, a UNIQUE on a name). Cloud's tests fail, and a
   hosted start on a database whose newest base table has no wall refuses to serve. Tests: B2, `subset_holds` and
   `every_shakedown_table_is_walled`; B3, `hosted_start_refuses_a_newest_base_table_without_a_wall`.
5. **An insert with no org setting** (a system job or a forgotten `begin`). It fails `NOT NULL`, and never lands in
   another org. Test: B2, `an_insert_without_an_org_setting_fails`.

## Step O1 (owner or coordinator, not an agent task): databases that applied the retired migrations

Before A3's gate. Agents never touch these databases.
- `shakedown_tpl_*` (test templates): stale once checksums change; the test harness builds a new one and drops stale
  ones itself (Task 2 ruling). Nothing to do.
- `shakedown_sdk_live` (the SDK's live-test database): drop it; the next SDK run recreates it.
- `arena` on deploy-pg-1 (shared dev database, 14 runs, backup `work-local/db-backups/arena-20261120T0638Z.dump`):
  take a fresh dump; then drop schemas `identity` and `authz`, drop the policies and RLS flags on schema `shakedown`,
  drop the tables the retired `shakedown` 2 created (`provider_keys`, `worlds`, `org_settings`, `saves`,
  `sandboxes`, `api_tokens`, `run_tokens`, `grants`, `quotas`, `usage`, `usage_monthly`), drop the columns it added
  (`org_id`, `team_id`, `created_by`, `world_id`, `placement`, `sealed_env`) and its constraints, and delete the
  `schema_meta.migrations` rows of stream `identity` and of `shakedown` 2 and 3. The next `shaked` applies the new
  `shakedown` 2. Any other database on deploy-pg-1 with those `schema_meta` rows: the same.

---

## Part A: Shakedown repo, branch `factory`

### Task A1: Take hosted mode out of open source

**Files:**
- Delete: `core/crates/shaked/src/auth/signing.rs`, `src/quotas.rs`, `src/store/usage.rs`, `src/api/usage.rs`,
  `docs/quotas.toml`, `core/crates/shaked/tests/auth.rs`, `tests/quotas.rs`.
- Modify: `src/auth/middleware.rs` (keep only the local layer), `src/auth/identity.rs` (drop `orgs_of_user`),
  `src/auth/system.rs` (drop `ListOrgsOfUser`), `src/mode.rs` (no `Mode`; `bind_from(get, serves_others: bool)`),
  `src/app.rs` (`Config` loses `mode`, `gateway_secret`; one `/api` layer), `src/main.rs` (no hosted branch, no
  `gateway_secret()`), `src/store/open.rs` (drop `DbUrls`, `HOSTED_PLAN`, `migrate_hosted`, `refuse_bypassing_login`),
  `src/store.rs` (`open_hosted` gone), `src/api.rs` (no quota check or meter in `create_run`, no `usage::router`),
  `src/gateway.rs` (no monthly quota check or meter), `src/api/me.rs` (answers `{mode: "local", user: {name: "You"},
  features: []}`).
- Modify tests: `tests/modes.rs` (drop the hosted cases; `a_public_bind_needs_hosted` becomes
  `a_public_bind_is_refused`), `tests/names.rs` (drop `hosted_mode_opens_its_three_logins`), `tests/schema.rs` (drop
  `open_hosted` and hosted-login tests), `tests/tokens.rs` and `tests/run_tokens.rs` (drop the cases that call
  `AppSetup::hosted`), `tests/support/app.rs` (drop `AppSetup::hosted`).

**Interfaces:**
- Produces: `pub fn bind_from(get: impl Fn(&str) -> Option<String>, serves_others: bool) -> Result<Bind, String>`
  (error for a non-loopback bind when `serves_others` is false: "binding beyond this machine needs a layer that
  authenticates callers; the open-source shaked serves only this machine"). A4 passes `layer.policy().serves_others`.

- [ ] **Step 1: Change the tests first:** delete the files and cases above; rename and rewrite
  `a_public_bind_is_refused`:

```rust
#[test] fn a_public_bind_is_refused() {
    let e = bind_from(env(&[("SHAKEDOWN_BIND", "0.0.0.0:4510")]), false).unwrap_err();
    assert!(e.contains("serves only this machine"), "{e}");
}
#[tokio::test] async fn me_is_personal() { /* GET /api/me => 200 {mode:"local", user:{name:"You"}, features:[]} */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test modes --test api` (compile error: the new
  `bind_from` arity; `me_is_personal` fails on the old body).
- [ ] **Step 3: Implement** the removals and the two changes.
- [ ] **Step 4: Run the gate.** Expect the old counts minus the deleted tests; the ledger records both.
- [ ] **Step 5: Commit:** "The open-source shaked has no hosted mode: no signed requests, quotas or usage meters, and a bind beyond this machine is refused".

### Task A2: A generic migrator in the `shakedown-schema` crate

**Files:**
- Move: `core/crates/coppertide-schema` → `core/crates/shakedown-schema` (`git mv`); package `shakedown-schema`, lib
  `shakedown_schema`. Modify `core/Cargo.toml` (members), `Cargo.lock`, `core/crates/shaked/Cargo.toml`, every
  `coppertide_schema::` use, the bootstrap hint's path.
- Modify: `src/stream.rs` (`Stream` is a name; `Files` sets), `src/migrator.rs` (every entry point takes the
  migrations; `META_DDL` checks the name pattern), `src/lib.rs`.
- Create: `tests/fixtures/extra/0001_extra.sql` (a third stream for tests: `CREATE SCHEMA extra; CREATE TABLE
  extra.t (id int)`, `-- requires: shakedown >= 1`).
- Test: `tests/files.rs`, `tests/migrate.rs` (+2).

**Interfaces:**
- Produces:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stream(pub &'static str);                   // ^[a-z][a-z_]{0,31}$ (checked by load())
pub const SHAKEDOWN: Stream = Stream("shakedown");
pub struct Files { pub stream: Stream, pub files: &'static [(&'static str, &'static str)],   // (file name, text)
                   pub also_accept: &'static [(u32, &'static [[u8; 32]])] }
pub fn load(sets: &[&Files]) -> Result<Vec<Migration>, Error>;   // parsed, sorted (stream, version); BadFile on a bad name
pub fn base() -> &'static [Migration];                 // LazyLock over load(&[&BASE]); BASE is the shakedown stream
pub async fn migrate(c: &mut impl Conn, plan: &Plan<'_>, set: &[Migration]) -> Result<Report, Error>;
pub async fn check(c: &mut impl Conn, plan: &Plan<'_>, set: &[Migration]) -> Result<Report, Error>;
pub async fn wait_ready(c: &mut impl Conn, plan: &Plan<'_>, set: &[Migration], every: Duration, give_up: Duration) -> Result<Report, Error>;
// Migration, Applied, Plan, Report, Error, Conn, BOOTSTRAP_SQL: as before, with Stream the new type.
```
- `META_DDL`'s check becomes `CHECK (stream ~ '^[a-z][a-z_]{0,31}$')`. Until A3 the crate also exports
  `pub fn retiring_identity() -> &'static [Migration]` (the two identity files) so shaked keeps working; A3 deletes it.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/migrate.rs
#[tokio::test] async fn a_stream_the_crate_does_not_know_applies_after_the_base() {
    // EXTRA = Files{stream: Stream("extra"), files: &[("0001_extra.sql", include_str!("fixtures/extra/0001_extra.sql"))], ..}
    // set = load(&[&BASE_FILES, &EXTRA]); migrate(Plan{apply: &[SHAKEDOWN, Stream("extra")], ..}, &set)
    // => schema_meta has ("extra", 1); to_regclass('extra.t') is not null
}
#[tokio::test] async fn a_binary_that_knows_only_the_base_reports_the_other_stream_as_newer_not_an_error() {
    // after the above: migrate(Plan{apply:&[SHAKEDOWN], require: &[]}, base()) => Ok, report.applied empty
}
// tests/files.rs: every existing test runs over every embedded set, plus:
#[test] fn stream_names_are_checked() { assert!(matches!(load(&[&Files{stream: Stream("Bad-Name"), ..}]), Err(Error::BadFile(_)))) }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shakedown-schema` (does not compile: crate renamed, no `load`).
- [ ] **Step 3: Implement** the interfaces above. `migrate_set` disappears into `migrate`.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "The schema crate is shakedown-schema: its migrator knows no stream names and applies whatever sets a binary passes, so another repo can add streams on top of Shakedown's".

### Task A3: Personal-use schema and engine: no org anywhere in open source

**Files:**
- Delete: `core/crates/shakedown-schema/migrations/identity/`, `migrations/shakedown/0002_orgs.sql`,
  `0003_validate.sql`, `bootstrap.sql`, `tests/fixtures/cloud-0015/`, the identity tests in `tests/migrate.rs`
  (`identity_1_create_path_matches_cloud_0015_moved_path`, `a_missing_setting_matches_no_row`), `retiring_identity`.
- Create: `migrations/shakedown/0002_keys_worlds_saves_tokens.sql`: section "shakedown 2" of `schema-base.sql`
  (`ALTER TABLE` for `runs.world_id`, `runs.placement`, `connections.sealed_env`; `CREATE TABLE` for the five
  tables; the indexes). Create `src/retired.rs`: the retired checksums below.
- Delete in shaked: `src/store/seed.rs`, `src/auth/scope.rs`, `src/auth/identity.rs`, `src/auth/audit.rs`,
  `src/auth/principal.rs`; `tests/rls.rs`, `tests/support/orgs.rs`, `tests/fixtures/orgs.toml`.
- Create in shaked: `src/auth/caller.rs`; rewrite `src/auth/can.rs` (engine checks only); `tests/no_orgs.rs`,
  `tests/no_orgs_allow.txt`, `tests/fixtures/retired/` (the five a217411 files: `identity/0001_baseline.sql`,
  `identity/0002_orgs.sql`, `shakedown/0002_orgs.sql`, `shakedown/0003_validate.sql`, and the bootstrap, as test data).
- Modify in shaked: `src/store/open.rs` (no roles, no seed, no identity: migrate `base()`, `ALTER DATABASE … SET
  search_path = shakedown, public`), `src/store/tx.rs` (no GUCs, no `SET ROLE`), `src/store.rs` and `store/*.rs`
  (no `org_id` in any SQL; `BRANCHES_OF_RUNS` joins on `r.id = b.run_id` only), `src/store/api_tokens.rs` and
  `run_tokens.rs` (no org; no audit), `src/auth/{mod,extract,middleware,tokens,run_tokens,system}.rs`, `src/exec.rs`
  (`BranchCtx.caller: Arc<Caller>`), every `Scope` user (`api/*`, runners, suites, experiments, review, dispatch,
  gc), `tests/support/{mod,pg,app}.rs` (`records(&store)` = `store.at(&Caller::person())`; no `identity_only`).
- Test: `tests/schema.rs`, `tests/tokens.rs`, `tests/run_tokens.rs`, `tests/store.rs`, unit tests in `can.rs`.

**Interfaces:**
- Consumes: A2 `base()`, `migrate`.
- Produces:
```rust
// auth/caller.rs
#[derive(Debug, Clone)] pub struct Caller { pub kind: CallerKind, pub ext: http::Extensions }
#[derive(Debug, Clone, PartialEq)] pub enum CallerKind {
    Person,                                                     // locally: whoever sits at this machine
    ApiToken { id: Uuid, scopes: Vec<TokenScope> },
    RunToken { run: String, branch: String, kind: RunTokenKind, agent: Option<String> },
}
impl Caller { pub fn person() -> Caller; pub fn api_token(id: Uuid, scopes: Vec<TokenScope>) -> Caller;
              pub fn run_token(run: &str, branch: &str, kind: RunTokenKind, agent: Option<&str>) -> Caller; }
pub enum TokenScope { Read, Run, Admin }                       // unchanged names and parsing
pub enum RunTokenKind { Run, Agent, Worker, Ticket }           // unchanged
// auth/can.rs: the engine's own checks, before any layer
pub enum Action { Read, Create, Start, Fork, Stop, Record, UseInRun, Rename, ChangeSetup, ReplaceSecret, DeleteFork,
                  Delete, ManageTokens, InstanceAdmin, RegisterAgent, Report, Finish, CallTools, CallModel, WorkerLink }
pub enum ResourceKind { Run, Experiment, Suite, World, ProviderKey, Connection, Review, ApiToken }
pub enum Target<'a> { Resource { kind: ResourceKind, id: &'a str }, Branch { run: &'a str, branch: &'a str },
                      Kind(ResourceKind), Instance }
pub enum Decision { Allow, NotFound, Forbidden(String) }
pub fn engine_check(c: &Caller, a: &Action, t: &Target<'_>) -> Decision;
// Person: Allow. ApiToken: read => Read only; run => also Create/Start/Fork/Stop/Record/UseInRun/Rename/ChangeSetup/
// DeleteFork; admin => everything. RunToken: RegisterAgent/Report/Finish (kind Run), WorkerLink (Worker, Ticket),
// CallTools/CallModel (Agent) on Target::Branch whose branch is its own; another branch NotFound; anything else Forbidden.
// auth/extract.rs
pub struct Who(pub Arc<Caller>);                               // axum extractor; 401 "no caller" when missing
// store
impl Store { pub fn at<'a>(&'a self, c: &'a Caller) -> Scoped<'a>;          // as before, a Caller instead of a Scope
             pub async fn tx(&self, c: &Caller) -> Result<Tx>; pub async fn system_tx(&self, job: SystemJob) -> Result<Tx>; }
pub enum SystemJob { InterruptAtStart, StartupGc, ResolveToken, StoreCopy }
// shakedown-schema src/retired.rs
pub const RETIRED: &[(&str, u32, &str)] = &[            // (stream, version, sha256 hex) of factory files never released
    ("shakedown", 2, "34552019c2a5a894b68b5c1007c139dbf5bfd559a88f37cca0811394dc31c68b"),
    ("shakedown", 3, "24fce61a85a57593c4cdb503551bc810349315eeeac66ce9c68c240ab050d3b5"),
    ("identity",  1, "2c29d43e2e4da0ed602400d0e5b34947373e2784bf3d19274bf41f712a1b778f"),
    ("identity",  2, "16e522f6d2dcfa5034004d87457d499a3d74cbce6c8b0335847430599f22cc99"),
];
pub fn retired_in(applied: &[Applied]) -> bool;
```
- The local `/api` middleware puts `Arc::new(Caller::person())` on every request past the Host guard (A4 makes it a
  hook). `Store` holds `pool: Pool, system: Option<Pool>`; `system_tx` uses `system` when set, else `pool`.
- `Store::open_named`: before migrating, if `retired_in(applied)`, fail with "this database was migrated by an
  unreleased factory build of shaked (2026-11-20); convert it as docs/superpowers/plans/2026-11-20-hosted-m1-
  foundation.md, step O1, says, or point SHAKEDOWN_DB at another database".

- [ ] **Step 1: Write the failing tests**

```rust
// tests/schema.rs
#[tokio::test] async fn a_fresh_database_has_only_shakedown_s_schemas() {
    // SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg_%' AND nspname <> 'information_schema'
    // == {"public", "schema_meta", "shakedown"}; schema_meta rows == [("shakedown",1), ("shakedown",2)]
}
#[tokio::test] async fn a_plain_login_that_owns_its_database_opens_it() {
    // TempLogin LOGIN CREATEDB (not superuser) creates its db; Store::open_named => Ok; create_run works
}
#[tokio::test] async fn a_database_from_the_retired_factory_build_is_refused_naming_the_fix() {
    // apply tests/fixtures/retired/* as a superuser, record them in schema_meta with their sha256 (as the old binary did)
    // Store::open_named => Err; msg contains "unreleased factory build" and "step O1"; msg has no "postgres://"
    // schema_meta row count unchanged
}
// tests/no_orgs.rs
#[test] fn open_source_names_no_orgs() {
    // walk the four trees; regex (?i)org_id|\borgs?\b|tenant|membership|row level security|set_config\('app\.
    // every hit must be a line in no_orgs_allow.txt ("<path>:<trimmed line>"); and every allow line must still hit
}
// tests/tokens.rs (local now)
#[tokio::test] async fn a_token_is_shown_once_and_stored_hashed() { /* as before, local */ }
#[tokio::test] async fn cli_token_create_prints_a_working_token() { /* as before */ }
// can.rs unit tests
#[test] fn read_tokens_only_read() {
    let c = Caller::api_token(Uuid::nil(), vec![TokenScope::Read]);
    assert_eq!(engine_check(&c, &Action::Read, &Target::Kind(ResourceKind::Run)), Decision::Allow);
    assert!(matches!(engine_check(&c, &Action::Start, &Target::Kind(ResourceKind::Run)), Decision::Forbidden(_)));
}
#[test] fn run_tokens_reach_only_their_branch() {
    let c = Caller::run_token("r", "b1", RunTokenKind::Run, None);
    assert_eq!(engine_check(&c, &Action::Finish, &Target::Branch { run: "r", branch: "b1" }), Decision::Allow);
    assert_eq!(engine_check(&c, &Action::Finish, &Target::Branch { run: "r", branch: "b2" }), Decision::NotFound);
    assert!(matches!(engine_check(&c, &Action::Read, &Target::Kind(ResourceKind::Run)), Decision::Forbidden(_)));
}
#[test] fn an_agent_token_calls_tools_and_models_only() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test schema --test no_orgs --lib auth::can` and
  `cargo test -p shakedown-schema`.
- [ ] **Step 3: Implement** in one compiler-driven sweep (as old Task 6): delete the org types, let the compiler
  list every use, replace each with `Caller`. Keep every existing assertion; tests that built org scopes use
  `Caller::person()`. The SQL in each store method loses its `org_id` parameter and filter, nothing else.
- [ ] **Step 4: Run the gate.** Needs step O1 done first for the SDK run (`shakedown_sdk_live`). Check with
  `core/scripts/leakcheck.sh` that no test database or login is left.
- [ ] **Step 5: Commit:** "Open-source Shakedown keeps only personal-use tables: no identity stream, roles, org columns or row-level security, and a database from the unreleased factory build is refused with the way to convert it".

### Task A4: The layer: hooks the engine calls, `LocalLayer`, the library entry point and a harness for embedders

**Files:**
- Create: `core/crates/shaked/src/layer.rs` (trait and `LocalLayer`), `src/layer/types.rs` (`Policy`, `Costly`,
  `Admission`, `Usage`, `Meter`, `KeySource`, `ModelWant`, `Fallback`, `AuditEvent`, `MeExtra`, `Refusal`, `TxFor`),
  `src/serve.rs` (main's start-up moved into the library), `src/harness.rs` (feature `harness`).
- Modify: `src/main.rs` (`shaked::serve::run(Arc::new(LocalLayer), …)`), `src/app.rs` (`/api` = engine routes merged
  with `layer.routes()`, one auth middleware; `/api/health` outside it), `src/auth/middleware.rs` (Bearer → engine
  resolution → `layer.authenticate`), `src/auth/tokens.rs` and `run_tokens.rs` (resolve to a `Caller` through
  `system_tx(ResolveToken)`), `src/store/tx.rs` (`begin` after `BEGIN`), `src/store.rs` (`Store::with_layer`,
  `from_pools`, `layer()`), `src/api.rs` and `src/api/*.rs` (every handler calls `authorize` before acting),
  `src/api/tokens.rs` (`audit` on create and revoke), `src/api/me.rs` (`me` hook), `src/api.rs::create_run` and
  `src/fork/start.rs` (`admit` then `meter` `runs`), `src/gateway.rs` (`admit` `ModelCall`, `meter`
  `ModelUsdMicros` and `ModelTokens` per recorded call, key `gateway:<branch>:<seq>` and `…:tokens`),
  `src/providers/mod.rs` (`resolve` falls back to `layer.fallback_provider`; `LocalLayer`'s answer is today's env
  lookup), `src/mode.rs` (`bind_from` called with `policy.serves_others`; `local_only` reads
  `policy.runs_packages_here`), `core/crates/shaked/Cargo.toml` (feature `harness`; dev-dependency on itself with it).
- Test: `core/crates/shaked/tests/layer.rs`, `tests/routes.rs`.

**Interfaces:**
- Consumes: A3 `Caller`, `engine_check`, `Action`, `Target`, `Decision`, `Tx`, `SystemJob`.
- Produces: the trait exactly as in the accounts design, "The engine and the cloud layer", and:
```rust
pub type BoxFuture<'a, T> = futures_util::future::BoxFuture<'a, T>;
pub struct Policy { pub serves_others: bool, pub private_model_urls: bool, pub runs_packages_here: bool }
pub struct Refusal { pub status: StatusCode, pub message: String }
pub enum Meter { Runs, ModelUsdMicros, ModelTokens, SavesBytes, SandboxSeconds }
pub enum KeySource { Own(Uuid), Environment, Fallback(&'static str) }
pub struct ModelWant<'a> { pub provider: Option<&'a str>, pub model: &'a str }
pub struct Fallback { pub provider: Provider, pub source: &'static str }
pub struct AuditEvent<'a> { pub action: &'static str, pub target: (&'static str, &'a str), pub detail: serde_json::Value }
pub struct MeExtra { pub mode: &'static str, pub features: Vec<String>, pub account: Option<serde_json::Value> }
pub struct LocalLayer;                                  // policy {false, true, true}; defaults per the spec's table
// The one helper every handler uses: engine_check, then layer.authorize; NotFound => 404, Forbidden => 403.
pub async fn authorize(state: &AppState, who: &Who, a: Action, t: Target<'_>) -> Result<(), ApiError>;
impl Store { pub fn with_layer(self, l: Arc<dyn Layer>) -> Store; pub fn from_pools(app: Pool, system: Option<Pool>, l: Arc<dyn Layer>) -> Store;
             pub fn layer(&self) -> &Arc<dyn Layer>; }
// serve.rs
pub struct ServeOptions { /* bind, worlds dir, hub settings: what main.rs read from the environment */ }
impl ServeOptions { pub fn from_env(policy: &Policy) -> anyhow::Result<ServeOptions>; }
pub async fn run(store: Store, opts: ServeOptions) -> anyhow::Result<()>;   // the layer comes with the store
// harness.rs (feature "harness", for this crate's tests and Cloud's)
pub const ROUTES: &[RouteCase];                         // every /api route: method, path, the Action and target kind it asks
pub struct RouteCase { pub method: &'static str, pub path: &'static str, pub action: Action, pub public: bool }
pub struct SpyLayer { /* records every hook call; answers set per test */ }
pub async fn scratch_db(tag: &str, sets: &[Migration]) -> ScratchDb;   // template per set checksum; dropped on Drop; guard-aware
pub async fn serve_app(store: Store) -> (String /* base url */, ServeHandle);
pub use crate::testkit_fake_llm::FakeLlm;               // the fake model server the gateway tests use
```
- Public routes (no `authorize`): `GET /api/health`, `GET /api/me`. MCP and gateway routes are authenticated by
  agent tokens; their transactions, `admit` and `meter` use `BranchCtx.caller`.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/routes.rs
#[test] fn the_route_table_names_every_route() {
    // scan src/api.rs, src/api/*.rs for `.route("…"` (with method chains); each (method, path) is in harness::ROUTES and back
}
#[tokio::test] async fn every_api_route_asks_the_layer() {
    let spy = SpyLayer::deny(Decision::NotFound);
    for r in ROUTES.iter().filter(|r| !r.public) {
        let s = call(&app_with(spy.clone()), r.method, &fill(r.path), body_for(r.path)).await.status();
        assert_eq!(s, 404, "{} {}", r.method, r.path);
        assert!(spy.asked(r.action), "{} {} never asked for {:?}", r.method, r.path, r.action);
    }
}
// tests/layer.rs
#[tokio::test] async fn begin_runs_in_every_transaction_with_its_caller() {
    // spy counts begin calls: one per store call; interrupt_at_start => TxFor::System(InterruptAtStart)
}
#[tokio::test] async fn a_refused_admission_starts_nothing() {
    // spy admit(StartRun) => Refuse{429, "runs quota reached"}; POST /api/runs => 429, body error == that; list_runs empty
}
#[tokio::test] async fn every_recorded_model_call_is_metered_once() {
    // FakeLlm through the gateway, one call => spy meters [ModelUsdMicros key gateway:<b>:<seq>, ModelTokens key …:tokens]; resend => no new
}
#[tokio::test] async fn an_agent_without_a_key_gets_the_layer_fallback() { /* spy fallback => FakeLlm provider; call succeeds; KeySource::Fallback */ }
#[tokio::test] async fn local_bearer_tokens_are_checked_and_capped() {
    // no header => 200; sd_ read token: GET /api/runs 200, POST /api/runs 403; revoked => 401; unknown => 401
    // sdr_ token of branch b1: POST /api/branches/b2/finish => 404; GET /api/runs => 403
}
#[tokio::test] async fn layer_routes_sit_behind_authenticate() { /* spy routes /api/spy; authenticate refuses => 401; allows => 200 */ }
#[tokio::test] async fn token_changes_are_audited_through_the_layer_without_the_secret() {}
#[tokio::test] async fn health_needs_no_caller() { /* spy authenticate refuses everything; GET /api/health => 200 */ }
#[test] fn a_public_bind_needs_a_layer_that_serves_others() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --features harness --test routes --test layer`
- [ ] **Step 3: Implement** the interfaces above, route by route; each handler's (Action, Target) is its
  `ROUTES` row. `LocalLayer::authenticate` returns `found` or `Caller::person()`.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "shaked calls a layer at fixed points (who calls, every transaction, every route, costly steps, metering, fallback models, audit, extra routes) and the open-source binary passes the personal-use LocalLayer".

### Task A5: Your own model keys and OpenAI-compatible servers

**Files:**
- Create: `core/crates/shaked/src/vault.rs` (port of Cloud `origin/main:src/vault.rs`: format v1, AES-256-GCM,
  `SHAKEDOWN_VAULT_KEY` / `SHAKEDOWN_VAULT_KEY_FILE`, else `<repo>/core/vault.key`, created 0600 on first seal and
  listed in `.gitignore`), `src/store/provider_keys.rs`, `src/api/models.rs`, `src/providers/keys.rs`,
  `src/providers/reach.rs` (public-address check).
- Modify: `src/providers/mod.rs` (`resolve`: your key, then `layer.fallback_provider`), `src/gateway.rs`,
  `src/agents/open.rs` (resolve with `ctx.caller`), `src/api.rs` (`/api/providers` lists your keys beside the
  environment's), `src/harness.rs` (`ROUTES` rows for the new routes).
- Test: `core/crates/shaked/tests/provider_keys.rs`; unit tests in `vault.rs`, `reach.rs`.

**Interfaces:**
- Consumes: A4 `fallback_provider`, `authorize`, `audit`, `Policy::private_model_urls`.
- Produces:
```rust
pub struct Vault; impl Vault { pub fn from_hex(h: &str) -> Result<Vault, String>; pub fn from_env() -> Result<Vault, String>;
    pub fn seal(&self, context: &str, plain: &[u8]) -> Vec<u8>; pub fn open(&self, context: &str, sealed: &[u8]) -> Result<Zeroed, String>; }
pub fn key_context(id: Uuid) -> String;                    // "provider_key:<id>:key"
pub enum KeyKind { Anthropic, OpenAi, OpenRouter, Bedrock, Custom }
pub struct NewKey<'a> { pub name: &'a str, pub kind: KeyKind, pub secret: &'a str, pub base_url: Option<&'a str>,
                        pub model: Option<&'a str>, pub prices: Option<&'a serde_json::Value> }
pub struct KeyRow { pub id: Uuid, pub name: String, pub kind: KeyKind, pub base_url: Option<String>, pub key_last4: String,
                    pub checked_ok: Option<bool>, pub checked_at: Option<DateTime<Utc>>, pub created_at: DateTime<Utc> }
impl Scoped<'_> { pub async fn add_key(&self, v: &Vault, k: NewKey<'_>) -> Result<KeyRow>;   // name clash => Err "a key named …"
                  pub async fn replace_key_secret(&self, v: &Vault, id: Uuid, secret: &str) -> Result<bool>;
                  pub async fn list_keys(&self) -> Result<Vec<KeyRow>>; pub async fn delete_key(&self, id: Uuid) -> Result<bool>;
                  pub async fn test_key(&self, v: &Vault, http: &reqwest::Client, id: Uuid) -> Result<KeyRow>; }
pub fn reachable(url: &Url, policy: &Policy) -> Result<(), String>;   // private allowed only when policy.private_model_urls
// routes: GET/POST /api/models/keys, POST /api/models/keys/{id}/secret, POST /api/models/keys/{id}/test, DELETE /api/models/keys/{id}
```
- `add_key` takes `pg_advisory_xact_lock(hashtext('provider_keys:name:' || $1))`, then checks the name, in one
  transaction. The id is generated in Rust before sealing, so the context names it. Bedrock's secret is JSON
  `{access_key_id, secret_access_key, session_token?}` and `base_url` holds the region. `custom:<name>` takes over
  from `SHAKEDOWN_LOCAL_MODELS`, which is still read. `audit` on add, replace and delete.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_sealed_key_opens_only_for_its_row() { /* context of another id => Err; sealed bytes don't contain the secret */ }
#[tokio::test] async fn keys_are_write_only() { /* POST then GET /api/models/keys: no "secret", no "sealed_key"; key_last4 == last 4 */ }
#[tokio::test] async fn two_keys_cannot_share_a_name() { /* second POST same name => 409, names it */ }
#[tokio::test] async fn your_key_wins_over_the_environment() { /* key => KeySource::Own; deleted => KeySource::Environment */ }
#[tokio::test] async fn test_makes_a_one_token_call_and_records_the_outcome() { /* FakeLlm sees max_tokens 1; bad key => checked_ok false, detail has no key */ }
#[tokio::test] async fn private_urls_are_fine_here_and_refused_when_the_policy_says_so() {
    // LocalLayer: http://127.0.0.1:8000 ok; SpyLayer policy{private_model_urls:false}: http://… refused,
    // https://10.0.0.5 refused, https://169.254.169.254 refused, https://[fc00::1] refused
}
#[tokio::test] async fn a_read_token_cannot_add_keys() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --features harness --test provider_keys` and `cargo test -p shaked vault:: reach::`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "You keep your own model keys and OpenAI-compatible servers in shaked, sealed and write-only, with a Test call, and agents use them before anything from the environment".

### Task A6: Object storage layer

As the old Task 12, with three changes: the key prefix comes from `layer.object_prefix(caller)` (`local` with
`LocalLayer`), `record_save`'s retention is 30 days (D19) unless the caller's layer says otherwise later (no hook in
M1), and `copy`'s repoint runs under `system_tx(SystemJob::StoreCopy)`.

**Files:**
- Create: `core/crates/shaked/src/objects/{mod.rs, url.rs, keys.rs, copy.rs}`, `src/store/saves.rs`.
- Modify: `core/crates/shaked/Cargo.toml` (`object_store = { version = "0.12", default-features = false, features
  = ["aws", "fs"] }`; check for the newest 0.x at task start and pin it), `src/app.rs` (`AppState.objects:
  Option<Arc<ObjectStores>>`), `src/serve.rs` (probe the write store at start when `policy.serves_others`),
  `src/cli.rs` (`store copy`).
- Test: `core/crates/shaked/tests/objects.rs` (memory, file, and `live_minio_*`); unit tests in `url.rs`, `keys.rs`.

**Interfaces:**
- Consumes: A3 `saves` table (`store`, `object_key`, `sha256`, `bytes`, `state`, `expires_at`); A4 `object_prefix`.
- Produces: the old Task 12 interfaces, with `keys::save(prefix: &str, integration: &str, save_id: &str) ->
  ObjectKey` (`<prefix>/saves/<integration>/<save_id>/`), `keys::transcript(prefix, run, branch, agent)`,
  `keys::export(prefix, id)`; `ObjectKey` refuses `..`, a leading `/` and an empty prefix.

- [ ] **Step 1: Write the failing tests:** the old Task 12 list (`urls_parse_to_the_right_backends`,
  `put_get_roundtrip_with_checksum_on_memory_and_file`,
  `reads_come_from_the_store_a_row_names_after_the_write_store_changes`, `copy_verifies_and_repoints`,
  `copy_reruns_idempotently_and_never_repoints_a_bad_object`, `a_start_that_serves_others_probes_the_write_store`,
  `#[ignore] live_minio_s3_put_get_copy`), with `keys_stay_under_their_prefix`
  (`save("p", …)` starts with `"p/saves/"`; `"../"` and absolute segments are `Err`).
- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test objects`
- [ ] **Step 3: Implement.** Nothing moves in M1: saves stay on the parts' disk; M2's worker is the first writer.
- [ ] **Step 4: Run the gate**, plus `cargo test -p shaked --test objects -- --ignored live_minio` once.
- [ ] **Step 5: Commit:** "Saves, transcripts and exports have a storage layer over R2, S3, files or memory, switchable by name, with a verified shaked store copy".

### Task A7: UI: API tokens, Models, the account menu; docs

**Files:**
- Create: `ui/src/api/{me.ts, tokens.ts, models.ts}`, `ui/src/features/settings/{SettingsPage.tsx, TokensPage.tsx,
  ModelsPage.tsx, settings.test.tsx}`, `ui/src/app/AccountMenu.tsx`, `ui/src/app/AccountMenu.test.tsx`.
- Modify: `ui/src/App.tsx` (routes `/settings/tokens`, `/settings/models`), `ui/src/app/TopBar.tsx` (Settings entry;
  `AccountMenu` when `/api/me` has `account`), `ui/src/app/palette.ts`.
- Modify docs: `README.md` "Old names" (`SHAKEDOWN_PORT` → `SHAKEDOWN_BIND`; `SHAKEDOWN_AGENT_BASE` →
  `SHAKEDOWN_PUBLIC_URL`; `SHAKEDOWN_LOCAL_MODELS` → custom model servers in Settings → Models, still read; tables
  in `public` → schema `shakedown`, moved on first start), `docs/reference.md` (`/api/me`, `/api/tokens`,
  `/api/models/keys`, `shaked token create`, `shaked store copy`, `SHAKEDOWN_BIND`, `SHAKEDOWN_PUBLIC_URL`,
  `SHAKEDOWN_ALLOWED_HOSTS`, `SHAKEDOWN_OBJECT_STORE*`, `SHAKEDOWN_VAULT_KEY*`, `SHAKEDOWN_DB`), `docs/sdk.md`
  (`token=`), `docs/agents.md` (each kind's token), and a short `docs/embedding.md` (the `Layer` hooks, what
  `LocalLayer` does, `shaked::serve::run`).

**Interfaces:**
- Consumes: `GET /api/me` (A4), `/api/tokens` (A3), `/api/models/keys` (A5).
- Produces: `fetchMe(): Promise<Me>` with `Me = { mode: string; user: { name: string }; features: string[];
  account?: { label: string; items: { label: string; current?: boolean; post?: string; body?: unknown }[] } }`.
  `AccountMenu` renders `account.label` and its items; an item with `post` POSTs `body` there (credentials
  included) and reloads. It knows nothing else: the hosted layer fills it (B4).

- [ ] **Step 1: Write the failing tests**

```tsx
it('shows a new token once, then only its prefix', async () => {});
it('offers read, run and admin scopes and names what each allows', async () => {});
it('adds a custom server with a URL and runs Test', async () => { /* POST body {kind:'custom', base_url}; shows "Works" or the detail */ });
it('never renders a key secret after saving', async () => { /* input cleared; list shows "…last4" */ });
it('shows no account menu when /api/me has no account', async () => {});
it('posts the chosen item of the account menu and reloads', async () => {});
```

- [ ] **Step 2: Run them and see them fail:** `cd ui && npx vitest run --maxWorkers=1 src/features/settings src/app/AccountMenu.test.tsx`
- [ ] **Step 3: Implement** with existing components (`PageHeader`, `PageBody`, ConnectPage-style forms); plain
  copy, simplicity first.
- [ ] **Step 4: Run** `npx tsc -b && npx vitest run --maxWorkers=1`, the `no_orgs` test, and
  `grep -rn "SHAKEDOWN_BIND\|SHAKEDOWN_OBJECT_STORE" docs README.md` to check every documented name exists in code.
- [ ] **Step 5: Commit:** "Settings has API tokens and Models pages, the top bar shows an account menu when the layer gives one, and the docs list the new settings, the old names they replace and the layer hooks".

### Task A8: The final review's deferred engine minors

**Files:** `src/agents/claude.rs`, `src/driver.rs`, `src/proxy/tickets.rs`; tests in `tests/run_tokens.rs`,
`tests/proxy.rs`, `tests/cli.rs`.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn claude_does_not_start_when_its_config_file_cannot_be_written() { /* read-only workdir => agent error names the file; no inline tokenless config */ }
#[test] fn two_agent_ids_with_the_same_env_name_are_refused() { /* "a-b" and "a_b" in one team => Err naming both */ }
#[tokio::test] async fn a_branch_s_tickets_are_forgotten_when_it_ends() { /* Tickets::len() back to 0 */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test run_tokens --test proxy --test cli`
- [ ] **Step 3: Implement** the three fixes.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Claude Code never falls back to a tokenless config, two agents cannot share a token variable, and an ended branch's proxy tickets are forgotten".

---

## Part B: CopperTide-Cloud repo, branch `hosted-shakedown`

Starts after A4 (B8 also needs A5). Every Part B commit is in the worktree below, never in the main checkout.

### Task B1: The worktree, the crates workspace, and `coppertide-schema` with stream `identity`

**Files** (paths relative to the Cloud worktree):
- Create: `crates/Cargo.toml` (virtual workspace: `coppertide-schema`, `shaked-hosted`), `crates/clippy.toml`,
  `crates/coppertide-schema/{Cargo.toml, bootstrap.sql, src/lib.rs, src/streams.rs}`,
  `crates/coppertide-schema/migrations/identity/{0001_baseline.sql, 0002_orgs.sql}` (from a217411's
  `core/crates/coppertide-schema/migrations/identity/`, then edited per `schema-cloud.sql`: `orgs.kind` is
  `personal | customer`, no `orgs_one_local`, `authz.place_team()` added; invites get RLS in B2),
  `crates/coppertide-schema/migrations/cloud/` (empty; Cloud's R2 writes `cloud` 1), `tests/{files.rs, drift.rs,
  support/mod.rs}`.
- `crates/shaked-hosted/` gets only `Cargo.toml` and an empty `src/lib.rs` here, so the workspace builds.

**Interfaces:**
- Consumes: A2 `Files`, `load`, `migrate`, `base()`.
- Produces:
```rust
pub const IDENTITY: Stream = Stream("identity"); pub const CLOUD: Stream = Stream("cloud");
pub const SHAKEDOWN_HOSTED: Stream = Stream("shakedown_hosted");
pub fn all() -> &'static [Migration];          // base() + identity + cloud + shakedown_hosted, sorted
pub const BOOTSTRAP_SQL: &str;                  // roles and schemas, schema-cloud.sql section 0
```

- [ ] **Step 1: Make the worktree and the link:**
  `git -C ~/MLSys/CopperTide/CopperTide-Cloud fetch origin && git -C ~/MLSys/CopperTide/CopperTide-Cloud worktree add
  .worktrees/hosted-shakedown -b hosted-shakedown origin/main`, then `ln -s
  ~/MLSys/CopperTide/Shakedown/.worktrees/factory ~/MLSys/CopperTide/CopperTide-Cloud/.worktrees/shakedown`.
  Check: `ls .worktrees/shakedown/core/crates/shakedown-schema/Cargo.toml` from the Cloud checkout.
- [ ] **Step 2: Write the failing tests**

```rust
// tests/files.rs: every cloud file parses, versions dense from 1, requires resolve within all(), contract markers
#[test] fn every_cloud_file_parses_and_names_a_known_stream() {}
// tests/drift.rs (moved from shakedown-schema tests/migrate.rs, reading Cloud's own ../../migrations/*.sql at test time)
#[tokio::test] async fn identity_1_create_path_matches_cloud_0015_moved_path() {}
#[tokio::test] async fn a_missing_setting_matches_no_row() { /* SET ROLE shaked_app, no GUCs: count(*) FROM identity.orgs == 0 */ }
```

- [ ] **Step 3: Run them and see them fail:** `cd crates && cargo test -p coppertide-schema`
- [ ] **Step 4: Implement** the interfaces above. `support::scratch_db` runs `BOOTSTRAP_SQL` (the test login is a
  superuser), drops the database on exit, and honours `SHAKEDOWN_TEST_GUARD`.
- [ ] **Step 5: Run the Part B gate; commit:** "Cloud's coppertide-schema crate holds the identity stream and the roles, on top of Shakedown's migrator and base stream".

### Task B2: Stream `shakedown_hosted` 1, the subset test and the walls

**Files:**
- Create: `crates/coppertide-schema/migrations/shakedown_hosted/0001_orgs.sql` (`-- requires: identity >= 2`,
  `-- requires: shakedown >= 2`; sections 5–8 of `schema-cloud.sql` that touch `shakedown` and `shakedown_hosted`,
  plus the identity write policies and the invites RLS), `tests/subset.rs`, `tests/subset_uniques.txt`,
  `tests/walls.rs`, `tests/rls.rs` (moved from a217411 `core/crates/shaked/tests/rls.rs`, rewritten on raw
  connections under `SET ROLE shaked_app` with `set_config`, since the engine no longer sets GUCs),
  `tests/fixtures/orgs.toml` (moved from a217411 `core/crates/shaked/tests/fixtures/orgs.toml`).

**Interfaces:**
- Consumes: B1 `all()`, `BOOTSTRAP_SQL`.
- Produces: `pub fn as_org(org: Uuid, user: Option<Uuid>, teams: &[Uuid], role: &str, place: Option<Uuid>) -> String`
  in `tests/support` (the five `set_config` calls), reused by B3's `begin` test.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/subset.rs: the five checks of the accounts design, "The subset rule"
#[tokio::test] async fn subset_holds() {
    let a = fingerprint(&scratch(&base()).await).await;               // base only
    let b = fingerprint(&scratch_with_bootstrap(all()).await).await;  // everything
    for x in &a.items { assert!(b.items.contains(x), "the cloud changed or removed {x}"); }
    for t in b.tables.difference(&a.tables) { panic!("cloud stream created table shakedown.{t}"); }
    for c in b.columns_not_in(&a) { assert!(ALLOWED_CLOUD_COLUMNS.contains(&c.name.as_str()), "{c}"); }
}
#[tokio::test] async fn every_base_unique_is_reviewed() { /* each unique constraint/index of A is in subset_uniques.txt */ }
// tests/walls.rs
#[tokio::test] async fn every_shakedown_table_is_walled() {
    // for every relkind r/p in shakedown and shakedown_hosted: org_id NOT NULL, relrowsecurity, relforcerowsecurity,
    // a policy named org_wall
}
#[tokio::test] async fn an_insert_without_an_org_setting_fails() {
    // SET ROLE shaked_app, no settings: INSERT INTO shakedown.runs (id, task_id, team) => error (not-null org_id)
    // with org A's settings: the row's org_id == A; read as org B: 0 rows
}
// tests/rls.rs (moved): rls_refuses_cross_org_reads_and_writes_on_every_table (list compared with pg_class),
// root_visibility_by_team_seat_creator_and_grant, a_grant_shows_a_team_run, viewers_and_billing_cannot_write,
// no_scope_no_rows, a_branch_s_org_must_be_its_run_s (composite FK refuses a branch of A pointing at B's run)
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p coppertide-schema --test subset --test walls --test rls`
- [ ] **Step 3: Implement** the migration.
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Stream shakedown_hosted gives every Shakedown table an org, filled from the transaction, with forced row-level security, and a test proves the cloud schema keeps every base table, column, constraint and index".

### Task B3: `shaked-hosted`: opening the database, `begin`, principals, `can()`, the binary

**Files:**
- Create: `crates/shaked-hosted/{Cargo.toml, src/lib.rs, src/main.rs, src/open.rs, src/begin.rs, src/principal.rs,
  src/can.rs}`; `src/principal.rs` and `src/can.rs` from a217411 `core/crates/shaked/src/auth/{principal,can}.rs`
  (the org matrix and its tests, verbatim where they still fit), `src/open.rs` from a217411
  `core/crates/shaked/src/store/open.rs` (`DbUrls`, the hosted plan now `apply: [shakedown, shakedown_hosted]`,
  `require: [(identity, 2)]`, waiting 5 s × 2 min, `refuse_bypassing_login`) plus the startup wall check.
- `Cargo.toml`: `shaked = { path = "../../../shakedown/core/crates/shaked" }`, `shakedown-schema` and
  `coppertide-schema` by path; dev-dependency `shaked` with feature `harness`.
- Test: `crates/shaked-hosted/tests/{begin.rs, open.rs}`, unit tests in `can.rs`.

**Interfaces:**
- Consumes: A4 `Layer`, `Store::from_pools`, `serve::run`, `ServeOptions`, `SystemJob`; B2 the schema.
- Produces:
```rust
pub struct HostedLayer { /* pools, gateway secret, presets, trial key path */ }
impl HostedLayer { pub async fn open(urls: &DbUrls, cfg: HostedConfig) -> anyhow::Result<(HostedLayer, Store)>; }
pub struct HostedConfig { pub gateway_secret_file: PathBuf, pub quotas_file: Option<PathBuf>, pub trial_key_file: Option<PathBuf> }
pub struct Principal { /* User, ServiceAccount, ApiToken, RunToken as the old auth::principal */ }   // rides in Caller.ext
pub fn can(p: &Principal, a: &OrgAction, t: &OrgTarget<'_>, flags: &OrgFlags) -> bool;           // the matrix, unchanged
// begin: SET LOCAL ROLE shaked_app + the five set_config for TxFor::Caller (from the Principal in ext; a caller
// without one => Err, so the transaction never runs); SET LOCAL ROLE shaked_system for TxFor::System.
```
- Hosted policy: `{ serves_others: true, private_model_urls: false, runs_packages_here: false }`. Until B4,
  `authenticate` refuses everything with 401 "sign in through the gateway or send an API token".
- `main.rs`: read `SHAKEDOWN_PG_URL`, `SHAKEDOWN_SYSTEM_PG_URL`, `SHAKEDOWN_MIGRATE_PG_URL`,
  `SHAKEDOWN_GATEWAY_SECRET_FILE` (≥ 32 bytes), `SHAKED_HOSTED_QUOTAS`, `SHAKED_HOSTED_TRIAL_KEY_FILE`; open; run.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn begin_sets_the_org_settings_and_role() {
    // a Caller with a User principal of org A: inside store.tx, current_setting('app.org_id') == A, current_user == shaked_app
}
#[tokio::test] async fn a_caller_without_a_principal_gets_no_transaction() {}
#[tokio::test] async fn a_hosted_app_login_that_bypasses_row_level_security_is_refused() {}   // moved
#[tokio::test] async fn hosted_start_waits_for_identity_then_gives_up() {}                     // identity 1 only => Behind
#[tokio::test] async fn hosted_start_refuses_a_newest_base_table_without_a_wall() {
    // apply all(); then as superuser ALTER TABLE shakedown.events NO FORCE ROW LEVEL SECURITY; open => Err naming events
}
#[test] fn org_matrix() {} #[test] fn levels() {} #[test] fn admins_cannot_change_owners() {}    // moved unit tests
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "shaked-hosted runs the Shakedown engine with a cloud layer: it migrates the hosted streams, refuses a login that bypasses row-level security, sets each transaction's org and role, and decides by the org matrix".

### Task B4: Who calls: `signing` v2, live membership, org-bound tokens, `/api/me`, audit

**Files:**
- Create: `crates/shaked-hosted/src/{signing.rs, authenticate.rs, identity.rs, audit.rs, me.rs}`: `signing.rs` from
  a217411 `core/crates/shaked/src/auth/signing.rs` (v2, `V2_VECTOR`, the header names), `identity.rs` from
  a217411 `auth/identity.rs` (`principal_of_user`, `orgs_of_user` on the layer's system pool), `audit.rs` from
  a217411 `auth/audit.rs`.
- Test: `crates/shaked-hosted/tests/{auth.rs, tokens.rs}`, moved from a217411 `core/crates/shaked/tests/auth.rs` and
  the hosted cases of `tests/tokens.rs` and `tests/run_tokens.rs`.

**Interfaces:**
- Consumes: A4 `authenticate(req, found)`, `audit`, `me`; B3 `Principal`, `HostedLayer`.
- Produces: `authenticate`: `found` = `ApiToken{id}` → read the row's `org_id`, `user_id`, `service_account_id`,
  `team_id` (system pool), refuse a removed member, a disabled user or service account, a deleted org (401);
  `RunToken` → its org, refuse a deleted org; else a good v2 signature → `principal_of_user` (403 "not a member of
  this org"); else 401. `me`: `{mode: "hosted", features: <org features>, account: {label: <org name>, items:
  <orgs of the user, personal first>, each {label, current, post: SHAKED_HOSTED_SWITCH_ORG_URL, body: {org_id}}}}`.
  `audit`: one `identity.audit_log` row, actor from the principal, never a secret.

- [ ] **Step 1: Write the failing tests** (moved, plus the two marked new)

```rust
#[test] fn v2_vector_verifies_and_any_field_change_fails() {}
#[tokio::test] async fn hosted_signed_member_lists_only_their_orgs_runs() {}
#[tokio::test] async fn a_bad_or_late_signature_is_refused() {}
#[tokio::test] async fn hosted_removed_member_is_refused_on_the_next_request() {}
#[tokio::test] async fn hosted_deleted_org_is_refused() {}
#[tokio::test] async fn removing_a_member_kills_their_tokens() {}
#[tokio::test] async fn creating_and_revoking_a_token_is_audited_without_the_secret() {}
#[tokio::test] async fn a_run_token_of_a_deleted_org_is_refused() {}                      // new (deferred minor)
#[tokio::test] async fn me_lists_the_personal_org_first_and_marks_the_current_one() {}    // new
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test auth --test tokens`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Hosted shaked takes requests the gateway signed for a user and an org, or a token whose holder is still a member, and audits token changes".

### Task B5: `authorize`: levels on every route

**Files:**
- Create: `crates/shaked-hosted/src/authorize.rs` (map the engine's `Action`/`Target` to the matrix and to resource
  levels: read the resource's `team_id`, `created_by` and grants in the caller's transaction; `may_write` from
  a217411 `auth/middleware.rs`).
- Test: `crates/shaked-hosted/tests/levels.rs`.

**Interfaces:**
- Consumes: A4 `authorize(c, a, t)`, `harness::ROUTES`; B3 `can`, `level`.
- Produces: `Decision::NotFound` for `Level::None`, `Forbidden` for a lower level; `Target::Kind` with
  `Action::Create` checks the matrix row (`CreateOrgWide` or the team in `app.place_team_id`).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn a_viewer_gets_403_starting_a_run_and_404_for_other_orgs() {}   // moved
#[tokio::test] async fn read_tokens_and_viewers_cannot_write() {}                      // moved (viewer half)
#[tokio::test] async fn a_member_cannot_delete_another_member_s_team_run_without_admin() {}   // the final review's I1 remainder
#[tokio::test] async fn a_team_bound_token_cannot_mint_tokens() { /* 403, not 500 (deferred minor) */ }
#[tokio::test] async fn a_member_cannot_create_an_org_wide_key() {}
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test levels`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Every hosted route checks the caller's level on the resource: viewers read, members run, editors change, admins share and delete".

### Task B6: Org routes: members, teams, seats, invites, grants

**Files:**
- Create: `crates/shaked-hosted/src/routes/{mod.rs, org.rs, teams.rs, invites.rs, grants.rs}` (mounted by
  `Layer::routes`).
- Test: `crates/shaked-hosted/tests/org_routes.rs`.

**Interfaces:**
- Produces: `GET /api/org` (org, members with roles, teams); `PATCH /api/org/members/{user}` `{role}`; `DELETE
  /api/org/members/{user}`; `POST/PATCH/DELETE /api/teams[/{id}]`; `PUT/DELETE /api/teams/{id}/seats/{user}`
  `{role}`; `GET/POST /api/invites`, `DELETE /api/invites/{id}` (POST body `{email, role, team_id?}`, answer `{id,
  url}` once: `<SHAKED_HOSTED_INVITE_BASE>/invite/<token>`, the gateway accepts it in Cloud's R3); `GET/POST
  /api/grants`, `DELETE /api/grants/{id}`; `PATCH /api/resources/{kind}/{id}/team` `{team_id | null}`. Each checks
  `can()` and audits. The last owner cannot be removed or demoted (`FOR UPDATE` on the org's owner rows). A personal
  org refuses invites and teams (D18). An invite needs a free seat (D17; seats = members + open invites ≤ the
  `seats` quota). A team that still scopes resources cannot be deleted (409 naming how many).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn admins_invite_within_seats_and_the_invite_shows_once() {}
#[tokio::test] async fn a_personal_org_takes_no_invites_or_teams() {}
#[tokio::test] async fn the_last_owner_stays() {}
#[tokio::test] async fn admins_cannot_change_owners_through_the_route() {}
#[tokio::test] async fn a_team_with_runs_cannot_be_deleted_until_they_move() {}
#[tokio::test] async fn a_grant_shares_one_run_with_a_team_and_removal_revokes_it() {}
#[tokio::test] async fn maintainers_manage_seats_and_members_do_not() {}
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test org_routes`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Hosted Shakedown manages an org's members, teams, seats, invites and grants, keeps at least one owner, and keeps personal orgs to one person".

### Task B7: Quotas and usage: `admit` and `meter`

**Files:**
- Create: `crates/shaked-hosted/quotas.toml` (from a217411 `docs/quotas.toml`, plus `trial_usd_micros = 30000000`,
  period `none`, hard, in plan `beta`), `src/quotas.rs` and `src/usage.rs` (from a217411
  `core/crates/shaked/src/{quotas.rs, store/usage.rs}`, on `shakedown_hosted.*`), `src/routes/usage.rs` (from
  a217411 `src/api/usage.rs`).
- Test: `crates/shaked-hosted/tests/quotas.rs` (moved from a217411 `core/crates/shaked/tests/quotas.rs`).

**Interfaces:**
- Consumes: A4 `admit(c, Costly)`, `meter(c, Usage)`.
- Produces: `admit`: `StartRun`/`Fork` check `runs` (hard → 429 naming the quota; soft → `Warn`); `ModelCall` checks
  monthly `model_usd_micros` (402); `StoreSave` checks `saves_bytes`. Each check and the following meter of the same
  meter run under `pg_advisory_xact_lock(hashtext('quota:' || org || ':' || meter))`, so concurrent starts cannot both
  pass the last slot (deferred minor). `meter` writes `usage` and upserts `usage_monthly` in one transaction,
  idempotent on the key. `GET /api/usage/org` as before.

- [ ] **Step 1: Write the failing tests:** the moved list (`built_in_presets_parse_and_beta_has_the_s4_numbers`,
  `metering_is_idempotent_and_totals_monthly`, `an_override_row_beats_the_preset`,
  `over_a_hard_runs_quota_a_run_is_refused_naming_the_quota`, `a_soft_quota_warns_and_lets_the_run_start`,
  `every_recorded_model_call_is_metered_once`, `org_usage_is_for_owners_and_billing_and_members_see_their_own`), plus
  `two_concurrent_starts_cannot_both_take_the_last_run` (quota 1, two parallel POSTs → one 2xx, one 429).
- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test quotas`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Hosted orgs are metered once per run and per model call and held to their plan's quotas, without overshooting a hard limit under concurrent starts".

### Task B8: The trial credit and org keys (D23)

**Files:**
- Create: `crates/shaked-hosted/src/trial.rs` (`fallback_provider`), `src/keys.rs` (who may create which keys).
- Test: `crates/shaked-hosted/tests/trial.rs`.

**Interfaces:**
- Consumes: A5 `KeySource`, `Provider`, the engine's model gateway; A4 `fallback_provider`, `admit`, `meter`; B7.
- Produces: `fallback_provider(c, want)`: `None` unless the org has no key for the provider `want` names; then an
  OpenRouter provider on the key in `SHAKED_HOSTED_TRIAL_KEY_FILE`, model `want.model` as asked, `source: "trial"`.
  `admit(ModelCall)` on a trial call: remaining = limit − lifetime `trial_usd_micros`; refuse with 402 "the trial
  credit is used up; add a key in Settings → Models" at ≤ 0. `meter` records the call's cost from OpenRouter's
  reported `usage.cost` (requested with `usage: {include: true}`) as `trial_usd_micros`, not `model_usd_micros`.
  No trial key file: no trial (the fallback is `None`), logged once at start.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn an_org_without_a_key_runs_on_the_trial_with_the_model_it_asks_for() { /* FakeLlm as OpenRouter; request model == asked */ }
#[tokio::test] async fn trial_credit_is_metered_and_runs_out() { /* limit 10 micros, cost 6 => 2nd call 402 with the Settings → Models text */ }
#[tokio::test] async fn an_org_key_is_used_before_the_trial() {}
#[tokio::test] async fn trial_spend_is_its_own_meter() { /* usage rows: trial_usd_micros, no model_usd_micros */ }
#[tokio::test] async fn each_org_has_its_own_trial() { /* org A spent; org B still runs */ }
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test trial`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the Part B gate.**
- [ ] **Step 5: Commit:** "Each hosted org gets a $30 trial on any OpenRouter model, used only without a key of its own, metered apart and refused once spent".

### Task B9: The router isolation test: org B gets 404 for org A's ids on every route

**Files:**
- Create: `crates/shaked-hosted/tests/tenants.rs` (from the old Task 13, on `shaked::harness::ROUTES` plus the
  layer's own routes).
- Modify: whatever leaks; each fix in the layer's `authorize` or, if the engine reads a child without its root, in
  Shakedown (a separate commit there, by path, with its own test).

**Interfaces:**
- Consumes: A4 `harness::ROUTES`, `serve_app`; B4 `sign_v2`; B6 routes.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn the_table_names_every_layer_route() {}
#[tokio::test] async fn org_b_gets_404_for_org_a_ids_on_every_route() {
    // org A's owner makes a run, branch, fork, suite, experiment, review, connection, key, token, world row, save row, team, invite, grant
    for r in all_routes() { let res = call_signed(r.method, &fill(r.path, &a_ids), B_OWNER).await;
        assert!(matches!(res.status().as_u16(), 404 | 405), "{} {} => {}", r.method, r.path, res.status());
        assert!(!res.text().await.contains(&a_ids.run), "{} {} leaks an id", r.method, r.path); }
}
#[tokio::test] async fn org_b_lists_show_none_of_org_a() {}
#[tokio::test] async fn org_b_tokens_cannot_reach_org_a() { /* the same loop with an sd_ token of B and an sdr_ token of B's branch */ }
```

- [ ] **Step 2: Run them and see them fail:** `cd crates && cargo test -p shaked-hosted --test tenants`
- [ ] **Step 3: Fix** each leak.
- [ ] **Step 4: Run both gates** (Part A if Shakedown changed). Then `rm -rf crates/target`.
- [ ] **Step 5: Commit:** "Another org's ids are not found on any hosted route, under a signature, an API token or a run token".

---

## Order, size and what lands dark

| # | Task | Depends on | Size |
|---|---|---|---|
| O1 | convert the retired databases (owner) | — | 1 h |
| A1 | hosted mode out | — | 0.5 d |
| A2 | generic migrator, `shakedown-schema` | A1 | 1 d |
| A3 | personal-use schema and engine | A2, O1 for the gate | 2.5 d |
| A4 | the layer and its hooks | A3 | 2.5 d |
| A5 | your own model keys | A4 | 2 d |
| A6 | object storage | A4 | 2 d |
| A7 | UI and docs | A4, A5 | 2 d |
| A8 | deferred engine minors | A3 | 0.5 d |
| B1 | worktree, `coppertide-schema`, identity | A3 | 1 d |
| B2 | `shakedown_hosted` 1, subset, walls, RLS | B1 | 1.5 d |
| B3 | `shaked-hosted`: open, begin, `can()`, binary | A4, B2 | 1.5 d |
| B4 | authenticate, tokens, `/api/me`, audit | B3 | 1.5 d |
| B5 | authorize, levels | B4 | 1.5 d |
| B6 | org routes | B5 | 2 d |
| B7 | quotas and usage | B4 | 1 d |
| B8 | trial credit, org keys | A5, B7 | 1.5 d |
| B9 | router isolation | B5–B8, A6 | 1.5 d |

About 26 engineer-days. Part A runs in the Shakedown worktree, where another agent also commits; Part B can start
once A4 lands and then runs beside A5–A8 in its own repo. Nothing reaches production: `factory` and
`hosted-shakedown` are unmerged, the open-source binary has no hosted path, and `update.sh` builds neither
`shaked-hosted` nor the Shakedown repo until M3. Cloud's R1–R5 (the gateway adopting `coppertide-schema`, personal
orgs for existing users, invite acceptance, the active-org route) are Cloud's own plan; the hosted binary requires
`identity >= 2`, so it cannot start against production before R3.
