# Hosted Shakedown M1: foundation in `shaked` and object storage — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `shaked` multi-tenant underneath, while local single-user Shakedown keeps working as it does today.
That means one schema crate, orgs on every row with RLS, principals and `can()`, API, run and agent tokens, a
configurable bind with a hosted mode, per-org model keys, quotas and usage, and a storage layer that works with any
object store.

**Architecture:** A new crate `coppertide-schema` holds the SQL migrations for each stream and a migrator that
does not care which Postgres driver calls it. `shaked` applies the `identity` and `shakedown` streams at start
and seeds the local org. It moves from one `Client` to a deadpool pool, and every query runs in
`Store::tx(&Scope)`, which sets the `app.*` GUCs. An auth layer turns each request into a `Principal`: in local
mode the built-in owner, in hosted mode a gateway signature or a token. It then builds the `Scope`. Hosted mode
is off by default and fails closed, so every task lands dark.

**Tech Stack:** Rust (axum 0.8, tokio, tokio-postgres, deadpool-postgres, ring, object_store), Postgres 17,
React 18 + TypeScript + vitest, Python SDK (httpx).

**Spec:** in `~/MLSys/CopperTide/CopperTide-Cloud/docs/superpowers/specs/` (uncommitted there; read the files, not
`origin/main`):
- `2026-11-19-hosted-shakedown-design.md`: the M1 section and Security S1–S6.
- `2026-11-19-accounts-and-schema-design.md` and `2026-11-19-schema.sql`, which is the DDL. These two supersede
  the hosted spec's "Tenancy" paragraph, so where they differ, `tenant` means `org_id`.
- `2026-11-19-sandbox-provider-analysis.md` §6. `SandboxBackend` models template snapshots and belongs to M2.
  Nothing here may assume a live fork.

## Calls made

| Call | Why | Cost if wrong |
|---|---|---|
| The migrator talks to the database through a two-method `Conn` trait. M1 builds only the tokio-postgres impl, and Cloud's plan writes the sqlx impl. The spec said "shaked adds sqlx". | Keeps sqlx's heavy build off the shared machine and out of local `shaked`. | Cloud writes a ~40-line adapter. If the trait doesn't fit sqlx, it changes before Cloud's R2. |
| A migration may list `also_accept` checksums of earlier texts. | `identity` 1 is written here from Cloud's tables. If Cloud's drift test finds a difference before R2, the file can be fixed without bricking local databases that already applied it. | A reviewer has to approve each accepted old checksum. |
| `org_id` defaults to the local org in `shakedown` 2, and the default is dropped in `shakedown` 3 (Task 6). The DDL drops it in the same migration. | Code that doesn't pass `org_id` yet stays green between Tasks 5 and 6. Dropping a default is a contract change anyway. | One extra migration. |
| `reviews` keeps `PRIMARY KEY (key)` and gains `UNIQUE (org_id, key)`. The DDL swaps the primary key. | Swapping a PK is a contract change, and review keys already embed globally unique run and branch ids. | None functionally. The text differs from the DDL; noted in the migration. |
| `saves` and `worlds` get `store text` next to `object_key`. | Owner: R2 and S3 must switch instantly. A row names a store, never a URL. | One column. |
| Local `shaked` needs a superuser login (as with the compose Postgres today), or `bootstrap.sql` already run. Without either it refuses to start, naming that file. | RLS policies name the roles `shaked_app` and friends, so the roles must exist before the migrations run. | Someone with a non-superuser local Postgres runs one SQL file once. |
| Locally there is one login, and each transaction does `SET LOCAL ROLE shaked_app` (or `shaked_system`). Hosted uses two logins: `SHAKEDOWN_PG_URL` in `shaked_app` and `SHAKEDOWN_SYSTEM_PG_URL` in `shaked_system`. Migrations run over `SHAKEDOWN_MIGRATE_PG_URL`. | Tests and local runs then go through RLS exactly as hosted does. | None. |
| Tokens carry a prefix for each kind: `sd_` API, `sdr_` run, `sda_` agent, `sdw_` worker, `sdt_` ticket, each followed by 64 hex characters. Agent tokens are checked against an in-memory map of hashes; the database keeps only the hash. | Routing and secret scanning by prefix; no database round trip on every MCP or model call. | Changing a prefix later breaks issued tokens. They are short-lived, except API tokens. |
| Claude Code gets its token in a 0600 `--mcp-config` file, not in argv. Codex gets it through `bearer_token_env_var` pointing at `SHAKEDOWN_AGENT_TOKEN`. | argv is visible to other local users in `ps`. | One temp file per agent, deleted when the agent ends. |
| `shaked` verifies `signing` v2 itself (`auth/signing.rs`, ring HMAC) against a fixed test vector that the CopperTide `signing` crate must also pass. The header names live in one const block. | Local `shaked` must build without another repo. The phase-3 rename may rename `x-sculptured-*`. | Two implementations could drift. The shared vector catches that. |
| Model keys are sealed with AAD `provider_key:<org_id>:<name>` (Cloud `vault.rs` format v1, key from `SHAKEDOWN_VAULT_KEY[_FILE]`). Names are immutable. | Owner: bound to the org and the key name. | Renaming a key means re-sealing it. |
| Quota presets live in `docs/quotas.toml` (built in, overridable with `SHAKEDOWN_QUOTAS`). Plan `beta` holds the S4 numbers; plan `local` caps branches at 24 h and nothing else. Numbers the owner hasn't set: trial credit `0` (trial off), monthly model spend and seats unlimited in `shaked`. | No hard-coded domain data. D9 gives only the S4 numbers. | The owner edits one file. |
| A new meter, `trial_usd_micros`, is added to the `usage` and `quotas` checks in `shakedown` 2. | D8: a free trial on our keys has to be metered apart from the org's own spend. | One CHECK value. |
| Object storage: nothing moves in M1. Saves stay on the parts' disk (M2's worker uploads them), transcripts start in M2/M6, and exports stay client-side. M1 ships the layer, the key layout, `saves` rows, `shaked store copy` and a startup probe in hosted mode. | Saves are made by parts, not by `shaked`. Moving them needs the worker. | M2 is the first real writer. The live MinIO test covers the S3 path until then. |
| Hosted M1 refuses world and benchmark changes (`/api/worlds/*/use`, builds, prepare) and refuses to launch parts and Code connections. The current world per org (`org_settings.current_world_id`) arrives with M2. | `AppState.current` is global, and hosted runs need sandboxes anyway. | Hosted M1 can't run anything, and that's intended. |
| The crate lives at `core/crates/coppertide-schema`, a member of the `core` workspace. | D16 (recommended). | Moving it later is a path change in two repos. |

## Owner decisions this plan assumes (recommended option; the tasks marked ⚑ change if the answer differs)

- **D15** (the database on the gateway box now): ⚑ Task 2 (pool URLs and the migrate URL), ⚑ Task 7 (identity read live
  from the same database).
- **D16** (the crate in the Shakedown repo): ⚑ Task 1.
- **D17** (customer admins invite people with no account, within the seat quota): ⚑ Task 10 (the `seats` meter is
  only a hook in `shaked`; the gateway enforces it).
- **D18** (personal orgs are single-member): ⚑ Task 4 (a personal org's owner is an ordinary owner), ⚑ Task 14 (the
  switcher lists the personal org first).
- **D19** (retention 30 days, audit 13 months): ⚑ Task 5 (`org_settings.retention_days` defaults to 30), ⚑ Task 12
  (`saves.expires_at`). There is no audit-retention code in M1.
- **D20** (opt-in copy of model keys): ⚑ Task 11 (the seal format matches Cloud's vault, so a later copy is just a
  re-seal; nothing else to build).
- **D21** (guest membership later): ⚑ Task 4 (grants only within one org; no guest principal).

## Global Constraints

- Paths as of `factory` at 52ecaac. The phase-3 rename (headers, MCP name, crate names) is still running: at the
  start of each task, run `git -C .worktrees/factory ls-files core/crates | cut -d/ -f3 | sort -u` and use the
  current crate directories.
- Gate for each commit: in `core/`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace --no-fail-fast`. Postgres for tests is container `deploy-pg-1` on 127.0.0.1:5544
  (`docker start deploy-pg-1`). UI: `npx tsc -b && npx vitest run --maxWorkers=1`. SDK: `uv run --all-extras pytest -q` in
  `sdk/python`.
- Shared machine: `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, one cargo build at a time, output to a file
  and read its tail. Never start `shaked` while tests run.
- clippy `too_many_lines` = 30, enforced by `-D warnings`. Never `#[allow]` it.
- New dependencies, ruled: `deadpool-postgres` (spec), `object_store` with features `aws` + `fs` (owner: any store),
  `sha2` in the crate. Nothing else without a ledger line in this plan.
- Local mode behaves as it does today after every task, except that agents carry tokens (from Task 9 on). Hosted
  mode is `shaked --hosted` or `SHAKEDOWN_MODE=hosted`. It is off by default and fails closed.
- Migrations are append-only. `DROP`, `RENAME`, `SET NOT NULL` or `TYPE` needs `-- contract: unused since <sha>`.
  Every migration sets `lock_timeout = '5s'` and `statement_timeout = '60s'`.
- Tokens and sealed values never reach events, logs, error messages or exports. Lists show `token_prefix` and
  `key_last4` only.
