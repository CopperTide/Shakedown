# ERPNext (`erpnext`)

Puts an ERPNext site in a Shakedown world: one site per instance, on one ERPNext install in Docker that the program
starts and owns. ERPNext is an open-source ERP. Here it stands in for the commercial ERP a customer runs, so agents
work with items, suppliers, customers, orders and journal entries through ERPNext's own REST API. The `factory` and
`pharma` benchmarks use it. A site's state is its database plus its files, so a save is the
[`mariadb`](../mariadb/README.md) integration's kind of save (a position in the binary log) plus a copy of the
site's files.

| | |
|---|---|
| Program | `shakedown-erpnext` (Rust, crate `shakedown-erpnext`) |
| Stands in for | SAP S/4HANA, Oracle (as `describe.system.stands_in_for` says) |
| Restore | exact, from a save (the database rebuilt from a dump and the binary log, plus the site's files). No approximate restore. |
| World clock | not used |
| Agent tools | `get_doc`, `list_docs`, `insert_doc`, `update_doc`, `submit_doc` |

## What it needs

- Docker with Compose v2, and these images present locally (none is pulled): `frappe/erpnext:v16.53.0`,
  `redis:6.2-alpine` and `mariadb:11.8`.
- The program must run as uid 1000, the uid the ERPNext image runs as. The sites folder it bind-mounts into the
  containers must belong to that uid, and `start` fails saying so otherwise.
- No GPU. The first start builds the base site once. An earlier run measured about 90 s for the base, plus the
  time the stack takes to come up.

What the program runs, on the first `start` (or the first call that needs it):

- **The install.** Compose project `shakedown_erpnext` from [`compose.yml`](compose.yml): `backend`, one `queue`
  worker for the `long`, `default` and `short` queues, `websocket`, `frontend` on `127.0.0.1:8180`, and its own
  `redis-cache` and `redis-queue`. There is no `db` service, no `scheduler` and no `configurator`.
- **Its database server.** A MariaDB server made by the kit's mariadb helper, separate from the `mariadb`
  integration's: container `shakedown-erpnext-db`, volume `shakedown_erpnext_db`, `127.0.0.1:3308`, root password
  `shakedown`, with Frappe's flags (`--character-set-server=utf8mb4 --collation-server=utf8mb4_unicode_ci
  --skip-character-set-client-handshake`) after the binary-log flags. It carries the label `shakedown.part=mariadb`
  (the pre-rename `arena.part=mariadb` label is also accepted; foreign containers are refused) and joins the project's network as `db`. A server of its own
  means neither integration's log purges remove log the other needs.

The install's settings are read from the environment, else the defaults above. The config of the first start may
override most of them. `shaked` clears a program's environment, so under `shaked` these variables have no effect
unless the package's run command sets them, for example with `SHAKEDOWN_RUN_ERPNEXT` (which `shaked` runs with
`/bin/sh -c`). They apply to tests and direct runs.

| Variable | Config key | Default | Purpose |
|---|---|---|---|
| `SHAKEDOWN_ERPNEXT_DB_CONTAINER` | `db_container` | `shakedown-erpnext-db` | The database server's container. |
| `SHAKEDOWN_ERPNEXT_DB_PORT` | `db_port` | `3308` | Its port on `127.0.0.1`. |
| `SHAKEDOWN_ERPNEXT_DB_VOLUME` | `db_volume` | `shakedown_erpnext_db` | Its data volume. |
| `SHAKEDOWN_ERPNEXT_DB_PASSWORD` | `db_root_password` | `shakedown` | Its root password. |
| `SHAKEDOWN_ERPNEXT_PROJECT` | `project` | `shakedown_erpnext` | The compose project (`[a-z0-9_]+`). |
| `SHAKEDOWN_ERPNEXT_PORT` | `http_port` | `8180` | The frontend's port on `127.0.0.1`. |
| `SHAKEDOWN_ERPNEXT_IMAGE` | none | `frappe/erpnext:v16.53.0` | The ERPNext image. |
| `SHAKEDOWN_ERPNEXT_COMPOSE` | none | this folder's `compose.yml`, as a path fixed when the program is built | The compose file. |
| `SHAKEDOWN_SAVES` | none | `saves` | Where saves, the base and the sites folder go. `shaked` always passes its own. |

For an existing Arena-era install, explicitly retain its container, volume, compose project, ports and
password using the overrides above. The product rename does not rename Docker resources. Before using old
saves with the new helper, preserve the server UUID from `arena_meta.server` in `shakedown_meta.server`;
back up the old metadata first, and do not replace an existing, different UUID. Saved binlogs identify the
server by that UUID. The local airline upgrade retained the original table and copied its schema and row
into the previously absent new metadata table.

The program passes `SHAKEDOWN_ERPNEXT_SITES`, `SHAKEDOWN_ERPNEXT_PORT` and `SHAKEDOWN_ERPNEXT_IMAGE` to
`docker compose` itself, so the compose file gets them under `shaked` too.

## Use it

A world names it as a part, for example `{part: "erp", integration: "erpnext", config: {}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `diff_doctypes` | array of strings | `Item, Supplier, Customer, Purchase Order, Sales Order, Work Order, Journal Entry` | The doctypes `diff`, and `inspect` with no keys, read. |
| `dump_after_bytes` | integer | `67108864` (64 MiB) | Binary-log bytes a restore may replay before a save writes a dump instead, as in `mariadb`. |
| `db_container`, `db_port`, `db_volume`, `db_root_password`, `project`, `http_port` | | see above | The install. Read by the first start only: one install per program, and a later start whose config names another is `-32602`. |

The tools send Frappe's REST API requests to `127.0.0.1:<http_port>` with the Administrator's API key and the
header `Host: <site>`. Keys are `<Doctype>/<name>`.

| Tool | Effect | Arguments |
|---|---|---|
| `get_doc` | read | `doctype`, `name` |
| `list_docs` | read | `doctype`, `filters?`, `fields?` (`name` is always added), `limit?` (default 20, 0 for all), `start?` (skip that many, for the next page), `order_by?` (Frappe's, like `"modified desc"`) |
| `insert_doc` | write | `doc`, which names its `doctype` |
| `update_doc` | write | `doctype`, `name`, `fields` |
| `submit_doc` | write | `doctype`, `name`. Sets `docstatus` to 1, which Frappe runs as a submit. |

Frappe refusing a call (a missing document, a failed validation) is the tool's error, with Frappe's message, and so
are bad arguments. Only ERPNext not answering (connection refused, HTTP 502 to 504) fails the call itself.

### The base and the instances

- **The base** is made once, on the first start, and kept in `$SHAKEDOWN_SAVES/erpnext/.base/`. The helper makes the
  database `erpnext_base` and its account. `bench new-site shakedown-base.localhost --no-setup-db --install-app
  erpnext` installs into them, `generate_keys` makes an API key for the Administrator, and the setup wizard runs
  (company `Shakedown Co`, abbreviation `AC`, USD, United States, UTC, the standard chart of accounts, fiscal year
  2026). After its background jobs finish, the base is saved: a dump of the database, the site's files, its config
  without the database keys (`site_config.json`, which keeps the `encryption_key` the stored API secret needs) and
  the key and Administrator password (`api.json`). The manifest is written last, and a base without one is made
  again. Then the base site, its database and account are removed and the log is purged. The base has no log
  segments, so it restores on any server. Each time the install comes up, the base is labelled with the running
  server's identity, and saves chained from it name that server.
- **Grants.** The base install and every site run with the helper's grants on their own database only
  (`SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, CREATE TEMPORARY TABLES, LOCK TABLES`). That was
  enough for `bench new-site` with ERPNext, which creates no views, triggers or routines.
- **An instance** `<instance>` (at most 61 characters) is the site `<instance>.localhost` on the database
  `eb_<instance>`, restored from the base's or a save's manifest as `mariadb` restores, by a replay account limited
  to it. It gets its own account `eb_<instance>` with a random password. Its folder is made from the save's files
  plus `logs/`, `locks/` and a `site_config.json`: the base's shared config, its own database and account, and
  `pause_scheduler: 1`. Its Redis cache keys (`eb_<instance>|*`) are deleted, and the start waits up to 90 s until
  the site answers an authenticated request. Frappe keeps a site's config in each worker for up to 60 s, so a site
  made again under a recent name may take that long. A starting site's database is marked busy so `gc` leaves it
  alone, and starting an id that runs or is starting is `-32602`. `stop` removes the folder, account and database.
- **The sites folder** is `$SHAKEDOWN_SAVES/erpnext/.sites`, bind-mounted into the containers. It is a dot folder,
  so `shaked`'s sweep of save folders skips it. The program writes its `common_site_config.json` (the database at
  `db:3306`, the two Redis servers) and `apps.txt`.
- **State changes only through tool calls.** There is no scheduler service, and every site has `pause_scheduler`.
  Jobs that tool calls enqueue run on the worker.
- **connect** returns `{url, site, api_key, api_secret, admin_password}`, with `url` `http://127.0.0.1:<http_port>`.
  Send `Host: <site>`, or log in to the web pages as Administrator.

### Saves and restores

- **A save** is `$SHAKEDOWN_SAVES/erpnext/<save>/`. First the site's background jobs are drained: Frappe's `RQ Job`
  list for the site, queued or started, polled every 250 ms. After 60 s the save fails with `-32001` ("background
  jobs still running for <site>"). Then come `files/public` and `files/private` (copies of `public/files` and
  `private/files`, hard-linked to the previous save's copy where unchanged), `site.json` (the instance's
  `diff_doctypes`) and last `manifest.json`: the instance's base plus its own log segment, or a dump when a
  restore would replay more than `dump_after_bytes`. This is the `mariadb` integration's rule, through the same
  kit code. Saves are exact. A failed save leaves no folder.
- **A restore** (`from`, either mode) is a new instance from the save's manifest and files, as above. A save made on
  another server, or on one replaced since, is `-32001`.

### inspect, diff, usage and gc

- **inspect and diff** read through the REST API: an instance's site in place, and a save through a scratch site
  `view-<pid>-<n>.localhost` on the database `ev_<pid>_<n>`, made from it and removed after. Building a scratch
  site takes tens of seconds, so `diff` reads both sides at once, and the documents and differing keys it read from
  saves are kept in memory (up to 16 saves) for the `inspect` and `diff` that follow. `inspect` with no keys gives
  every document of the target's `diff_doctypes`. `diff` lists every field of every document of both targets'
  `diff_doctypes` and compares them by key. A target diffed with itself is not read. In `inspect`, a key that
  Frappe answers 404 for, or of a doctype it does not have, is null. Other refusals are errors.
- **drop_save and gc {keep}** work as in `mariadb`: remove the dumps that no save, live instance or the base needs,
  and purge the log before the oldest position still needed. When nothing needs any position and no site is being
  made, the log is rotated first, so all of it goes. `gc` also drops the saves not kept, and the databases,
  accounts and folders of sites that are neither live nor being read.
- **Re-rooting.** As in `mariadb`, `gc` rewrites the saves `keep` keeps by a pattern (`base_*`, `gold_*`) as full
  dumps once they alone hold more than 256 MiB of log. Their files stay as they are. It shares the kit's code with
  `mariadb` and has not been run live on this integration.
- **usage** is this server's log files plus the `eb_*` and `ev_*` databases, and 0 when the server is not running.
  The `mariadb` integration does not count this server, so nothing is counted twice. The sites folder, saves,
  dumps and the base are under `SHAKEDOWN_SAVES`, which `shaked` measures. The server's own data volume and the
  Redis volumes are not counted.
- **fingerprint** is `{image, frappe, erpnext}`: the image's tag and the app versions `bench version` prints, run
  once per program in a throwaway container (`--network none`, never pulled). So `describe` needs Docker and the
  image.

## How it's tested

```bash
cargo test -p shakedown-erpnext                                  # from the repository root, no Docker
cargo test -p shakedown-erpnext -- --ignored live_ --nocapture   # needs Docker, the images above, uid 1000
```

The unit tests cover the settings and their checks, the tool schemas and queries, Frappe's answers and error text,
site and key names, the site configs, the sites folder (sites made from saved files, files saved back with
unchanged ones linked), the document cache, the base's relabelling, and which site folders `gc` may remove.

`live_erpnext_roundtrip` runs a stack of its own (project `shakedown_erpnext_t<pid>` on a free port) and a
database server of its own, and removes them, the saves and the sites folder when it ends. It inserts, saves,
changes, restores and diffs. It checks reads and tool errors, `diff` and `inspect` of saves, and a restore through
a restarted program. Then it runs `shakedown-part-check` on `check/`, the package `erpnext-check` (this program,
whose examples insert a ToDo and an Item and read them back), with a config file that points it at the test's
stack.
The earlier README records this test as run against the real install in Docker. No date or Docker version was
recorded.

## Limits

- One install per program, and one program per `SHAKEDOWN_SAVES`.
- The default compose file path is fixed when the program is built, so the binary expects the repository it was
  built from, unless `SHAKEDOWN_ERPNEXT_COMPOSE` says otherwise.
- During the base install, the temporary account's password and the Administrator password are on the
  `bench new-site` command line, so they show in the host's process list for the minute the install takes.
- Reading a save for `inspect` or `diff` builds a scratch site: tens of seconds each time a save is read first.
- Re-rooting has not run live on this integration. The scheduler never runs, so anything ERPNext does only on a
  schedule does not happen.
- No approximate restore and no use of the world clock.
