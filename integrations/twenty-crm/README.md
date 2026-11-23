# Twenty CRM (`twenty-crm`)

Puts a [Twenty](https://twenty.com) CRM in a Shakedown world: one Twenty server per instance, each with its own
Redis and its own database on one Postgres server, all in Docker and run by the program. Twenty is an open-source
CRM. Here it stands in for the commercial CRM a customer runs, so agents find accounts, contacts and opportunities,
log notes, move deals and create tasks. The `factory` and `pharma` benchmarks use it.

| | |
|---|---|
| Program | `shakedown-twenty-crm` (Rust, crate `shakedown-twenty-crm`) |
| Stands in for | Salesforce (as `describe.system.stands_in_for` says) |
| Restore | exact, from a save (`CREATE DATABASE .. TEMPLATE`, the kit's Postgres helper). No approximate restore. |
| World clock | not used |
| Agent tools | `crm_find_account`, `crm_find_contact`, `crm_find_opportunities`, `crm_log_note`, `crm_update_opportunity`, `crm_create_task` |

## What it needs

- Docker with Compose v2, and these images present locally (none is pulled): `twentycrm/twenty:v2.51.0`,
  `postgres:17-alpine` and `redis:6.2-alpine`.
- No GPU. An earlier run recorded about 1 GB of memory per Twenty server.

What the program runs, on the first start (or the first call that needs it):

- **The database server.** Compose project `shakedown_twenty` from [`compose.yml`](compose.yml): `postgres:17-alpine`
  as container `shakedown-twenty-db`, volume `shakedown-twenty-db`, on `127.0.0.1:5546`, network `shakedown-twenty`,
  login `shakedown:shakedown`. It stays up.
- **One server per instance.** Compose project `shakedown_twenty_<instance>` from [`instance.yml`](instance.yml): the
  Twenty server on a free port on `127.0.0.1`, and a Redis with no persistence. There is no worker service and no
  cron registration.

The program reads three variables, else the defaults. The config of the first start may override the first two.
`shaked` clears a program's environment, so under `shaked` these variables have no effect unless the package's run
command sets them (for example with `SHAKEDOWN_RUN_TWENTY_CRM`). They apply to tests and direct runs.

| Variable | Config key | Default | Purpose |
|---|---|---|---|
| `SHAKEDOWN_TWENTY_PROJECT` | `project` | `shakedown_twenty` | The database server's compose project (`[a-z0-9_]+`), and the prefix of the instances' projects. Docker names use it with `-` for `_`. |
| `SHAKEDOWN_TWENTY_DB_PORT` | `db_port` | `5546` | The database server's port on `127.0.0.1`. |
| `SHAKEDOWN_TWENTY_IMAGE` | none | `twentycrm/twenty:v2.51.0` | The Twenty server's image. |
| `SHAKEDOWN_SAVES` | none | `saves` | Where the base's sign-in and the instance records go. `shaked` always passes its own. |

`compose.yml` also reads `SHAKEDOWN_TWENTY_PG_IMAGE` (default `postgres:17-alpine`). The program does not set it,
so it takes effect only from the program's own environment. The compose files are read from this folder, at a path
fixed when the program is built.

## Use it

A world names it as a part, for example `{part: "crm", integration: "twenty-crm", config: {}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `workspace_name` | string | the base's, `Shakedown` | The workspace's display name (the app shows it top left), set in each new instance's database before its server starts. An instance started from a save that has it is unchanged. The `factory` benchmark gives it its company's name. |
| `project`, `db_port` | | see above | The database server. Read by the first start only: a later start whose config names another server is `-32602`. |

The tools call Twenty's REST API on the instance's port with the base's API key. An account is Twenty's `company`.

| Tool | Effect | Arguments |
|---|---|---|
| `crm_find_account` | read | `name?`. Each account comes with its contacts and opportunities. |
| `crm_find_contact` | read | `name?`, `email?`, `account_id?`. A full name must match first and last name, one word either. |
| `crm_find_opportunities` | read | `name?`, `account_id?` |
| `crm_log_note` | write | `account_id`, `title`, `body` (Markdown). Writes `note/<id>` and `noteTarget/<id>`. |
| `crm_update_opportunity` | write | `id`, `stage?` (`NEW`, `SCREENING`, `MEETING`, `PROPOSAL`, `CUSTOMER`), `close_date?` (`YYYY-MM-DD`). One of the two is needed. |
| `crm_create_task` | write | `title`, `body?`, `due?` (`YYYY-MM-DD`), `account_id?` |

The three finds take `limit` (default 60) and `cursor`, and answer a page: the records as Twenty's REST API gives
them (depth 1), with `id`, `name`, `account_id`, `close_date`, `amount` and the like set over them, plus `total`
and, when there are more, `next_cursor`. Name and email filters match any part, ignoring case. A note or task on an
account that does not exist is refused before anything is written. Reads name `<table>/<id>` keys. Twenty refusing
a call, and bad arguments, are the tool's error. Only Twenty not answering fails the call itself.

### The base and the instances

- **The base** is made once and kept as the database `tb_base`, with its sign-in in
  `$SHAKEDOWN_SAVES/twenty-crm/.base/api.json`. A Twenty server on an empty database runs the image's migrations,
  the app's sign-up makes the workspace and an API key with the Admin role, the server is removed, and the demo
  records a new workspace comes with are emptied. The sign-in file is written last, and a base without one is made
  again.
- **An instance** `<id>` (at most 60 characters) is the database `ti_<id>`, copied from the base or a save, and its
  server, which starts without migrations and must answer `/healthz` within 180 s. Every copy has the same workspace
  and API key. `.live/<id>.json` records its port and server, so a restarted program can still use, save and stop
  it. Starting an id that runs is `-32602`. `stop` removes the project and the database.
- **State changes only through tool calls.** There is no worker and no cron, so nothing writes in the background.
  What a worker would do is not done: timeline activity rows and webhooks.
- **connect** returns `{url, api_key, admin_email, admin_password}`, for a benchmark's seed and for signing in to the
  app.

### Saves and restores

- **A save** is the database `ts_<save>`, a copy of the instance's database. Postgres cannot copy a database with
  connections open, so the server's connections are closed for the copy, and its pools open new ones on the next
  request. Postgres holds the saves itself, so the `dir` of `save` is ignored, as in
  [`postgres`](../postgres/README.md). The server's Redis holds only caches and is not saved.
- **A restore** copies `ts_<save>` into a new `ti_<id>`. Only exact restores: an approximate one is refused with
  `-32001`, and so is a start from a save that does not exist.

### inspect, diff, usage and gc

- **inspect and diff** read the database directly, so a save needs no server. Keys are `<table>/<id>` for
  `company`, `person`, `opportunity`, `note`, `noteTarget`, `task` and `taskTarget`. Each document is the row, plus
  `accounts` (the account names) on a note or task and `account` on an opportunity or person. Other tables are
  copied by a save but not compared. A key that is not there is null.
- **usage** is the size of the `ti_*` and `ts_*` databases, and 0 before this program has needed the server.
- **gc {keep}** drops the saves `keep` does not name (an entry ending in `*` keeps every id with that prefix) and
  the databases of instances not running. It never drops the base. `drop_save` of a save that does not exist is
  `-32002`.

## How it's tested

```bash
cargo test -p shakedown-twenty-crm                                  # from the repository root, no Docker
cargo test -p shakedown-twenty-crm -- --ignored live_ --nocapture   # needs Docker and the three images
```

The unit tests cover the settings, ids, the instance records a restarted program reads, `workspace_name`, the
tool schemas, filters, paging and the fields agents see, refusals and Twenty not answering, and the documents of
notes and opportunities.

`live_twenty_roundtrip` runs a database server and Twenty servers of its own (project `shakedown_twenty_t<pid>`)
and removes everything when it ends. It checks that the base is empty, then finds, writes, inspects, saves,
changes, restores, works through a restarted program, runs `gc`, and runs `shakedown-part-check` on `check/`, the
package `twenty-crm-check` (this program, whose examples create a task and run the three finds), with a config file
that points it at the test's server. The earlier README records one run against the real image in Docker on Linux,
with every check step passing (replay skipped, since the part restores from saves). The test took about 100 s, of
which about 60 s was the first start making the base. A restore took about 9 s, and a save about 0.1 s and 20 MB of
database. No date or Docker version was recorded.

## Limits

- Not run: Twenty's web UI against a restored instance, and more than two instances at once.
- No worker: no timeline activity rows and no webhooks.
- One database server per program.
- Only the seven tables above are compared. Changes elsewhere are saved but not seen by `diff`.
- No approximate restore and no use of the world clock.