- Tests: never a real model; Docker-only tests are `#[ignore]` and named `live_*`; every database, container and
  folder a test makes is dropped, even when it fails. Test databases connect through `SET ROLE shaked_app`.
- No hard-coded domain data: quota presets, fixtures and org names go in data files.
- Commits: plain sentences in the repo's style. `git add <paths>` only (another agent commits in this worktree).
  End with the attribution lines from the session. No push, no merge to `main`.

## Review Focus

1. **Old local databases.** Records in `public`, in a database named `arena` or `shakedown`, made by any earlier
   `shaked`, migrate in place on the first start. Every run still lists, loads and forks, and a second start
   changes nothing. Test: Task 2, `an_old_public_database_moves_to_shakedown_and_keeps_its_runs`.
2. **A branch paused longer than its token's lifetime.** On resume, its agents' tokens and proxy tickets still
   work, because expiry counts only unpaused time. Test: Task 9, `a_long_pause_does_not_expire_agent_tokens`.
3. **`custom` model servers on localhost.** Allowed in local mode (a vLLM on 127.0.0.1 is the normal case).
   Refused in hosted mode for private, link-local and metadata addresses, checked at Test time and on each call.
   Test: Task 11, `custom_urls_local_ok_hosted_private_refused`.
4. **A `shaked store copy` interrupted and run again.** It skips objects already verified, and it never repoints a
   row whose object failed verification. Test: Task 12, `copy_reruns_idempotently_and_never_repoints_a_bad_object`.
5. **A token in a recorded run.** No `sd_`, `sdr_` or `sda_` string appears in events, in a Code connection's
   recorded stderr, or in the agent CLI's argv. Test: Task 9, `no_token_reaches_events_or_argv`.

---

### Task 1: Crate `coppertide-schema`: migrator, `identity` 1–2, `shakedown` 1 (⚑ D16)

**Files:**
- Create: `core/crates/coppertide-schema/{Cargo.toml, src/lib.rs, src/migrator.rs, src/stream.rs, src/pg.rs}`
- Create: `core/crates/coppertide-schema/bootstrap.sql`: section 0 of the DDL (four NOLOGIN roles, `CREATE SCHEMA`
  with AUTHORIZATION).
- Create: `core/crates/coppertide-schema/migrations/identity/0001_baseline.sql`: for each of `users`,
  `identities`, `invites`, `sessions`, `login_flows`, `limits` and `admin_audit`, `ALTER TABLE public.x SET SCHEMA
  identity` when `to_regclass('public.x')` exists, else `CREATE TABLE identity.x` exactly as Cloud's 0001–0015
  leave it.
- Create: `.../migrations/identity/0002_orgs.sql`: `authz.*` functions, `orgs`, `memberships`, `teams`,
  `team_members`, `service_accounts`, `audit_log`, the `invites` org columns, `sessions.active_org_id`, identity
  grants and RLS (DDL §2, §3, §6, §7 identity part).
- Create: `.../migrations/shakedown/0001_baseline.sql`: `CREATE SCHEMA IF NOT EXISTS shakedown`; move `public.{runs,
  branches, events, suites, suite_runs, experiments, reviews, connections}` when present; `SET LOCAL search_path =
  shakedown`; then the `SCHEMA` strings of `store.rs` and `store/{suites,experiments,reviews,connections}.rs`
  copied verbatim, in that order.
- Create: `core/crates/coppertide-schema/tests/{migrate.rs, files.rs}`, and `tests/fixtures/cloud-0015/*.sql`
  (copied from CopperTide-Cloud `origin/main:migrations/0001–0015`).
- Modify: `core/Cargo.toml` (members), `Cargo.lock`.

**Interfaces:**
- Produces:
```rust
pub enum Stream { Identity, Cloud, Shakedown }               // as_str(): "identity" | "cloud" | "shakedown"
pub struct Migration { pub stream: Stream, pub version: u32, pub name: &'static str, pub sql: &'static str,
    pub checksum: [u8; 32], pub requires: Vec<(Stream, u32)>, pub no_transaction: bool,
    pub also_accept: &'static [[u8; 32]] }
pub fn migrations() -> &'static [Migration];                  // LazyLock over include_str!, sorted (stream, version)
pub struct Applied { pub stream: Stream, pub version: u32, pub checksum: Vec<u8> }
pub trait Conn: Send {                                        // native async fn in trait; no async-trait
    async fn batch(&mut self, sql: &str) -> Result<(), Error>;          // simple-query protocol
    async fn applied(&mut self) -> Result<Vec<Applied>, Error>;         // empty when schema_meta is missing
}
pub struct Plan<'a> { pub apply: &'a [Stream], pub require: &'a [(Stream, u32)], pub applied_by: &'a str }
pub struct Report { pub applied: Vec<(Stream, u32)>, pub newer: Vec<(Stream, u32)> }   // newer: logged, fine
pub async fn migrate(c: &mut impl Conn, plan: &Plan<'_>) -> Result<Report, Error>;
pub async fn check(c: &mut impl Conn, plan: &Plan<'_>) -> Result<Report, Error>;      // never writes
pub async fn wait_ready(c: &mut impl Conn, plan: &Plan<'_>, every: Duration, give_up: Duration) -> Result<Report, Error>;
pub enum Error { Checksum { stream: Stream, version: u32, name: String }, Behind { stream: Stream, have: u32, need: u32 },
    BadFile(String), Sql(String) }
pub const BOOTSTRAP_SQL: &str;
#[cfg(feature = "tokio-postgres")] impl Conn for tokio_postgres::Client {}
```
- Migrator algorithm. Run `CREATE SCHEMA IF NOT EXISTS schema_meta` and the `schema_meta.migrations` DDL with `IF NOT
  EXISTS`. For each owned stream: `SELECT pg_advisory_lock(hashtext('coppertide-schema:' || stream))`, read
  `applied()`, and fail with `Checksum` if a stored checksum is neither the embedded one nor in `also_accept`.
  Apply each missing version in order. Normal files run as `BEGIN; SET LOCAL lock_timeout='5s'; SET LOCAL
  statement_timeout='60s'; <sql>; INSERT INTO schema_meta.migrations …; COMMIT`. Files marked
  `-- no-transaction` run without the BEGIN/COMMIT, and the INSERT follows. Then `pg_advisory_unlock`. Header
  lines: `-- requires: <stream> >= <n>`, `-- no-transaction`, `-- contract: unused since <sha>`.
  `applied_by` must match `^[a-z0-9 ._-]{1,80}$` (it is formatted into SQL).

- [ ] **Step 1: Write the failing tests** (each on its own scratch database, dropped on exit)

