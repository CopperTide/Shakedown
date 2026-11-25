# CopperTide approvals (`sculptured`)

Puts CopperTide's approvals in a Shakedown world: changes proposed to a business system, and the decisions on them.
Each instance runs a CopperTide server of its own, and the tools propose, list and decide changes through that
server's HTTP API, as an outside system would. No CopperTide agent runs for it. The `factory` and `pharma`
benchmarks use it as their approvals part, with links that hand each proposed change to whoever decides it. The
package name `sculptured` is the integration's code identifier, kept from before the CopperTide rename.

| | |
|---|---|
| Program | `shakedown-sculptured` (Rust, crate `shakedown-sculptured`) |
| Restore | exact, from a save (SQLite online backups plus a copy of the server's data folder). Only with the same CopperTide build. No approximate restore. |
| World clock | not used |
| Agent tools | `propose_change`, `list_changes`, `decide_change` |

## What it needs

- **The CopperTide server program**, built from the CopperTide repository. It is run as it is, never built here.
  The config's `binary` names it. Without one, the program takes the first `coppertide` on `PATH`, else the first
  `sculptured` (the program's name before the rename). `shaked` passes `PATH`, so the lookup works under it.
- **The route for outside proposers**, `POST /api/teams/{id}/changes`, in that build. As of 2026-11-22 the route is
  on the CopperTide repository's `platform` branch, whose program is still named `sculptured`. The `main` branch's
  program, `coppertide`, lists changes (`GET`) but has no such route, so `propose_change` cannot work against it.
- Linux: stopping an instance reads `/proc` to check which processes to kill.
- No Docker and no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` | Where saves and the running instances' folders go. `shaked` always passes its own. |
| `SHAKEDOWN_COPPERTIDE_BINARY`, `SHAKEDOWN_SCULPTURED_BINARY` | none | Read by the live test only, to find the server program. The integration itself reads neither. |

## Use it

A world names it as a part, for example `{part: "approvals", integration: "sculptured", config: {}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `binary` | string | `coppertide` on `PATH`, else `sculptured` | The CopperTide server program. It must be a file. |
| `team` | string | `shakedown` | The team whose changes the tools propose, list and decide: 1 to 40 of `a-z`, `0-9` and single inner dashes, not `import`, `import-file` or `check-file`. |

| Tool | Effect | Arguments and answer |
|---|---|---|
| `propose_change` | write | `action`, `summary`, `payload`, `system?`, `reason?`. Sent to `POST /api/teams/{team}/changes` with `proposer: "shakedown"`. Answers the change, writes `change/<id>`, and gives a `change_proposed` world event whose `data` is the change. |
| `list_changes` | read | none. The team's changes, newest first. Reads every change's key. |
| `decide_change` | write | `id`, `approved`, `note?` (only with `approved: false`). Answers the change's open question as a person would, waits until the change shows the decision, and answers the change. Writes `change/<id>`. |

A change is `{id, action, system, summary, payload, reason, decision}`. `decision` is `{approved, by, note}`, with
`by` one of `person`, `mandate` or `nobody-in-time`, or null while the change waits. CopperTide's times and
proposing agent are left out. CopperTide shows the proposer as `external.shakedown@<team>`.

Refusals are the tool's `error` and change nothing (`reads` and `writes` empty): bad arguments, CopperTide refusing
a proposal, an unknown change, a change already decided, a change that can no longer be decided, and 8 changes
already waiting. An unknown tool is `-32602`. A server that cannot be reached fails the call.

### Instances

- **The server.** Each instance runs `<binary> serve --port 0 --data-dir <saves>/sculptured/.live/<instance>/data`
  with `--web-dir` set to a folder that does not exist (no web UI). It runs in a process group of its own, in the
  instance folder, with a cleared environment: `HOME` is the instance's `home/`, `TMPDIR` a short scratch folder
  `<temp>/shakedown-sc-<16 hex>` (short enough for the runner's socket path), `RUST_LOG=warn`, and `PATH`. The system
  picks the loopback port, read from `data/host-local/controld.url`. The start waits up to 60 s until the server's
  runner is connected (`GET /api/hosts` lists `local`), then makes sure the team exists. The server's log is
  `serve.log` in the instance folder.
- **The team** is made on the first start of a fresh server: `POST /api/teams` with `{template: "blank", name}` (its
  id is its name, and it has no agents), then its settings with `question_timeout_min: 1440`. A restored server
  already has it. The team has no rules, so every change waits for a person's decision. After a day (wall clock)
  unanswered, CopperTide records the change not approved, by `nobody-in-time`.
- **The record.** `.live/<instance>/run.json` holds the config, the absolute program path, its build
  (`--version`), the port, the scratch folder and the processes: the server's pid and group, and the runner's pid
  from `host-local/hostd.pid`. A restarted program still reaches, saves and stops the instance from it. `.live` is
  a dot folder, so `gc` and `shaked`'s sweep of save folders leave it alone.
- **stop** kills (SIGKILL) the recorded processes: the server's group, then the runner's group (the recorded pid,
  and the one `hostd.pid` names now, since the server starts its runner in a session of its own). Each process is
  killed only while it still runs the recorded program and has an argument that is the data folder or under it.
  Then the instance folder and the scratch folder are removed. Stopping an instance that does not run does nothing.

### Saves and restores

- **A save** is `$SHAKEDOWN_SAVES/sculptured/<save>/`. Tool calls on the instance wait while it is made. It holds
  `data/`, `changes.json` and `meta.json`. `data/` has an SQLite online backup of each store in the data folder
  (such as `controld.sqlite` and the runner's `host-local/hostd.sqlite`) and a folder copy of the rest. The copy
  leaves out the stores' `-wal`, `-shm` and `-journal` files and what only a running server has
  (`host-local/hostd.pid`, `hostd.lock`, `controld.url`, sockets). `changes.json` is the team's changes at the save,
  as the tools showed them. `meta.json` is `{build, aliases}`: the `--version` of the server that wrote the store,
  and the re-raised changes. A failed save leaves no folder.
- **A restore** (exact only) is refused with `-32001` when the save's `build` differs from the `--version` of the
  config's `binary`, or when more than 8 changes waited in it. Otherwise `data/` is copied into the new instance's
  data folder and the server starts as above. The seed is ignored.
- **Re-raised changes.** A restored server withdraws the questions its predecessor asked, so a change that waited
  at the save could not be decided any more. The start therefore proposes every such change again, oldest first,
  with the same action, summary, payload, system and reason, and records `original → copy` in
  `.live/<instance>/aliases.json`. Saves carry the map in `meta.json`, so restores of restores compose (the latest
  copy counts). `list_changes`, `inspect` and `diff` show the latest copy under the original id and hide every
  copy, and `decide_change` with the original id answers the copy's question. CopperTide's own UI and API show
  the copy under its new id, next to the withdrawn original, which waits forever.

### inspect, diff, usage and gc

- **inspect and diff.** Keys are `change/<id>`. A live instance's changes come from its server, a save's from
  `changes.json`. `diff` lists the changes whose document differs, decisions included.
- **usage** is 0: everything is under `SHAKEDOWN_SAVES` except the scratch folders, which `stop` removes.
- **gc** first stops, as `stop` does, every instance under `.live/` that this program did not start (left running
  by a program that ended without stopping it), then drops the saves `keep` does not keep.
- **fingerprint** is `{"integration": "sculptured"}`.

## How it's tested

```bash
cargo test -p shakedown-sculptured                                  # from the repository root
cargo test -p shakedown-sculptured -- --ignored live_               # runs real CopperTide servers
```

The first runs the unit tests (config and team names, the change documents, re-raised changes and their aliases,
saves with SQLite backups, decisions, stopping several instances) and three tests with stand-ins instead of
CopperTide: `gc` stopping a leaked instance and nothing else, a restarted program inspecting, saving and stopping a
running instance, and restores refused before anything runs (another build, too many waiting changes).

`live_sculptured_roundtrip` proposes, saves, decides, checks the refusals and the diff, restores, and decides a
re-raised change. It finds the server program in `SHAKEDOWN_COPPERTIDE_BINARY`, `SHAKEDOWN_SCULPTURED_BINARY`, or
on `PATH`. Without one it prints that it was skipped and passes. It ran once on 2026-11-07, by hand, against a local
build of the program (then named `sculptured`), and `shakedown-part-check` passed against the same build. No routine
test runs either.

The conformance check uses the examples in this package's manifest (a purchase-order proposal and a list). With
the server program on `PATH`:

```bash
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/sculptured
```

## Limits

- Needs a CopperTide build with the outside-proposer route (see What it needs). A save restores only with the build
  that made it.
- At most 8 changes may wait at once. A save with more cannot be restored.
- After a restore, CopperTide's own UI shows re-raised copies under new ids, beside withdrawn originals.
- The day-long decision timeout runs on the wall clock, not the world clock.
- Linux only, for stopping instances.
- No approximate restore.
