# PostgreSQL (`postgres`)

Puts a PostgreSQL database in a Shakedown world: one database per instance, on a Postgres server you run. Use it
for any system whose state is a Postgres database, or as the store behind a benchmark's own tools. The `retail`
benchmark runs on it.

| | |
|---|---|
| Program | `shakedown-postgres` (Rust, crate `shakedown-postgres`) |
| Restore | exact, from a save (`CREATE DATABASE .. TEMPLATE`). No approximate restore. |
| World clock | not used |
| Agent tools | none by default. `--sql-tools` adds `sql_query` and `sql_execute`. |

## What it needs

- A Postgres server. `deploy/compose.yml` runs one (`postgres:17-alpine` on `127.0.0.1:5544`, login
  `shakedown:shakedown`).
- No Docker of its own and no GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_PG_URL` | `postgres://shakedown:shakedown@127.0.0.1:5544/postgres` | The server. `shaked` always passes its own. |
| `SHAKEDOWN_PG_NS` | empty | A namespace put after `pi_` and `ps_`, so tests never touch other databases. `?shakedown_ns=<ns>` on `SHAKEDOWN_PG_URL` does the same. |

## Use it

A benchmark or world names it as a part, for example `{part: "retail", integration: "postgres", config: {...}}`.

| Config key | Type | Meaning |
|---|---|---|
| `init_sql` | string | SQL run in a fresh, empty database. Ignored when the instance starts from a save. |

How it behaves:

- **Instances and saves.** Each instance is a database `pi_<instance>`. Each save is a copy, `ps_<save>`, made with
  `CREATE DATABASE .. TEMPLATE`. Postgres holds the saves itself, so the `dir` of `save` is ignored. `stop` drops
  the instance's database, so save first.
- **Tools.** `shakedown-postgres --sql-tools` lists and serves `sql_query` (read) and `sql_execute` (write).
- **inspect and diff.** `inspect` keys are `<table>/<primary key>`, for tables with a one-column primary key.
  `diff` compares every row, and a fingerprint of each table without such a key.
- **connect** returns `{url}`, which a benchmark's tools use to reach the instance.
- **usage and gc.** `usage` counts the sizes of the `pi_*` and `ps_*` databases. `gc {keep}` drops the saves not
  kept (an entry ending in `*` keeps every save id with that prefix) and the instances not running.
- **Older runs.** With no namespace, an id that names an existing `snap_*`, `branch_*`, `gold_*` or `world_base`
  database is used as it is. `usage` and `gc` also cover `snap_*` and `branch_*`, and `drop_save` drops a `snap_*`
  or `branch_*` database, never `gold_*` or `world_base`.

## How it's tested

```bash
docker compose -f deploy/compose.yml up -d          # the Postgres on 127.0.0.1:5544
cargo test -p shakedown-postgres                    # from the repository root
```

The tests need that Postgres server, or another one in `SHAKEDOWN_PG_URL`. They are not marked `live_`, because
Shakedown needs that server anyway. They cover save and restore, diff (tables with and without a primary key,
other schemas), typed parameters, `gc`, the older database names, and a `shakedown-part-check` conformance run of
`check/`, the package `postgres-check` (this program with `--sql-tools` and example calls):

```bash
echo '{"init_sql": "CREATE TABLE t(id int primary key, v text)"}' > /tmp/pg-check.json
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/postgres/check \
  --config /tmp/pg-check.json
```

## Limits

- One server per `SHAKEDOWN_PG_URL`. A save is a full copy of the database, so large databases make large saves.
- `CREATE DATABASE .. TEMPLATE` needs nobody connected to the source, so a save ends every other connection to the
  instance's database, including a benchmark's or an outside client's. Those clients must reconnect.
- No approximate restore and no use of the world clock.