```rust
// tests/files.rs: no database
#[test] fn every_file_parses_and_versions_are_dense_from_1() { /* per stream: 1..=n, names match NNNN_name.sql */ }
#[test] fn every_requires_names_an_existing_migration() {}
#[test] fn destructive_statements_carry_a_contract_marker() {
    // DROP / RENAME / SET NOT NULL / TYPE outside comments => the file has "-- contract: unused since "
}
#[test] fn every_file_sets_no_timeouts_itself() {} // the migrator sets them; a file's SET would override
// tests/migrate.rs
#[test] fn empty_to_latest_then_again_is_a_no_op() { /* 2nd migrate(): report.applied.is_empty() */ }
#[test] fn a_changed_checksum_refuses_and_also_accept_admits() { /* UPDATE schema_meta... => Error::Checksum */ }
#[test] fn a_missing_required_stream_reports_behind_and_wait_ready_gives_up() {
    // Plan{apply:[Shakedown], require:[(Identity,2)]} on a db with identity 1 => Error::Behind{have:1,need:2}
    // wait_ready(every=10ms, give_up=50ms) => Err(Behind)
}
#[test] fn two_concurrent_migrators_apply_each_version_once() { /* join two tasks; count(*) per version == 1 */ }
#[test] fn newer_versions_than_the_binary_knows_are_reported_not_refused() {}
#[test] fn identity_1_create_path_matches_cloud_0015_moved_path() {
    // db A: apply fixtures/cloud-0015 into public, then migrate identity; db B: migrate identity fresh.
    // assert_eq!(fingerprint(A, "identity"), fingerprint(B, "identity"))
    // fingerprint = sorted rows of columns (name, type, nullable, default), pg_get_constraintdef, pg_get_indexdef
}
#[test] fn shakedown_1_is_idempotent_over_tables_made_by_todays_shaked() {
    // apply today's SCHEMA strings into public; migrate; tables now in schema shakedown with their rows
}
#[test] fn a_missing_setting_matches_no_row() { /* under SET ROLE shaked_app, no GUCs: SELECT count(*) FROM identity.orgs == 0 */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p coppertide-schema` (it doesn't compile: the crate is missing).
- [ ] **Step 3: Implement** the interfaces above. `pg.rs` is the tokio-postgres `Conn` behind feature
  `tokio-postgres`, which is on by default for `shaked`'s use. Tests that need roles run `BOOTSTRAP_SQL` first
  (the test login is a superuser).
- [ ] **Step 4: Run them and see them pass:** `cargo test -p coppertide-schema`, then the gate.
- [ ] **Step 5: Commit:** "The coppertide-schema crate applies the identity and shakedown migration streams with checksums, advisory locks and required versions".

### Task 2: `shaked` on the migrator, with a pool, the local seed and template test databases (⚑ D15)

**Files:**
- Modify: `core/crates/shaked/Cargo.toml` (`coppertide-schema`, `deadpool-postgres`).
- Modify: `core/crates/shaked/src/store.rs` and `store/*.rs`: drop the `SCHEMA` consts and `migrate()`. `Store`
  holds `app: Pool`, `system: Pool`, `set_role: bool`, and each method does `self.app.get().await?`.
- Create: `core/crates/shaked/src/store/open.rs` (connect, roles check, migrate, seed) and `src/store/seed.rs`.
- Modify: `core/crates/shaked/src/main.rs` (the interrupt step moves to `jobs::interrupt_at_start`).
- Create: `core/crates/shaked/src/jobs.rs`.
- Modify: `core/crates/shaked/tests/support/pg.rs`: `fresh_db(tag)` = `CREATE DATABASE … TEMPLATE
  shakedown_tpl_<first 8 hex of the combined checksum>`, made once per binary under an advisory lock.
- Test: `core/crates/shaked/tests/schema.rs`.

**Interfaces:**
- Consumes: Task 1 `migrate`, `Plan`, `BOOTSTRAP_SQL`.
- Produces:
```rust
pub const LOCAL_USER_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000a");
pub const LOCAL_ORG_ID:  Uuid = uuid!("00000000-0000-0000-0000-00000000000b");
pub struct DbUrls { pub app: String, pub system: Option<String>, pub migrate: Option<String> }  // hosted: all three
impl Store {
    pub async fn open(admin: &Admin) -> Result<Store>;                 // unchanged signature (local)
    pub async fn open_named(admin: &Admin, name: &str) -> Result<Store>; // unchanged signature (local)
    pub async fn open_hosted(urls: &DbUrls) -> Result<Store>;          // Task 3 calls it; no seed, require identity >= 2
}
pub(crate) async fn seed_local(c: &tokio_postgres::Client) -> Result<()>;   // the 3 INSERTs at the DDL's end, idempotent
```
- Pool connections get `options = "-c search_path=shakedown,identity,authz,public"`. The local order is: check
  `rolsuper` (else check that the role `shaked_app` exists, else stop with "run core/crates/coppertide-schema/
  bootstrap.sql once"); run `BOOTSTRAP_SQL` when the login is a superuser; migrate `[Identity]`; `seed_local`;
  migrate `[Shakedown]`. `set_role = rolsuper`. `applied_by = "shaked <git sha or version>"`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn an_old_public_database_moves_to_shakedown_and_keeps_its_runs() {
    // support::legacy makes a db with today's public tables and a recorded run; Store::open_named;
    // list_runs() has it; get_branch(...).events == before; open_named again: schema_meta rows unchanged
}
#[test] fn a_fresh_database_has_the_local_owner() {
    // identity.memberships (LOCAL_ORG_ID, LOCAL_USER_ID) role == "owner"; orgs.kind == "local"; plan == "local"
}
#[test] fn running_branches_are_interrupted_at_start() { /* as today, now through jobs::interrupt_at_start */ }
#[test] fn a_non_superuser_without_roles_is_refused_naming_bootstrap_sql() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test schema`
- [ ] **Step 3: Implement.** Store methods keep their signatures and only swap `self.client` for a pooled
  connection. Switch every test helper that makes a store to `fresh_db`.
- [ ] **Step 4: Run the gate.** All existing tests pass unchanged.
- [ ] **Step 5: Commit:** "shaked keeps its records in the shakedown schema through coppertide-schema, on a connection pool, with the built-in local owner".

### Task 3: Bind, public URL, allowed hosts and modes

**Files:**
- Create: `core/crates/shaked/src/mode.rs`.
- Modify: `src/main.rs` (bind from settings; `--hosted`); `src/app.rs` (`Config` gains `mode`, `public_url`,
  `allowed_hosts`; `loopback_host_only` becomes `allowed_host`; `install_default_drivers` and `agent_base` derive
  from `public_url`); `src/external.rs` (`endpoints` takes the public URL); `src/driver.rs` and the part launch in
  `src/world/*` (refused in hosted mode); `src/api/worlds.rs` (use, build and prepare refused in hosted mode).
- Test: `core/crates/shaked/tests/modes.rs`, plus unit tests in `mode.rs`.

**Interfaces:**
- Produces:
```rust
pub enum Mode { Local, Hosted }
pub struct Bind { pub addr: SocketAddr, pub public_url: Url, pub allowed_hosts: Vec<String> }
pub fn bind_from(get: impl Fn(&str) -> Option<String>, hosted: bool) -> Result<Bind, String>;
// SHAKEDOWN_BIND (default 127.0.0.1:4510; old SHAKEDOWN_PORT still read as 127.0.0.1:<port>),
// SHAKEDOWN_PUBLIC_URL (default http://<bind>), SHAKEDOWN_ALLOWED_HOSTS (comma list; default loopback names +
// the public URL's host). Non-loopback bind without hosted => Err("… needs --hosted").
pub fn host_allowed(host: &str, allowed: &[String]) -> bool;   // replaces is_loopback_host, same parsing rules
pub const HOSTED_REFUSAL: &str = "runs on the control box only in a sandbox (not yet available)";
```
- The hosted `/api` layer answers 401 `{"error":"sign in through the gateway or send an API token"}` to every
  request until Task 7. It fails closed.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn defaults_are_todays() { /* addr 127.0.0.1:4510, public http://127.0.0.1:4510, loopback hosts */ }
#[test] fn old_port_setting_still_binds() { /* SHAKEDOWN_PORT=4600 => 127.0.0.1:4600 */ }
#[test] fn a_public_bind_needs_hosted() { /* SHAKEDOWN_BIND=0.0.0.0:4510, hosted=false => Err contains "--hosted" */ }
#[test] fn agent_urls_derive_from_the_public_url() {
    // PUBLIC_URL=https://sd.example => endpoints(..).mcp_url starts "https://sd.example/mcp/"; same for gateway, http agent base
}
#[test] fn allowed_hosts_keep_the_dns_rebinding_guard() { /* every case of today's loopback_hosts test, plus listed host */ }
#[test] fn hosted_refuses_every_api_call_and_part_launch() {
    // AppFx hosted: GET /api/runs => 401; driver::launch => Err(HOSTED_REFUSAL); POST /api/worlds/x/use => 403
}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test modes` and `cargo test -p shaked mode::`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "shaked binds where SHAKEDOWN_BIND says, hands out SHAKEDOWN_PUBLIC_URL, and a hosted shaked refuses everything until it can tell who is calling".

### Task 4: Principals, levels and `can` (⚑ D18, D21)

**Files:**
- Create: `core/crates/shaked/src/auth/{mod.rs, principal.rs, can.rs}`.
- Test: unit tests in `can.rs`.

**Interfaces:**
- Produces:
```rust
pub enum OrgRole { Owner, Admin, Member, Viewer, Billing }
pub enum SeatRole { Maintainer, Member }
pub enum Level { None, View, Run, Edit, Admin }                      // Ord
pub enum TokenScope { Read, Run, Admin }
pub enum RunTokenKind { Run, Agent, Worker, Ticket }
pub enum Principal {
    User { user: Uuid, org: Uuid, role: OrgRole, seats: Vec<(Uuid, SeatRole)> },
    ServiceAccount { sa: Uuid, org: Uuid, role: OrgRole, team: Option<Uuid> },
    ApiToken { token: Uuid, holder: Box<Principal>, scopes: Vec<TokenScope>, team: Option<Uuid> },
    RunToken { org: Uuid, run: String, branch: String, kind: RunTokenKind, agent: Option<String>, sandbox: Option<Uuid> },
}
pub enum Action { // resource actions (spec §3 table) …
    Read, Start, Fork, Stop, Record, UseInRun, Rename, ChangeSetup, ReplaceSecret, DeleteFork, Share, MoveTeam, Delete,
    // … run-internal (run tokens only) …
    RegisterAgent, Report, Finish, CallTools, CallModel, WorkerLink,
    // … org-level (matrix rows)
    SeeOrg, CreateOrgWide(ResourceKind), CreateTeam, ManageSeats(Uuid), Invite, ChangeRole { target: OrgRole },
    ServiceAccounts, AnyTokens, OwnTokens, Audit, Usage, Billing, OrgSettings, DeleteOrg }
pub enum ResourceKind { Run, Experiment, Suite, World, ProviderKey, Connection }
pub struct Resource { pub kind: ResourceKind, pub org: Uuid, pub id: String, pub team: Option<Uuid>,
    pub created_by: Option<Uuid>, pub grants: Vec<Grant> }
