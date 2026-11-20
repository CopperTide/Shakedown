# live-service integration

The `service` integration (`integrations/service/`, same crate, program `arena-live-service`) for a live platform on
the world clock: one Docker Compose project whose containers read the world's time, take deploys from a pipeline and
report what they watch as world events. Everything in `service`'s README holds here too (volumes, save strategies,
`live_copy`, `images`, `probes`, `http`). Its `describe` differs: `name: live-service`, `uses_clock: true`, and one
world tool, `service_deploy` (`effect: write`; a world's links call it, agents don't get it). A world with two
live-service parts has two tools of that name, which arenad refuses.

A live instance comes up with `docker compose up -d` and is up once every container is healthy (or running without
a health check) and every one-shot (a seed) has exited 0, at most `deploy_wait_s`; compose's own `--wait` is not used,
since it fails on a one-shot that finished. A start from a save waits the same way.

Config additions:

- `live: true`: the instance's clock folder (`$ARENA_SAVES/live-service/.clock/<instance>/`) is given to compose as
  `ARENA_CLOCK_DIR` (services mount it read-only). `clock.json` `{epoch_ms, clock_ms, world_ms, speed, wall_ms}` is
  written before the project comes up and replaced (written beside, then renamed) on each `advance`, which answers
  `changed: true`. `epoch_ms` and `speed` are the `world_time` of `start`; a world that gives none starts at the wall
  clock at speed 1. `stop` removes the folder.
- `service_deploy {service, image, sha?, ref?}` answers `{accepted: true}` at once and writes `image/<service>`. In
  the background the service is recreated with its new `IMAGE_<SERVICE>` (`docker compose up -d --no-deps
  <service>`) and watched with `docker compose ps -a` until it is `running healthy` (or `running` without a health
  check), `unhealthy` or `exited` fail it, at most `deploy_wait_s` (default 300 s, the sample app's health
  `start_period`). The next `advance` reports `deployed {by: "pipeline", result: {job_id: <ref as given>, service,
  image, sha, ok, error?}}`. One deploy runs at a time per instance. A failed deploy leaves the new image in place, as
  a real failed rollout does. A service the config's `images` doesn't name is the tool error `the project has no
  service <service>`. Deploys still running when the program restarts are not reported.
- `watches: [{probe, every_ms, items, id, on, off, fields?}]`: on an `advance` at or past its next time (the first
  advance, then every `every_ms` of world time), the probe `probe` is read; its items are at pointer `items` (`""`
  for the doc itself), each named by pointer `id`. An id not seen before gives an event `on`, a seen id that is gone
  an event `off`, each with `id` and the item's `fields` (pointers; `off` carries them as last seen). A probe that
  fails changes nothing. A save keeps what each watch saw (`watches.json`), so a start from it does not report again
  what was on at the save.

Verified:

- `tests/live_part.rs` on the fake `docker`: clock file, image variables, deploy and its failure, images saved and
  restored, probes and their saved docs, watches and a restored watch, a live copy that stops nothing, the start's
  wait (one-shots that finish well, one that fails).
- `tests/live_platform.rs` (`live_platform_deploys_a_new_image`, run once on 2026-11-19, Docker 29, Compose 5.0.2,
  86 s): the software factory's real platform (`benchmarks/world/templates/software-factory/platform/`, lean
  profile, its `service.yaml` read as is) with the world 10 minutes behind the wall: every container up, a new tag of
  the orders image deployed to `prod-orders` came up healthy, `deployed` gave `ok: true` and `job_id: "j1"`,
  `version/orders` answered the new sha; the shop's metrics (`timestamp(deploy_info)`), logs and traces read back
  through the world-time gateway were stamped in world time; a save (three `shop`/`glitchtip` dumps, three live-copied
  stores) kept the new image, and a second platform started from it answered the new sha. Not run: watches against
  real Alertmanager alerts, the traffic service's probes (Task 9's service is not in the compose file).
