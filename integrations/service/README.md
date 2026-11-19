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
- `hardlink`: the volume copied into the tree `<save>/<volume>/` by the helper image (the one that archives every
  volume, default `redis:6.2-alpine`, never pulled): the instance's previous save (or the save it started from) is
  hard-linked whole (`cp -al`), and every entry whose type, size, mtime (seconds), mode or owner differs, or that was
  modified at or after the second that save was written, is copied afresh; so unchanged files are hard links and cost
  nothing. The trees belong to the container's users, so they are restored (`cp -a`), archived for digests (the same
  `tar -c` as a live volume) and removed (`rm -rf`) through that image too.
- `skip`: not saved; a start from a save creates it empty (a cache).

Each save has `save.json` (`{volumes: [{name, save}], helper_image?, env?}`; `env` keeps only `PATH` and `DOCKER_*` of the config's
`env`, what the helper's `docker` needs once the instance is gone — never the compose file's other variables). The last save of each
instance is kept in `.last/<instance>.json`. `describe.save_at` is `points`: arenad saves the part at save points
(before a model call, on request, at run end), not after every request. `http: [{host, service, port, env?,
reads?}]` makes `http_routes` answer each host with the port Docker published for `service` (`docker compose port`),
once that port accepts connections (at most 60 s), so agents reach it through arenad's proxy. A save that stops a
service restarts it on a new port; arenad asks `http_routes` again after each save. The live test `live_service_strategies` (Postgres, a hard-linked volume
and a cache) passed on Docker 29.1 with `postgres:17-alpine` and `redis:6.2-alpine`: an unchanged file is one inode
in two saves, the live instance's digests equal its last save's, and a start from the first save has its row and file.

## Images, probes and live copies (fake-Docker tested)

- `images: {<compose service>: <image>}` reaches compose as `IMAGE_<SERVICE>` (upper case, `-` as `_`). The running
  instance's record keeps the images it runs now and each save's `save.json` keeps them as `images`; a start from a
  save runs the save's images. `inspect` key `image/<service>` gives `{image}`.
- `probes: [{key, service, port, path, headers?}]` are docs read over HTTP: `inspect` of a live instance GETs `path` on
  the port Docker published for `service` (2 s timeout, with `headers`); `{world_ms}` and `{world_s}` in the path are
  the world's time (the clock file's, see `live-service`; the wall clock without one). The doc is the JSON body, or
  `{error}` when the service can't be reached, answers other than 2xx, or answers no JSON. A save keeps its probes'
  docs in `probes.json`, which `inspect` of the save reads. Volume digests are made only when every key, or a
  `volume/` key, is asked for.
- A `tar` or `hardlink` volume with `live_copy: true` is copied while everything runs (a store whose own recovery reads
  a crash-consistent copy): a save whose file volumes are all live copies stops nothing.

The same program is also the `live-service` package (`integrations/live-service/`), which adds the world clock,
`service_deploy` and watches. This package refuses `live` and `watches`.