pub struct Grant { pub to: Grantee, pub level: Level }               // Grantee::User(Uuid) | Grantee::Team(Uuid)
pub enum Target<'a> { Org(Uuid), Resource(&'a Resource), Branch { org: Uuid, run: &'a str, branch: &'a str } }
pub struct OrgFlags { pub members_create_teams: bool }
pub fn level(p: &Principal, r: &Resource) -> Level;
pub fn required(a: &Action) -> Level;
pub fn can(p: &Principal, a: &Action, t: &Target<'_>, flags: &OrgFlags) -> bool;
```
- The rules are the accounts spec §3, verbatim. The level is the max of: admin role, org-wide member `run`, team
  seat `edit` (maintainer `admin`), creator `admin`, and grants. Then caps apply: viewer → view, billing → none,
  token scopes (`read` → view, `run` → edit, `admin` → uncapped), team-bound → its team plus org-wide only. A
  principal of another org gets `None`. Run tokens allow only run-internal actions on their own branch, and an
  `Agent` token only `CallTools` and `CallModel` for its agent.

- [ ] **Step 1: Write the failing tests.** These are table-driven: one row for each cell of the org matrix and for
  each row of the resource-action table.

```rust
#[test] fn org_matrix() { for (role, action, want) in MATRIX { assert_eq!(can(&user(role), &action, &Target::Org(A), &F), want, "{role:?} {action:?}") } }
#[test] fn admins_cannot_change_owners() { assert!(!can(&user(Admin), &Action::ChangeRole{target: Owner}, &Target::Org(A), &F)) }
#[test] fn members_create_teams_only_when_the_org_allows() {}
#[test] fn levels() {
    assert_eq!(level(&user(Member), &org_wide_run()), Level::Run);
    assert_eq!(level(&seat(Maintainer), &team_run()), Level::Admin);
    assert_eq!(level(&user(Viewer), &granted(Level::Admin)), Level::View);       // cap
    assert_eq!(level(&user(Billing), &org_wide_run()), Level::None);
    assert_eq!(level(&user_of(B), &org_wide_run()), Level::None);                 // other org
    assert_eq!(level(&api_token(&[Read], user(Owner)), &org_wide_run()), Level::View);
    assert_eq!(level(&team_bound_sa(T2), &team_run_of(T1)), Level::None);
}
#[test] fn run_tokens_reach_only_their_branch_and_agent() {
    assert!(can(&agent_tok("b1","a"), &Action::CallTools, &branch(A,"r","b1"), &F));
    assert!(!can(&agent_tok("b1","a"), &Action::CallTools, &branch(A,"r","b2"), &F));
    assert!(!can(&agent_tok("b1","a"), &Action::Read, &Target::Resource(&org_wide_run()), &F));
}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked auth::can`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run them and see them pass,** then the gate.
- [ ] **Step 5: Commit:** "shaked decides who may do what with one can(principal, action, target), by org role, team seat, authorship, grant and token scope".

### Task 5: `shakedown` 2: orgs on every row, the new tables and RLS; `Store::tx` and `SystemScope` (⚑ D19)

**Files:**
- Create: `core/crates/coppertide-schema/migrations/shakedown/0002_orgs.sql` (`-- requires: identity >= 2`).
  Existing tables get `org_id uuid NOT NULL DEFAULT '<LOCAL_ORG_ID>'`, `team_id uuid`, `created_by uuid` (root
  tables), the `(org_id, id)` uniques and the DDL's FKs as `NOT VALID`. `reviews` gets `UNIQUE (org_id, key)` and
  keeps its PK. The new tables (`provider_keys`, `worlds`, `org_settings`, `saves`, `sandboxes`, `api_tokens`,
  `run_tokens`, `grants`, `quotas`, `usage`, `usage_monthly`) come from the DDL. `saves` and `worlds` also get
  `store text NOT NULL`. The meter checks gain `trial_usd_micros`. Privileges and RLS are DDL §6–§7, shakedown part.
- Create: `core/crates/shaked/src/auth/scope.rs`, `src/store/tx.rs`.
- Test: `core/crates/shaked/tests/rls.rs`; `core/crates/coppertide-schema/tests/migrate.rs` (+1 test).

**Interfaces:**
- Consumes: Task 4 `Principal`, `OrgRole`.
- Produces:
```rust
pub struct Scope { /* private: org, user: Option<Uuid>, role: OrgRole, teams: Vec<Uuid>, principal: Principal */ }
impl Scope {
    pub(in crate::auth) fn of(p: Principal) -> Scope;   // team-bound => teams=[team], user=None, role=Member
    pub fn org(&self) -> Uuid; pub fn user(&self) -> Option<Uuid>; pub fn principal(&self) -> &Principal;
}
pub fn local_owner() -> Scope;                           // in auth; Principal::User{LOCAL_USER_ID, LOCAL_ORG_ID, Owner, []}
#[cfg(any(test, feature = "testkit"))] pub fn test_scope(p: Principal) -> Scope;  // tests only
pub struct SystemScope { job: SystemJob }               // constructors pub(crate) per job, in auth::system
pub enum SystemJob { InterruptAtStart, StartupGc, ResolveToken, ListOrgsOfUser, StoreCopy }
pub struct Tx { /* Option<deadpool_postgres::Object>, committed: bool */ }
impl Tx { pub async fn query(&self, sql: &str, p: &[&(dyn ToSql + Sync)]) -> Result<Vec<Row>>;
          pub async fn query_opt(..) -> Result<Option<Row>>; pub async fn query_one(..) -> Result<Row>;
          pub async fn execute(..) -> Result<u64>; pub async fn commit(self) -> Result<()>; }
// Drop without commit: deadpool_postgres::Object::take(conn); the connection closes, never goes back mid-transaction.
impl Store { pub async fn tx(&self, s: &Scope) -> Result<Tx>; pub async fn system_tx(&self, s: &SystemScope) -> Result<Tx>; }
```
- `test_scope` reaches integration tests through `[dev-dependencies] shaked = { path = ".", features = ["testkit"] }`.
- `tx` runs `BEGIN`, then `SET LOCAL ROLE shaked_app` when `set_role` is true, then the DDL §5 `set_config` line
  (`app.team_ids` is the comma list, `app.org_role` the lowercase role). `system_tx` uses the `system` pool and
  `SET LOCAL ROLE shaked_system`.
- `jobs::interrupt_at_start` now uses `system_tx(SystemJob::InterruptAtStart)`.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/rls.rs: fixtures/orgs.toml seeds org A and org B, a team in each, owner/member/viewer/billing users
#[test] fn every_shakedown_table_has_forced_rls() {
    // SELECT relname FROM pg_class WHERE relnamespace='shakedown'::regnamespace AND relkind IN ('r','p')
    //   AND NOT (relrowsecurity AND relforcerowsecurity)  => empty
}
#[test] fn every_identity_org_table_has_rls() { /* orgs, memberships, teams, team_members, service_accounts, users, audit_log */ }
#[test] fn rls_refuses_cross_org_reads_and_writes_on_every_table() {
    for t in SHAKEDOWN_TABLES {          // compared with pg_class: the list must name every table
        insert_fixture_row(t, A).await;
        let b = store.tx(&owner_of(B)).await?;
        assert_eq!(count(&b, t).await, 0, "{t} read");
        assert_eq!(b.execute(&update_all(t), &[]).await?, 0, "{t} update");
        assert_eq!(b.execute(&delete_all(t), &[]).await?, 0, "{t} delete");
        assert!(insert_row_for(&b, t, A).await.is_err(), "{t} insert into A");
    }
}
#[test] fn root_visibility_by_team_seat_creator_and_grant() {
    // member without seat: team-scoped run invisible; with seat visible; creator sees own; team grant shows it
}
#[test] fn viewers_and_billing_cannot_write() { /* viewer INSERT runs => error; billing SELECT runs => 0 */ }
#[test] fn no_scope_no_rows() { /* raw shaked_app connection without set_config => 0 rows everywhere */ }
#[test] fn an_uncommitted_tx_never_returns_its_connection() { /* drop Tx mid-way; pool status: size decreased */ }
// coppertide-schema tests/migrate.rs
#[test] fn shakedown_2_on_todays_data_gives_every_row_the_local_org() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test rls`
- [ ] **Step 3: Implement** the migration and the interfaces above. Store methods stay unscoped for now and still
  work through the local org default.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Every Shakedown record belongs to an org, row-level security keeps orgs apart, and queries run in a transaction scoped to who asks".

### Task 6: Every store query under a `Scope`; `shakedown` 3

**Files:**
- Modify: `core/crates/shaked/src/store.rs` and `store/*.rs`. Public methods move from `impl Store` to `impl
  Scoped<'_>`. Inserts set `org_id = scope.org()` (and `created_by` on root tables). Every SELECT, UPDATE and DELETE
  also filters `org_id = $org`. A child getter joins its root (`branches` JOIN `runs`), so a branch of an invisible
  run is "not found".
- Modify: the callers, about 92 call sites in 35 files: `api/*` take `Caller`; `exec.rs` `BranchCtx` gains `scope:
  Arc<Scope>`; runners, suites, experiments, review jobs and dispatch use `ctx.scope` or their starter's scope;
  `gc.rs` uses `SystemJob::StartupGc`. Tests: about 139 uses, switched to `store.at(&local_owner())`.
- Create: `core/crates/shaked/src/auth/extract.rs`.
- Create: `core/crates/coppertide-schema/migrations/shakedown/0003_validate.sql`: `VALIDATE CONSTRAINT` for each
  NOT VALID FK, then `ALTER COLUMN org_id DROP DEFAULT` with `-- contract: unused since <Task 6 sha>`.
- Test: `core/crates/shaked/tests/store.rs` (+), `tests/rls.rs` (+).

