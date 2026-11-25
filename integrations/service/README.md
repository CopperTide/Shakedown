# Docker Compose service (`service`)

Puts any Docker Compose project in a Shakedown world: one project per instance, with the volumes you list held in
each save. Use it for a system that has no integration of its own: an HTTP API, a cache, a small stack of
services. The same crate also builds [`live-service`](../live-service/README.md), which adds the world clock,
deploys and watches.

| | |
|---|---|
| Program | `shakedown-service` (Rust, crate `shakedown-service`) |
| Restore | exact, from a save (copies of the listed volumes, each saved its own way). No approximate restore. |
| World clock | not used. A config with `live` or `watches` is refused: those need `live-service`. |
| Agent tools | none. Agents reach the services over HTTP through `shaked`'s proxy when the config lists `http` hosts. |

## What it needs

- Docker with Compose v2 (`docker compose`) on the machine that runs the program.
- A helper image with `tar`, `sh`, `cp`, `find`, `stat` and `awk`, present locally: it is never pulled. The default
  is `redis:6.2-alpine`. The images of your compose file are handled by `docker compose` as usual.
- No GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (relative to the working folder) | Where saves and the program's records go. `shaked` always passes its own. |

The `docker` commands inherit the program's environment, plus the config's `env`. Under `shaked` that environment
is cleared to `PATH`, `HOME` and a few Shakedown variables ([`docs/packages.md`](../../docs/packages.md)), so a variable such as `DOCKER_HOST` must go in the
config's `env`. `COMPOSE_PROJECT_NAME` is never passed on.

## Use it

A world names it as a part, for example
`{part: "cache", integration: "service", config: {compose_file: "compose.yml", volumes: ["data"]}}`. A relative
`compose_file` is read from the folder the program runs in, which under `shaked` is the package folder.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `compose_file` | string | required | The Docker Compose file. It must exist. |
| `volumes` | array | required | The compose file's volumes a save holds: a name (saved as `tar`) or `{name, save, ...}` (below). Names are letters, digits, `_` and `-`. |
| `helper_image` | string | `redis:6.2-alpine` | The image that copies volumes, archives and hard-linked trees. Never pulled. |
| `env` | object | `{}` | Extra environment for `docker compose`. |
| `images` | object | `{}` | `{<service>: <image>}`, passed to compose as `IMAGE_<SERVICE>` (upper case, `-` as `_`). Kept in saves. |
| `http` | array | `[]` | `[{host, service, port, env?, reads?}]`: hosts the instance serves through `shaked`'s proxy. |
| `probes` | array | `[]` | `[{key, service, port, path, headers?}]`: docs read over HTTP from the services. Kept in saves. |

Each instance is the Compose project `shakedown_<instance>`, run as
`docker compose -p shakedown_<instance> -f <compose_file>`. A fresh start runs `docker compose up -d --wait`.

### How a volume is saved

A volume given as an object is `{name, save?, services?, service?, database?, user?, live_copy?}`. `save` is one
of:

| `save` | What a save holds | What stops |
|---|---|---|
| `tar` (default) | `<volume>.tar`, streamed by a throwaway helper container (`--network none`). Numeric owners, modes and mtimes are kept. Every save holds full archives. | the `services` listed, or the whole project when any stopping volume lists none |
| `postgres` | `<volume>.sql`, from `pg_dump -U <user> --no-owner --no-privileges <database>` run with `docker compose exec -T <service>` | nothing |
| `hardlink` | the tree `<volume>/`, copied by the helper and hard-linked to the instance's previous save where a file is unchanged | as for `tar` |
| `skip` | nothing. A start from the save creates the volume empty (a cache). | nothing |

- **`postgres`** needs `service`, the compose service running the database. `database` and `user` default to
  `postgres`. A start from the save brings up the database services, waits up to 60 s for `pg_isready`, loads the
  dump with `psql -v ON_ERROR_STOP=1`, then brings up the rest. The dump's digest leaves out `--` comment lines and
  `pg_dump`'s random `\restrict` and `\unrestrict` lines, so two dumps of the same data compare equal.
- **`hardlink`** links against the instance's previous save, or the save it started from. That previous tree is
  hard-linked whole (`cp -al`). Then every entry whose type, size, mtime (seconds), mode or owner differs, or that
  was modified at or after the second the previous save was written, is copied afresh. Without a previous save the
  volume is copied whole. The trees belong to the container's users, so they are restored, archived for digests
  and removed through the helper too.
