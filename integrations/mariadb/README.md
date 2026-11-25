# MariaDB (`mariadb`)

Puts a MariaDB database in a Shakedown world: one database per instance, on a MariaDB server in Docker that the
program starts and owns. Use it for any system whose state is a MySQL-compatible database. A save does not copy the
database: it records where the instance's changes sit in the server's binary log, so saving takes milliseconds,
and a restore rebuilds the database by replaying that log.

| | |
|---|---|
| Program | `shakedown-mariadb` (Rust, crate `shakedown-mariadb`) |
| Restore | exact, from a save (an optional dump plus binary-log segments, replayed). No approximate restore. |
| World clock | not used |
| Agent tools | none by default. `--sql-tools` adds `sql_query` and `sql_execute`. |

## What it needs

- Docker, and the image `mariadb:11.8` present locally: it is never pulled.
- No GPU and no server of your own. On the first `start` the program creates and starts the container
  `shakedown-mariadb` (volume `shakedown_mariadb_data`, on `127.0.0.1:3307`, root password `shakedown`) if it is not
  running. The server runs with `--log-bin=binlog --binlog-format=ROW --binlog-row-image=FULL --server-id=1
  --binlog_expire_logs_seconds=0 --max_binlog_size=64M`.
- Containers it makes carry the label `shakedown.part=mariadb`. A container of the configured name without that
  label is refused, so no other MariaDB is touched.

The settings below are config keys first. Each also has an environment variable, read only when the config does
not set the key. `shaked` clears a program's environment, so under `shaked` these variables have no effect: they
apply to tests and direct runs only. Set the config key instead.

| Variable | Config key | Default | Purpose |
|---|---|---|---|
| `SHAKEDOWN_MARIADB_CONTAINER` | `container` | `shakedown-mariadb` | The server's container. |
| `SHAKEDOWN_MARIADB_PORT` | `port` | `3307` | Its port on `127.0.0.1`. |
| `SHAKEDOWN_MARIADB_VOLUME` | `volume` | `shakedown_mariadb_data` | Its data volume. |
| `SHAKEDOWN_MARIADB_PASSWORD` | `root_password` | `shakedown` | Its root password. |
| `SHAKEDOWN_MARIADB_DUMP_AFTER_BYTES` | `dump_after_bytes` | `67108864` (64 MiB) | Binary-log bytes a restore may replay before a save writes a dump instead. |
| `SHAKEDOWN_SAVES` | none | `saves` | Where saves and dumps go. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "shop", integration: "mariadb", config: {init_sql: "..."}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `init_sql` | string | none | SQL run in a fresh, empty database, by the instance's own account. Ignored when the instance starts from a save. |
| `container`, `port`, `volume`, `root_password`, `dump_after_bytes` | | see above | Another server, or another dump threshold. Container and volume names match `[A-Za-z0-9][A-Za-z0-9_.-]{0,62}`. |

| Tool (`--sql-tools`) | Effect | Does |
|---|---|---|
| `sql_query {sql, params?}` | read | One `SELECT`, `WITH`, `SHOW`, `DESCRIBE` or `EXPLAIN`, in a `START TRANSACTION READ ONLY` that is rolled back. |
| `sql_execute {sql, params?}` | write | Any statements, or one prepared statement with `?` placeholders when `params` is given. `USE` is refused. |

Both run as the instance's account with its database as the default database. A call names the keys it touched:
`<table>/<pk>` for a single-table `INSERT`, `UPDATE`, `DELETE` or `SELECT` that pins a one-column primary key with
`=`, `<table>/*` for a `REPLACE` or an unpinned statement, and `*` for several statements, a multi-table `UPDATE`
or `DELETE`, or anything else.

### Instances and accounts

- Each instance is the database `mi_<instance>`, so instance ids are at most 61 characters.
- Each instance has its own account `mi_<instance>` with a random password, allowed only on its database:
  `SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, CREATE TEMPORARY TABLES, LOCK TABLES`. The tools,
  `init_sql` and `connect` use it, so they cannot turn binary logging off, purge the log or reach another database.
  Views, triggers, routines and events are not allowed: their `DEFINER` names an account that would not exist under
  a restored database's name. `stop` drops the database and the account.
- **connect** returns `{url}`:
  `mysql://mi_<instance>:<password, percent-encoded>@127.0.0.1:<port>/mi_<instance>`.
- **Server identity.** The first `start` writes a random id into `shakedown_meta.server`. Every save records the
  server's container, port and id. A save made on another container or port, or on a server whose id changed (a new
  volume behind the same name), is refused with `-32001` ("the MariaDB server was replaced since this save" for
  the second case).

### Saves and restores

- **A save** is `$SHAKEDOWN_SAVES/mariadb/<save>/manifest.json`, `{server, root, segments}`: the manifest the
  instance started from, plus one segment `{db: mi_<instance>, from: <start>, to: <now>}` for its own changes. A
  fresh instance's start position is read after its database and account exist and before `init_sql` runs, so
  `init_sql` is part of its changes. A restored instance's changes start after the replay.