**Interfaces:**
- Consumes: Task 5 `Scope`, `Tx`, `Store::tx`, `local_owner`.
- Produces:
```rust
pub struct Scoped<'a> { store: &'a Store, scope: &'a Scope }
impl Store { pub fn at<'a>(&'a self, scope: &'a Scope) -> Scoped<'a>; }   // the only way to the record methods
pub struct Caller(pub Arc<Scope>);   // axum FromRequestParts: reads the Arc<Scope> the auth layer put in extensions
// local mode: the /api layer inserts Arc::new(local_owner()) for every request that passed the host guard.
```
- Handlers that act on a resource call `can()` before acting, and answer 404 for `Level::None` (it never says
  "forbidden" about another org's id) and 403 for a lower level.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn org_b_cannot_read_or_append_to_org_a_branches_through_the_store() {
    let a = store.at(&owner_of(A)).create_run("t", &team).await?;   // + branch, event
    assert!(store.at(&owner_of(B)).find_branch(&br).await?.is_none());
    assert!(store.at(&owner_of(B)).append(&br, None, EventKind::Note, json!({})).await.is_err());
    assert!(store.at(&owner_of(B)).list_runs().await?.is_empty());
}
#[test] fn a_branch_of_an_invisible_run_is_not_found() { /* member w/o seat, team-scoped run */ }
#[test] fn inserts_without_org_fail_after_shakedown_3() { /* raw INSERT INTO runs (id, task_id, team) => error */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test store --test rls`
- [ ] **Step 3: Implement.** Make the change in one sweep, so the compiler finds every call site (`Store` keeps no
  public record method). Commit when the gate is green.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Every query shaked makes names its org and runs under the caller's scope, and new records must say whose they are".

### Task 7: Hosted request auth: `signing` v2, live role and teams, `/api/me` (⚑ D15)

**Files:**
- Create: `core/crates/shaked/src/auth/{signing.rs, middleware.rs, identity.rs}`; `src/api/me.rs`.
- Modify: `src/app.rs` (the hosted `/api` layer replaces Task 3's blanket 401), `src/main.rs` (`--hosted` →
  `Store::open_hosted`, `SHAKEDOWN_GATEWAY_SECRET_FILE` required).
- Test: `core/crates/shaked/tests/auth.rs`.

**Interfaces:**
- Produces:
```rust
// auth/signing.rs: header names in one place (phase-3 rename may change them)
pub const USER_HEADER: &str = "x-sculptured-user"; pub const ORG_HEADER: &str = "x-sculptured-org";
pub const TS_HEADER: &str = "x-sculptured-ts";     pub const SIG_HEADER: &str = "x-sculptured-sig";
pub const MAX_SKEW_SECS: i64 = 60;
pub fn sign_v2(secret: &[u8], method: &str, path_and_query: &str, user: &str, org: &str, ts: i64) -> String; // hex HMAC-SHA256 of "METHOD\npath\nuser\norg\nts"
pub fn verify_v2(secret: &[u8], method: &str, path_and_query: &str, user: &str, org: &str, ts: i64, sig: &str, now: i64) -> bool; // constant-time
pub const V2_VECTOR: (&str, &str, &str, &str, &str, i64, &str);  // secret, method, path, user, org, ts, expected hex: shared with CopperTide `signing`
// auth/identity.rs
pub async fn principal_of_user(store: &Store, user: Uuid, org: Uuid) -> Result<Option<Principal>>; // live membership + seats; None if not a member / org deleted
pub async fn orgs_of_user(store: &Store, user: Uuid) -> Result<Vec<OrgSummary>>;                    // SystemJob::ListOrgsOfUser
pub struct OrgSummary { pub id: Uuid, pub slug: String, pub name: String, pub kind: String, pub role: OrgRole }
// GET /api/me => {mode, user:{id,name,email}, org:OrgSummary, role, orgs:[OrgSummary] (hosted only), features:[..], switch_org_url?:String}
```
- The middleware strips any inbound `x-sculptured-*` in local mode. In hosted mode, the order is: a Bearer token
  (Task 8), else a signature; neither gives 401. A bad signature or skew also gives 401. A signed user who is not a
  member of the signed org gets 403 `{"error":"not a member of this org"}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn v2_vector_verifies_and_any_field_change_fails() { /* V2_VECTOR; flip org => false; ts+61 => false */ }
#[test] fn hosted_signed_member_lists_only_their_orgs_runs() { /* AppFx::hosted with two orgs from fixtures */ }
#[test] fn hosted_removed_member_is_refused_on_the_next_request() { /* DELETE membership; next call => 403 */ }
#[test] fn hosted_deleted_org_is_refused() { /* orgs.deleted_at set => 403 */ }
#[test] fn local_ignores_signature_headers() { /* forged x-sculptured-org header has no effect; still local org */ }
#[test] fn a_viewer_gets_403_starting_a_run_and_404_for_other_orgs() { /* signed viewer of A: POST /api/runs 403; A's run id as B 404 */ }
#[test] fn me_says_mode_and_org() { /* local: mode "local", org.kind "local", no orgs list */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test auth`
- [ ] **Step 3: Implement** the interfaces above. `tests/support/app.rs` gains `AppSetup::hosted(fixtures)`.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "A hosted shaked takes requests the gateway signed for a user and an org, and reads that user's role and teams on every request".

### Task 8: API tokens: create, list, revoke, `shaked token create`, bearer auth, SDK `token=`

**Files:**
- Create: `core/crates/shaked/src/auth/tokens.rs`, `src/store/api_tokens.rs`, `src/api/tokens.rs`, `src/cli.rs`.
- Modify: `src/main.rs` (subcommands go through `cli.rs`: `base`, `token create`, and later `store copy`),
  `src/auth/middleware.rs`.
- Modify: `sdk/python/src/shakedown/client.py` (`Shakedown(url, token=None)`, falling back to `SHAKEDOWN_TOKEN`,
  sent as `Authorization: Bearer`; never in `repr` or errors); `sdk/python/tests/test_units.py`.
- Test: `core/crates/shaked/tests/tokens.rs`.

**Interfaces:**
- Consumes: Task 4 `TokenScope`, Task 5 `SystemJob::ResolveToken`.
- Produces:
```rust
pub struct Minted { pub id: Uuid, pub token: String, pub prefix: String }   // token shown once
pub fn new_secret(prefix: &'static str) -> String;        // prefix + 64 hex (ring SystemRandom, 32 bytes)
pub fn hash(token: &str) -> String;                       // sha256 hex
pub async fn resolve_api_token(store: &Store, bearer: &str) -> Result<Option<Principal>>; // one fixed query on token_hash; refuses revoked/expired/disabled SA/removed member; bumps last_used_at at most once a minute
impl Scoped<'_> { pub async fn create_api_token(&self, name: &str, scopes: &[TokenScope], team: Option<Uuid>, expires: Option<DateTime<Utc>>) -> Result<Minted>;
                  pub async fn list_api_tokens(&self) -> Result<Vec<ApiTokenRow>>;  // no hash, no secret
                  pub async fn revoke_api_token(&self, id: Uuid) -> Result<bool>; }
// routes: GET/POST /api/tokens, DELETE /api/tokens/{id}; POST body {name, scopes, expires_days?}; answer {id, token, prefix} once
// CLI: shaked token create --name <n> [--scope read|run|admin ...] [--expires-days N]  -> prints the token only (local owner)
pub async fn audit(tx: &Tx, scope: &Scope, action: &str, target: (&str, &str), detail: Value) -> Result<()>;
// auth::audit: one identity.audit_log row (actor from the scope's principal; system writes: actor_kind "system").
// Written for token.created / token.revoked here, provider_key.added / .replaced / .deleted in Task 11. Never secrets.
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_token_is_shown_once_and_stored_hashed() { /* list has prefix "sd_xxxxx", no "token"; db token_hash == hash(token) */ }
#[test] fn bearer_tokens_act_as_their_holder_capped_by_scope() {
    // hosted: read token GET /api/runs 200, POST /api/runs 403; run token POST 2xx; revoked => 401; expired => 401
}
#[test] fn removing_a_member_kills_their_tokens() { /* DELETE membership => row gone (FK cascade) => 401 */ }
#[test] fn creating_and_revoking_a_token_is_audited_without_the_secret() { /* 2 audit_log rows; detail has prefix, not token */ }
#[test] fn a_viewer_may_mint_read_tokens_only() { /* scopes ["run"] => 403 */ }
#[test] fn cli_token_create_prints_a_working_token() { /* run the built binary on a fresh db; GET /api/me with it => 200 */ }
```
```python
def test_token_is_sent_and_never_shown(monkeypatch): ...  # header "Bearer sd_x"; "sd_x" not in repr(client) nor str(error)
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test tokens`; `uv run pytest -q -k token` in `sdk/python`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the gate**, plus the SDK tests.
- [ ] **Step 5: Commit:** "API tokens: sd_ tokens shown once and stored hashed, capped by read, run or admin, made in the API or with shaked token create, and taken by the SDK".

### Task 9: Run, agent and worker tokens for every agent kind; proxy tickets expire; no tokenless calls

