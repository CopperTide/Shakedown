# service integration

Puts any Docker Compose project in the world: one project per instance, with the volumes you list. It is the fallback
for a system that has no save code of its own. Restore: exact from a save (stop, copy the volumes, start); approximate:
no. `uses_clock`: false. Verified live: `live_service_roundtrip` ran once against a real Redis 6.2 project in Docker
(save, change, restore, file owners kept, save and stop through a restarted program). It has no `arena-part-check` run:
`check/` is empty, because the part has no tools and needs a compose file in its config.

Each instance is a Docker Compose project `arena_<instance>`, run with
`docker compose -p arena_<instance> -f <compose_file>` (`COMPOSE_PROJECT_NAME` is never passed on).
Config: `{compose_file, volumes: [name], helper_image?, env?}`; `volumes` are the compose file's volumes
that a save holds (letters, digits, `_`, `-`).

- Save = stop the project, write each volume to `$ARENA_SAVES/service/<save>/<volume>.tar`, start it again.
  The archive is exactly what a throwaway container of `helper_image` (default `redis:6.2-alpine`, never
  pulled; any image with `tar` works) streams: owners (numeric), modes and mtimes are kept. No hard links:
  every save holds full archives.
- Start from a save = a new project whose volumes are created, filled from the archives (extracted as root, so
  owners survive), and then started.
- Each running instance's config is kept in `$ARENA_SAVES/service/.live/<instance>.json` (removed by `stop`), so a
  restarted program can still save, inspect and stop it.
- `stop` runs `docker compose down -v`: the instance and its volumes are gone, so save first.
- No tools. `inspect` key `volume/<name>` gives `{bytes, files, hash}`; `diff` compares per-volume hashes of the
  archive's entries (name, owner, mode, mtime, content; folder mtimes ignored). For a live instance both read the
  volume's files on disk (an archive made without stopping it).
- `usage` is 0: saves are under `ARENA_SAVES`, and live volumes are not counted (`stop` removes them).
- Live test: `cargo test -p arena-service -- --ignored live_` (needs Docker and `redis:6.2-alpine`).

## Save strategies, save points and hosts (designed; fake-Docker tested only)

A volume may be given as `{name, save?, services?, service?, database?, user?}` instead of a plain name (a plain name
is `{name, save: "tar"}` and saves exactly as above). `save` is one of:

- `tar` (default): the archive above. With `services`, only those compose services stop while it is copied (when
  every copied volume names some; otherwise the whole project stops, as before).
- `postgres`: `docker compose exec -T <service> pg_dump -U <user> --no-owner <database>` into `<volume>.sql` while
  the database runs (nothing stops). `service` is required; `database` and `user` default to `postgres`. A start from
  the save brings up the database services, waits for `pg_isready` (60 s), loads the dump with `psql`, then brings up
  the rest. Its digest is the SHA-256 of the dump's lines that are no `--` comment.
- `hardlink`: `rsync -a --delete --numeric-ids` of the volume into the tree `<save>/<volume>/`, with `--link-dest` to
  the instance's previous save (or the save it started from), so unchanged files are hard links and cost nothing.
  It needs `rsync_image`: an image with `rsync`, `tar`, `cp` and `rm`, **pinned by digest** (`name:tag@sha256:…`) in
  the config (never named in code), pulled the first time a save needs it. The trees belong to the container's
  users, so they are restored (`cp -a`), digested (`tar -c`) and removed (`rm -rf`) through that image too.
- `skip`: not saved; a start from a save creates it empty (a cache).

Each save has `save.json` (`{volumes: [{name, save}], rsync_image?, compose_file?, env?}`). The last save of each
instance is kept in `.last/<instance>.json`. `describe.save_at` is `points`: arenad saves the part at save points
(before a model call, on request, at run end), not after every request. `http: [{host, service, port, env?,
reads?}]` makes `http_routes` answer each host with the port Docker published for `service` (`docker compose port`),
once that port accepts connections (at most 60 s), so agents reach it through arenad's proxy. A save that stops a
service restarts it on a new port; arenad asks `http_routes` again after each save. The live test `live_service_strategies` (Postgres, a hard-linked volume
and a cache; `ARENA_TEST_RSYNC_IMAGE` names the rsync image) is written but has not been run.