- **A restore** (either mode, always exact) makes a new database, loads the root dump if there is one, then for
  each segment pipes `mariadb-binlog --database=<db> --rewrite-db=<db>-><target>` over the log files spanning it
  into `mariadb` inside the server container. The replay runs as a temporary account `rp_<target>`, limited to the
  target database plus `BINLOG REPLAY, BINLOG ADMIN`, with `sql_log_bin=0`. It does not grow the log, and DDL that
  names its database (`CREATE TABLE mi_a.x ..`) fails the restore instead of touching the source. A save whose log
  was purged is refused with `-32001`.
- **Dumps.** When a manifest's segments add up to more than `dump_after_bytes` of log, the save writes a
  `mariadb-dump --single-transaction --master-data=2` of the instance instead (a root and no segments), and the
  instance's changes start over from the position the dump notes. Dumps are stored once by content hash in
  `$SHAKEDOWN_SAVES/mariadb/.blobs/<sha256>.sql.gz`.
- **Exactness check.** `mariadb-binlog --database` picks events by the default database they ran under. So each
  save scans the instance's new log range (`SHOW BINLOG EVENTS`) for DDL that names `mi_<instance>` but ran under
  another default database or none, as a `connect` client can do. If it finds any, the save fails with `-32001`
  ("changes to <instance> were made while another database was selected, so this save can't be exact") and no
  manifest is written. The change stays in that instance's history, so every later save of it fails the same way.
  To recover, start a new instance from its last good save and redo the change through `sql_execute`, or with the
  instance's database as the default database. Comments do not hide a `USE` from `sql_execute` or `init_sql`: the
  check reads past `/* */`, `-- ` and `#` comments and into executable comments (`/*! .. */`).
- **Write DDL without the database name.** A segment holds only its own database's events, and a qualified name
  makes the save fail to restore.

### inspect, diff, usage and gc

- **inspect and diff.** `inspect` keys are `<table>/<primary key>`, for tables with a one-column primary key.
  Asking for a key of another table is `-32602`. `diff` compares rows by key, and a hash of each table without such
  a key (`<table>/*`). A save is read by rebuilding it into a scratch database `mx_<pid>_<n>`, dropped after.
- **usage** is the size of the log files plus the `mi_*` and `mx_*` databases. Saves and dumps are under
  `SHAKEDOWN_SAVES`, which `shaked` measures.
- **drop_save and gc {keep}** share one path: remove the dumps that no save or live instance needs, then
  `PURGE BINARY LOGS TO` the file holding the oldest position a remaining save or live instance needs. `gc` first
  drops the saves not kept, instance databases not running, scratch databases nobody reads, and their accounts. A
  manifest that cannot be read stops both with an error, rather than purging on a guess.
- **Re-rooting.** `shaked` keeps each benchmark's base and gold saves by pattern (`base_*`, `gold_*`), and those
  would hold the log from their positions for good. So `gc` rewrites the saves `keep` keeps by a pattern as full
  dumps once they alone hold more than 256 MiB of log that nothing else needs.
- **fingerprint** is `{server_version: "11.8"}`, the image's tag.

The server code is the kit's `shakedown_part::helpers::mariadb` (feature `mariadb`), which the
[`erpnext`](../erpnext/README.md) integration uses too, on a server of its own. The SQL tool schemas and argument
parsing are shared with [`postgres`](../postgres/README.md) in `shakedown_part::helpers::sql_tools`.

## How it's tested

```bash
cargo test -p shakedown-mariadb                                  # from the repository root, no Docker
cargo test -p shakedown-mariadb -- --ignored live_               # needs Docker and mariadb:11.8
```

The first runs the unit tests: settings, the keys read from statement text, read-only statements, and the `USE`
check through comments. The eight `live_` tests each make a server container and volume of their own, and
remove both when they end: a round trip, a restore of a restore, the dump threshold, re-rooting standing saves,
neighbouring databases and log rotation, accounts and server identity, the migration of an older identity table,
and a `shakedown-part-check` conformance run of `check/`, the package `mariadb-check` (this program with
`--sql-tools` and example calls). The earlier README recorded the seven other than the re-rooting test as run
against real MariaDB 11.8 servers in Docker. No date or Docker version was recorded.

To run the conformance check by hand against the default server, put the config in a file:

```bash
echo '{"init_sql": "CREATE TABLE t(id int primary key, v text)"}' > /tmp/mariadb-check.json
SHAKEDOWN_BIN_DIR=core/target/debug core/target/debug/shakedown-part-check integrations/mariadb/check \
  --config /tmp/mariadb-check.json
```

## Limits

- One `SHAKEDOWN_SAVES` per server. Saves under another root are not seen, and the log they need may be purged.
- After a restart, `usage` and `gc` reach a server they have not used yet with the default root password (or
  `SHAKEDOWN_MARIADB_PASSWORD`). A server set up with another `root_password` in the config is skipped by them
  until a `start` uses it again.
- No views, triggers, routines or events, and no DDL that names the database.
- A save that cannot be exact fails, and every later save of that instance fails too.
- No approximate restore and no use of the world clock.