**Files:**
- Create: `core/crates/shaked/src/auth/run_tokens.rs`, `src/store/run_tokens.rs`.
- Modify: `src/exec/roster.rs` (`tokens` becomes `HashMap<String, Held { hash, expires: Instant, revoked }>`;
  `token_ok` is false for an agent never given a token); `src/exec.rs` (mint at branch start, revoke at end);
  `src/window.rs`; `src/agents/mod.rs` (`Mcp` gains `token`); `src/agents/claude.rs` (0600 mcp-config file with
  `headers`); `src/agents/codex.rs` (`bearer_token_env_var`); `src/agents/open.rs` (Authorization on gateway
  calls, built-in and Bedrock); `src/agents/http.rs` (the existing `token`, new `gateway_url`); `src/external.rs`
  (`Endpoints.token`, `Started.run_token`); `src/driver.rs` (`SHAKEDOWN_RUN_TOKEN` and one `SHAKEDOWN_AGENT_TOKEN_<AGENT>`
  for each agent); `src/proxy/tickets.rs`, `src/proxy/env.rs`; `src/auth/middleware.rs` (`sdr_` and `sdw_` on
  run-internal `/api` routes); `src/mcp.rs`, `src/gateway.rs`.
- Modify: `docs/agents.md`, `docs/protocol.md` (`gateway_url` in the HTTP run request; tokens in the register answer).
- Test: `core/crates/shaked/tests/run_tokens.rs`; `tests/cli.rs` (+); `tests/proxy.rs` (+).

**Interfaces:**
- Consumes: Task 8 `new_secret`, `hash`; Task 4 `RunTokenKind`, `can`.
- Produces:
```rust
impl Roster {
    pub fn issue_token(&self, agent: &str, ttl: Duration) -> String;   // "sda_…"; replaces the uuid token
    pub fn token_of(&self, agent: &str) -> Option<String>;             // plain, in memory only, for handing to the agent
    pub fn revoke_token(&self, agent: &str); pub fn revoke_all(&self);
    pub fn extend(&self, by: Duration);                                // called by FlowControl on resume with the paused time
    pub fn token_ok(&self, agent: &str, authorization: Option<&str>) -> bool;  // false: none issued, revoked, expired, mismatch
}
impl Scoped<'_> { pub async fn mint_run_token(&self, run: &str, branch: &str, kind: RunTokenKind, agent: Option<&str>,
                      sandbox: Option<Uuid>, expires: DateTime<Utc>) -> Result<String>;   // stores hash only
                  pub async fn revoke_run_tokens(&self, branch: &str, agent: Option<&str>) -> Result<u64>; }
pub async fn resolve_run_token(store: &Store, bearer: &str) -> Result<Option<Principal>>;   // sdr_/sdw_ via ResolveToken
impl Rules { pub fn token_ttl(&self, branch_max: Duration) -> Duration; }   // the run's own limit, else quota branch_seconds_max
impl Tickets { pub fn issue(&self, branch: &str, agent: Option<&str>, expires: Instant) -> String;
               pub fn revoke(&self, branch: &str, agent: Option<&str>); pub fn extend(&self, branch: &str, by: Duration); }
pub const AGENT_TOKEN_ENV: &str = "SHAKEDOWN_AGENT_TOKEN";
```
- Carriage (hosted spec M1 table):

| Kind | MCP | Model gateway |
|---|---|---|
| built-in, Bedrock | `Authorization: Bearer` (in process) | same |
| Claude Code | the `headers` of its `--mcp-config` file | — (M2) |
| Codex | `-c mcp_servers.<s>.bearer_token_env_var="SHAKEDOWN_AGENT_TOKEN"` | — (M2) |
| HTTP | `token` in the run request | `gateway_url` in the run request |
| external, SDK, drivers | `token` in each agent's register answer and `Started` | same |

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn tokenless_mcp_and_gateway_calls_get_401_for_every_kind() {
    for kind in ["open", "bedrock", "claude-code", "codex", "http", "external", "driver"] {
        // start a live branch with one agent of `kind` (fake binaries / fake_llm / toy driver)
        assert_eq!(post(mcp_url, no_auth).status(), 401, "{kind} mcp");
        assert_eq!(post(gateway_url, no_auth).status(), 401, "{kind} gateway");
        assert_eq!(post(mcp_url, bearer(&token_of(kind))).status(), 200, "{kind} with token");
    }
}
#[test] fn claude_gets_its_token_in_a_private_config_file() {
    // args contain "--mcp-config" <path>; mode 0o600; json mcpServers.<s>.headers.Authorization == "Bearer sda_…"; file gone after the run
}
#[test] fn codex_reads_its_token_from_the_env() { /* arg bearer_token_env_var="SHAKEDOWN_AGENT_TOKEN"; env has it */ }
#[test] fn tokens_and_tickets_die_with_the_agent_and_the_branch() { /* after end: 401; proxy 407 */ }
#[test] fn a_long_pause_does_not_expire_agent_tokens() { /* ttl 200ms, pause 400ms, resume => 200 */ }
#[test] fn an_expired_ticket_is_refused_by_the_proxy() {}
#[test] fn no_token_reaches_events_or_argv() {
    // after a run of each kind: no event payload, recorded stderr, or Command argv contains "sd_" / "sdr_" / "sda_"
}
#[test] fn run_tokens_reach_only_their_branch() { /* hosted: sdr_ of b1 POST /api/branches/b2/finish => 404 */ }
#[test] fn a_worker_token_needs_its_sandbox() { /* mint kind Worker with a 'fake' sandboxes row; resolves to RunToken{kind: Worker} */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test run_tokens --test cli --test proxy`
- [ ] **Step 3: Implement** the interfaces above. Change the tests that called MCP or the gateway without a token
  to use `token_of`. Don't loosen their assertions.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Every agent carries a token that dies with it: no MCP or model gateway call gets in without one, and proxy tickets expire with it".

### Task 10: Quotas and usage: presets, metering, checks (⚑ D17)

**Files:**
- Create: `docs/quotas.toml` (plans `local` and `beta`), `core/crates/shaked/src/quotas.rs`, `src/store/usage.rs`.
- Modify: `src/gateway.rs` (meter `model_usd_micros` and `model_tokens` for each recorded call), `src/api.rs`
  (`create_run` meters `runs` and checks it), `src/budget.rs` (monthly spend check beside the per-run one).
- Test: `core/crates/shaked/tests/quotas.rs`; unit test for parsing `docs/quotas.toml`.

**Interfaces:**
- Produces:
```rust
pub enum Meter { ConcurrentSandboxes, SandboxSeconds, BranchSecondsMax, SavesBytes, ModelUsdMicros, TrialUsdMicros, Runs, Seats, ModelTokens }
pub struct Limit { pub value: u64, pub hard: bool, pub period: Period }      // Period::Month | Period::None
pub struct Presets(BTreeMap<String, BTreeMap<Meter, Limit>>);
impl Presets { pub fn built_in() -> &'static Presets; pub fn load(path: Option<&Path>) -> Result<Presets>; } // SHAKEDOWN_QUOTAS overrides
pub enum Verdict { Ok, Warn(String), Refuse(String) }                        // message names the meter and the limit
impl Scoped<'_> {
    pub async fn limit(&self, m: Meter) -> Result<Option<Limit>>;            // quotas row, else the org plan's preset; None = unlimited
    pub async fn meter(&self, m: Meter, qty: i64, source: &str, key: &str, run: Option<&str>, branch: Option<&str>) -> Result<bool>; // idempotent on key; upserts usage_monthly in the same tx; false = duplicate
    pub async fn check(&self, m: Meter, adding: u64) -> Result<Verdict>;     // reads usage_monthly / live rows
    pub async fn usage_by_month(&self) -> Result<Vec<(NaiveDate, Meter, i64)>>; // can(Usage) checked by the route
}
// GET /api/usage/org => this org's monthly totals (owner/admin/billing; member gets own runs' cost)
```
- `docs/quotas.toml`, `beta`: `concurrent_sandboxes 2`, `sandbox_seconds 180000/month`, `branch_seconds_max
  3600`, `saves_bytes 21474836480`, `trial_usd_micros 0` (owner sets it). `model_usd_micros` and `seats` are
  absent, which means unlimited in `shaked`. `local`: `branch_seconds_max 86400` and nothing else.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn built_in_presets_parse_and_beta_has_the_s4_numbers() {}
#[test] fn metering_is_idempotent_and_totals_monthly() { /* same key twice => one usage row; usage_monthly == qty */ }
#[test] fn an_override_row_beats_the_preset() {}
#[test] fn over_a_hard_runs_quota_a_run_is_refused_naming_the_quota() { /* POST /api/runs => 429, error contains "runs" */ }
#[test] fn a_soft_quota_warns_and_lets_the_run_start() {}
#[test] fn every_recorded_model_call_is_metered_once() { /* fake_llm call through gateway => 1 model_usd_micros row with key gateway:<branch>:<seq> */ }
#[test] fn local_runs_are_unlimited_but_metered() {}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test quotas`
- [ ] **Step 3: Implement** the interfaces above. The idempotency keys are `gateway:<branch>:<seq>` and `api:run:<run_id>`.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Each org's usage is metered once per call and per run, summed by month, and checked against its plan's quotas from docs/quotas.toml".

### Task 11: Per-org provider keys: vault, API, Test, lookup, trial credit (⚑ D20)

**Files:**
- Create: `core/crates/shaked/src/vault.rs` (a port of Cloud `vault.rs`: same format and errors, env
  `SHAKEDOWN_VAULT_KEY` / `SHAKEDOWN_VAULT_KEY_FILE`; local default `<arena>/core/vault.key`, created 0600 on first
  seal and added to `.gitignore`; hosted refuses to start without it).