- **`live_copy: true`** on a `tar` or `hardlink` volume copies it while everything runs, for a store whose own
  recovery reads a crash-consistent copy. A save whose file volumes are all live copies stops nothing.

Each save is `$SHAKEDOWN_SAVES/service/<save>/`, with `save.json`: the volumes and how each was saved, the helper
image, the images each service ran, and the `PATH` and `DOCKER_*` entries of the config's `env`. The helper needs
those to read or remove the trees once the instance is gone. No other variable from `env` is kept. Older saves
without `save.json` hold `.tar` archives only.

### How it behaves

- **Saves.** `describe.save_at` is `points`: `shaked` saves this part at save points (before a model call, on
  request, at the end of a run), not after every step. A save dumps the databases first, then stops what the file
  volumes need, copies them and starts it again. A failed save leaves no folder.
- **Starts from a save.** A new project is created with its volumes filled from the save (archives extracted as
  root, so owners survive), then started. A save missing one of the config's volumes is refused. The save's images
  replace the config's.
- **Records.** Each running instance's config is kept in `.live/<instance>.json` and its last save in
  `.last/<instance>.json`, under the package's saves folder. A restarted program can still save, inspect and stop
  the instance.
- **stop** runs `docker compose down -v`. The containers and volumes are gone, so save first.
- **inspect.** `volume/<name>` is `{bytes, files, hash}` (none for a `skip` volume). `image/<service>` is
  `{image}`. Each probe's `key` is the JSON body of `GET path` on the port Docker published for `service` (2 s
  timeout, with `headers`), or `{error}` when the service can't be reached, answers other than 2xx, or answers no
  JSON. `{world_ms}` and `{world_s}` in `path` are filled in with the world's time, which is the wall clock in this
  package. A save keeps its probes' docs in `probes.json`, and `inspect` of the save reads them. Volume digests are
  made only when the key list is empty or names a `volume/` key. On a live instance they come from an archive
  taken without stopping anything.
- **diff** compares volume digests only: each archive's entries (name, owner, mode, file mtime, content, with
  folder and symlink mtimes left out), each dump, each tree.
- **http_routes** answers each `http` host with the port Docker published for its service
  (`docker compose port`), once that port accepts connections (at most 60 s). A save that stops a service restarts
  it on a new port. `shaked` asks for the routes again after each save.
- **usage and gc.** `usage` is 0: saves are under `SHAKEDOWN_SAVES`, and `stop` removes live volumes. `gc` drops
  nothing in the program. `drop_save` removes a save's trees through the helper.

## How it's tested

```bash
cargo test -p shakedown-service                                   # from the repository root, no Docker
cargo test -p shakedown-service --test live_service -- --ignored live_   # needs Docker, redis:6.2-alpine, postgres:17-alpine
```

The first runs config checks and the part's description without Docker, and runs every save strategy against a
fake `docker` program put first on `PATH`. The fake records each command, so the tests check the exact commands:
plain names saved as before, a database dumped without stopping anything and restored before the rest starts,
skipped volumes, hard-linked trees and their removal, stopping only the listed services, HTTP routes before and
after a save, no compose secret in `save.json`, failures that leave no partial save, and a restarted program.

The two `live_` tests in `tests/live_service.rs` run real Docker:

- `live_service_roundtrip` (one Redis 6.2 project): save, change, start a second instance from the save, the saved
  key and file owner kept, save and stop through a restarted program. It ran once against Docker.
- `live_service_strategies` (a Postgres database, a hard-linked volume and a cache): an unchanged file is one inode
  in two saves, the live instance's digests equal its last save's, and an instance started from the first save has
  its row and file and an empty cache. It passed on Docker 29.1 with `postgres:17-alpine` and `redis:6.2-alpine`.
  `SHAKEDOWN_TEST_POSTGRES_IMAGE` picks another database image.

There is no `shakedown-part-check` run and no `check/` folder: the part has no tools, and its config needs a
compose file.

## Limits

- `tar` saves hold full archives every time. Only `hardlink` saves share unchanged files.
- `diff` sees volumes only. A change of image or of a probe's answer is not a difference.
- A `tar` or `hardlink` save stops services unless every file volume is a `live_copy`, and a stopped service may
  come back on a new port.
- Not verified: how `shaked`'s sweep of save folders after a `gc` handles hard-linked trees owned by container
  users. `drop_save` removes them through the helper.
- No approximate restore, no agent tools and no world clock. For those, see `live-service`.