- Create: `src/store/provider_keys.rs`, `src/api/models.rs`, `src/providers/org_keys.rs`, `src/providers/reach.rs` (public-address check).
- Modify: `src/providers/mod.rs` (`resolve` consults the org's keys first), `src/gateway.rs` and `src/agents/open.rs`
  (resolve with `ctx.scope`), `src/api.rs` (`/api/providers` lists org keys beside the env ones).
- Test: `core/crates/shaked/tests/provider_keys.rs`; unit tests in `vault.rs` and `reach.rs`.

**Interfaces:**
- Consumes: Task 10 `Scoped::meter`, `check`, `Meter::TrialUsdMicros`; Task 5 `Scope`.
- Produces:
```rust
pub struct Vault; impl Vault { pub fn from_hex(h: &str) -> Result<Vault, String>; pub fn from_env(mode: Mode) -> Result<Option<Vault>, String>;
    pub fn seal(&self, context: &str, plain: &[u8]) -> Vec<u8>; pub fn open(&self, context: &str, sealed: &[u8]) -> Result<Zeroed, String>; }
pub fn key_context(org: Uuid, name: &str) -> String;          // "provider_key:<org>:<name>"
pub enum KeyKind { Anthropic, OpenAi, OpenRouter, Bedrock, Custom }
pub struct NewKey<'a> { pub name: &'a str, pub kind: KeyKind, pub secret: &'a str, pub base_url: Option<&'a str>,
                        pub model: Option<&'a str>, pub prices: Option<&'a Value>, pub team: Option<Uuid> }
pub struct KeyRow { pub id: Uuid, pub name: String, pub kind: KeyKind, pub base_url: Option<String>, pub key_last4: String,
                    pub checked_ok: Option<bool>, pub checked_at: Option<DateTime<Utc>>, pub created_at: DateTime<Utc> } // never the secret
impl Scoped<'_> { pub async fn add_key(&self, v: &Vault, k: NewKey<'_>) -> Result<KeyRow>;
                  pub async fn replace_key_secret(&self, v: &Vault, id: Uuid, secret: &str) -> Result<bool>;
                  pub async fn list_keys(&self) -> Result<Vec<KeyRow>>; pub async fn delete_key(&self, id: Uuid) -> Result<bool>;
                  pub async fn test_key(&self, v: &Vault, http: &reqwest::Client, id: Uuid) -> Result<KeyRow>; } // one-token call; stores checked_ok/detail (provider message redacted)
pub enum KeySource { Org(Uuid), Env, Trial }
pub async fn provider_for(state: &AppState, scope: &Scope, id: Option<&str>) -> Result<(Provider, KeySource), String>;
// order: org key (id "anthropic"/"openai"/"openrouter"/"bedrock:<region>"/"custom:<name>") -> Env (local only) -> Trial (hosted, env key, trial check Ok)
pub fn public_https(url: &Url) -> Result<(), String>;        // hosted custom URLs: https + every resolved IP public (no private, loopback, link-local, 169.254.169.254, fc00::/7)
// routes: GET/POST /api/models/keys, POST /api/models/keys/{id}/secret, POST /api/models/keys/{id}/test, DELETE /api/models/keys/{id}
```
- Bedrock's secret is JSON `{access_key_id, secret_access_key, session_token?}`; `base_url` holds the region.
  `custom:<name>` takes over from `SHAKEDOWN_LOCAL_MODELS`, which is still read in local mode. Trial calls meter
  `trial_usd_micros`, and the call is refused with 402 once the trial is spent.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_sealed_key_opens_only_for_its_org_and_name() { /* context of org B or name "y" => Err; bytes don't contain the secret */ }
#[test] fn keys_are_write_only() { /* POST then GET /api/models/keys: no "secret", "sealed_key"; key_last4 == last 4 */ }
#[test] fn the_org_key_wins_over_the_env_locally_and_env_is_never_used_hosted() {
    // local: org anthropic key -> KeySource::Org; deleted -> Env; hosted no key, trial 0 -> Err naming Settings → Models
}
#[test] fn trial_credit_is_metered_and_runs_out() { /* hosted, trial 10 micros, fake_llm cost 6 => 2nd call 402 */ }
#[test] fn test_makes_a_one_token_call_and_records_the_outcome() { /* fake_llm sees max_tokens 1; checked_ok true; bad key => false, detail without key */ }
#[test] fn custom_urls_local_ok_hosted_private_refused() {
    // local http://127.0.0.1:8000 ok; hosted http://… refused; hosted https://10.0.0.5 refused; https://169.254.169.254 refused
}
#[test] fn org_b_cannot_list_use_or_test_org_a_keys() { /* 404s; an agent of org B naming custom:<A's name> => unknown provider */ }
#[test] fn a_viewer_cannot_add_keys_and_a_member_cannot_add_org_wide_ones() { /* matrix: create org-wide keys owner/admin only */ }
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test provider_keys` and `cargo test -p shaked vault:: reach::`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Each org keeps its own model keys and OpenAI-compatible servers, sealed and write-only, with a Test call, and a hosted trial on our keys is metered".

### Task 12: Object storage layer (⚑ D19)

**Files:**
- Create: `core/crates/shaked/src/objects/{mod.rs, url.rs, keys.rs, copy.rs}`, `src/store/saves.rs`.
- Modify: `core/crates/shaked/Cargo.toml` (`object_store = { version = "0.12", default-features = false, features
  = ["aws", "fs"] }`: check for the newest 0.x at task start and pin it); `src/app.rs` (`AppState.objects:
  Option<Arc<ObjectStores>>`); `src/main.rs` (hosted startup probe); `src/cli.rs` (`store copy`).
- Test: `core/crates/shaked/tests/objects.rs` (memory, file, and `live_minio_*`); unit tests in `url.rs` and `keys.rs`.

**Interfaces:**
- Consumes: Task 5 `saves` table (`store`, `object_key`, `sha256`, `bytes`, `state`, `expires_at`);
  `SystemJob::StoreCopy`.
- Produces:
```rust
pub struct StoreName(String);                       // ^[a-z][a-z0-9-]{0,31}$
pub struct ObjectKey(String);                       // "<org_uuid>/<kind>/…", no "..", no leading "/"
pub struct ObjectRef { pub store: StoreName, pub key: ObjectKey }   // what rows keep
pub fn parse_spec(spec: &str, env: impl Fn(&str) -> Option<String>) -> Result<Vec<(StoreName, Arc<dyn ObjectStore>)>, String>;
// SHAKEDOWN_OBJECT_STORE = "name=url,name=url" or one bare url (name "default"); the first is the write store unless
// SHAKEDOWN_OBJECT_STORE_WRITE names another. URLs:
//   r2://<bucket>?account=<id>[&prefix=p]       -> AmazonS3 at https://<id>.r2.cloudflarestorage.com, region "auto"
//   s3://<bucket>?region=<r>[&endpoint=<u>][&allow_http=true][&prefix=p]
//   file:///abs/dir      memory://
// Credentials only from env: SHAKEDOWN_OBJECT_STORE_<NAME>_ACCESS_KEY_ID / _SECRET_ACCESS_KEY, else AWS_* ; a URL with userinfo or a
// secret-looking query key is refused (URLs reach logs).
pub struct ObjectStores { write: StoreName, all: BTreeMap<StoreName, Arc<dyn ObjectStore>> }
impl ObjectStores {
    pub fn from_env() -> Result<Option<ObjectStores>, String>;
    pub async fn put(&self, key: &ObjectKey, body: impl Stream<Item = io::Result<Bytes>>) -> Result<Stored>; // to the write store, multipart, sha256 on the fly
    pub async fn get(&self, r: &ObjectRef) -> Result<BoxStream<'static, Result<Bytes>>>;                  // any registered store
    pub async fn delete_prefix(&self, store: &StoreName, prefix: &str) -> Result<u64>;
    pub async fn probe(&self) -> Result<()>;                         // put/get/delete "_probe/<uuid>" on the write store
}
pub struct Stored { pub r: ObjectRef, pub bytes: u64, pub sha256: String }
pub struct CopyReport { pub copied: u64, pub skipped: u64, pub failed: Vec<(String, String)>, pub repointed: u64 }
pub async fn copy(stores: &ObjectStores, from: &StoreName, to: &StoreName, prefix: Option<&str>, repoint: Option<&Store>) -> Result<CopyReport>;
// For each object: skip if the destination exists with the same size and sha256 (recorded in saves.sha256, else
// recomputed). Else stream-copy, re-read the destination, compare sha256. With repoint, UPDATE saves SET store=to
// only for verified keys, under system_tx(StoreCopy).
pub mod keys { pub fn save(org: Uuid, integration: &str, save_id: &str) -> ObjectKey;   // <org>/saves/<integration>/<save_id>/
               pub fn transcript(org: Uuid, run: &str, branch: &str, agent: &str) -> ObjectKey; // <org>/transcripts/<run>/<branch>/<agent>.jsonl
               pub fn export(org: Uuid, id: &str) -> ObjectKey; }                     // <org>/exports/<id>/
impl Scoped<'_> { pub async fn record_save(&self, run: &str, branch: &str, integration: &str, save_id: &str, seq: i64,
                      s: &Stored, retention: Duration) -> Result<Uuid>; }            // state ready, expires_at = now + org retention_days
// CLI: shaked store copy --from <name> --to <name> [--prefix <org>/] [--repoint]   (exit 1 if any failed)
```
- What moves in M1: nothing (see Calls made). M2's worker is the first caller of `put` and `record_save`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn urls_parse_to_the_right_backends() {
    // r2://b?account=acc => endpoint "https://acc.r2.cloudflarestorage.com", region "auto"; s3://b?region=eu-west-1;
    // "s3://AKIA:secret@b" => Err; "s3://b?secret_access_key=x" => Err; "ftp://x" => Err; two names, WRITE picks
}
#[test] fn keys_stay_under_their_org() { /* "../" or absolute segments => Err; save(org,..) starts with "<org>/saves/" */ }
#[test] fn put_get_roundtrip_with_checksum_on_memory_and_file() {}
#[test] fn reads_come_from_the_store_a_row_names_after_the_write_store_changes() {
    // put on "a"; restart with write "b" (a still registered); get(ObjectRef{a,key}) ok
}
#[test] fn copy_verifies_and_repoints() { /* memory a -> file b; report.copied==n; saves.store=="b" for all */ }
#[test] fn copy_reruns_idempotently_and_never_repoints_a_bad_object() {
    // corrupt one dest object between runs (wrong bytes): 2nd run skipped==n-1, failed==[that key], its row still "a"
}
#[test] fn hosted_start_probes_the_write_store() { /* unreachable store => start fails naming the store, not the URL's credentials */ }
#[test] #[ignore] fn live_minio_s3_put_get_copy() {
    // docker run --rm -d --name shaked-minio-<uuid> minio/minio server /data; s3://bucket?endpoint=http://127.0.0.1:<port>&region=us-east-1&allow_http=true
    // roundtrip a 12 MiB multipart object; copy minio -> file and verify; container removed in Drop
}
```

- [ ] **Step 2: Run them and see them fail:** `cargo test -p shaked --test objects`, then `cargo test -p shaked --test objects -- --ignored live_minio`
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run the gate**, plus the live test once.
- [ ] **Step 5: Commit:** "Saves, transcripts and exports have a storage layer over R2, S3, files or memory, switchable by name, with a verified shaked store copy".

### Task 13: Tenant router test: org B gets 404 for org A's ids on every route

**Files:**
- Create: `core/crates/shaked/tests/tenants.rs`, `tests/fixtures/orgs.toml` (two orgs, a team each, one user per
  role; extends Task 5's file).
- Modify: route handlers found to leak. Each fix is in the handler and comes with its row.

**Interfaces:**
- Consumes: Tasks 6–12 routes; `AppSetup::hosted`; Task 7 `sign_v2`.

- [ ] **Step 1: Write the failing test**

```rust
const ROUTES: &[(&str, &str)] = &[ ("GET", "/api/branches/{branch}"), ("POST", "/api/branches/{branch}/fork"), /* … every route … */ ];
#[test] fn the_table_names_every_route() {
    // scan src/api.rs, src/api/*.rs, src/mcp.rs, src/gateway.rs for `.route("…"` strings (incl. method chains);
    // assert each (method, path) is in ROUTES, and each ROUTES entry exists
}
#[test] fn org_b_gets_404_for_org_a_ids_on_every_route() {
    // org A (signed owner) makes a run, branch, fork, suite, experiment, review, connection, key, token, world row, save row
    for (m, p) in ROUTES {
        let url = fill(p, &a_ids);                    // org A's ids
        let r = call_signed(m, &url, B_OWNER).await;  // org B's owner, valid JSON bodies from body_for(p)
        assert!(matches!(r.status().as_u16(), 404 | 405), "{m} {p} => {}", r.status());  // 405 only for org-free routes listed in ORG_FREE
        assert!(!r.text().await.contains(&a_ids.run), "{m} {p} leaks an id");
    }
}
#[test] fn org_b_lists_show_none_of_org_a() { /* GET list routes with B: no A ids in the bodies */ }
#[test] fn org_b_tokens_cannot_reach_org_a() { /* same loop with an sd_ token of B, and an sdr_ run token of B's branch */ }
```

- [ ] **Step 2: Run it and see it fail:** `cargo test -p shaked --test tenants`. Every route that answers anything
  but 404 is a bug to fix.
- [ ] **Step 3: Fix** each leaking handler: the resource lookup goes through `Scoped` and `can()`.
- [ ] **Step 4: Run the gate.**
- [ ] **Step 5: Commit:** "Another org's ids are not found on any route, under a signature, an API token or a run token".

### Task 14: UI: API tokens, Models, org switcher (hosted, dark); docs (⚑ D18)

**Files:**
- Create: `ui/src/api/{me.ts, tokens.ts, models.ts}`, `ui/src/features/settings/{SettingsPage.tsx, TokensPage.tsx,
  ModelsPage.tsx, settings.test.tsx}`, `ui/src/app/OrgSwitcher.tsx`, `ui/src/app/OrgSwitcher.test.tsx`.
- Modify: `ui/src/App.tsx` (routes `/settings/tokens` and `/settings/models`), `ui/src/app/TopBar.tsx` (the
  switcher, and a Settings entry), `ui/src/app/palette.ts` (palette entries).
- Modify: `README.md` "Old names" (rows: `SHAKEDOWN_PORT` → `SHAKEDOWN_BIND`; `SHAKEDOWN_AGENT_BASE` →
  `SHAKEDOWN_PUBLIC_URL`; `SHAKEDOWN_LOCAL_MODELS` → custom model servers in Settings → Models, still read locally;
  tables in `public` → schema `shakedown`, moved on first start), `docs/reference.md` (settings, `/api/me`,
  `/api/tokens`, `/api/models/keys`, `/api/usage/org`, `shaked token create`, `shaked store copy`,
  `SHAKEDOWN_OBJECT_STORE*`, `SHAKEDOWN_VAULT_KEY*`, `SHAKEDOWN_MODE`), `docs/sdk.md` (`token=`), `docs/agents.md`
  (an agent's token for each kind).

**Interfaces:**
- Consumes: `GET /api/me` (Task 7), `/api/tokens` (Task 8), `/api/models/keys` (Task 11).
- Produces: `fetchMe(): Promise<Me>` with `Me = { mode: 'local' | 'hosted'; org: OrgSummary; role: OrgRole; orgs?:
  OrgSummary[]; features: string[]; switch_org_url?: string }`. The switcher renders only when `mode === 'hosted' &&
  features.includes('org_switcher') && (orgs?.length ?? 0) > 1`. It lists the personal org first, and choosing an
  org POSTs `{org_id}` to `switch_org_url` (the gateway owns the session; Cloud's plan adds that route).

- [ ] **Step 1: Write the failing tests**

```tsx
it('shows a new token once, then only its prefix', async () => { /* after create: token text visible + copy; after reload: "sd_xxxxx…" only */ });
it('offers only read scope to a viewer', async () => {});
it('adds a custom server with a URL and runs Test', async () => { /* POST body {kind:'custom', base_url}; shows "Works" / provider detail */ });
it('never renders a key secret after saving', async () => { /* the secret input is cleared; list shows "…last4" */ });
it('hides the org switcher locally and without the flag', async () => {});
it('lists the personal org first and switches through switch_org_url', async () => {});
```

- [ ] **Step 2: Run them and see them fail:** `cd ui && npx vitest run --maxWorkers=1 src/features/settings src/app/OrgSwitcher.test.tsx`
- [ ] **Step 3: Implement** the pages with existing components (`PageHeader`, `PageBody`, forms in the
  ConnectPage style). Use the plain copy above; the UI follows simplicity first.
- [ ] **Step 4: Run** `npx tsc -b && npx vitest run --maxWorkers=1`, and check that every name in the docs matches
  the code: `grep -rn "SHAKEDOWN_BIND\|SHAKEDOWN_OBJECT_STORE" docs README.md`.
- [ ] **Step 5: Commit:** "Settings has API tokens and Models pages, a hosted org switcher waits behind its flag, and the docs list the new settings and the old names they replace".

---

## Order, size and what lands dark

| # | Task | Depends on | Size |
|---|---|---|---|
| 1 | schema crate | — | 2 d |
| 2 | shaked on migrator + pool | 1 | 1.5 d |
| 3 | bind and modes | 2 | 1 d |
| 4 | principals and `can` | — (parallel to 2–3) | 1 d |
| 5 | `shakedown` 2, RLS, `tx` | 2, 4 | 2 d |
| 6 | scoped store, `shakedown` 3 | 5 | 2.5 d |
| 7 | hosted auth, `/api/me` | 3, 6 | 1.5 d |
| 8 | API tokens | 7 | 1.5 d |
| 9 | run/agent/worker tokens | 6, 8 | 2.5 d |
| 10 | quotas and usage | 6 | 1.5 d |
| 11 | provider keys | 10 | 2.5 d |
| 12 | object storage | 5, 6 | 2 d |
| 13 | tenant router test | 7–12 | 1.5 d |
| 14 | UI and docs | 7, 8, 11 | 2 d |

About 25 engineer-days, five weeks for one person. The hosted spec's "about two weeks" left out the schema crate,
object storage and the UI. Every task leaves local mode as it is today, apart from agent tokens in Task 9. Hosted
mode refuses everything until Task 7, and after that it still can't run worlds or code (M2). `update.sh` doesn't
build Shakedown yet (that is M3), so nothing here reaches production. The first production use of this crate is
Cloud's R2, in Cloud's own plan, once `factory` is merged to `main`.
