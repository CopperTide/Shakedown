# Live Docker Compose platform (`live-service`)

Puts a running platform of Docker Compose services in a Shakedown world, on the world clock. Its containers can
read the world's time, take new images from a pipeline's deploy step, and report what they watch, such as firing
alerts, as world events. The `software-factory` world template runs its platform on it. It is the
[`service`](../service/README.md) integration's program in its live mode: everything in that README holds here
too (volumes and how each is saved, `live_copy`, `images`, `probes`, `http`, saves at save points).

| | |
|---|---|
| Program | `shakedown-live-service` (Rust, crate `shakedown-service`, built from `integrations/service/`) |
| Restore | exact, from a save, as for `service`. Saves also keep the images, the probes' docs and what the watches saw. No approximate restore. |
| World clock | used: `clock.json` for the containers, deploys reported on `advance`, watches polled in world time |
| Agent tools | none. One world tool, `service_deploy`, which a world's links call. |

## What it needs

The same as `service`: Docker with Compose v2, the helper image present locally (default `redis:6.2-alpine`,
never pulled), and `SHAKEDOWN_SAVES` (always passed by `shaked`). This package's saves and records are under
`$SHAKEDOWN_SAVES/live-service/`. No GPU.

## Use it

A world names it as a part with the `service` config plus the keys below. A world with two `live-service` parts
has two tools named `service_deploy`, and `shaked` refuses a world whose tool names clash.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `live` | boolean | `false` | Write the world clock to `clock.json` in a folder compose gets as `SHAKEDOWN_CLOCK_DIR`. |
| `watches` | array | `[]` | `[{probe, every_ms, items, id, on, off, fields?}]`: probes polled in world time whose items appearing and going are world events. Each `probe` must be a probe `key` of the config. |
| `deploy_wait_s` | integer | `300` | How long the start and each deploy wait for the services to come up. |

How it behaves:

- **Starting.** The project comes up with `docker compose up -d`. It is up once every container is healthy (or
  running, without a health check) and every one-shot container, such as a seed, has exited 0, within
  `deploy_wait_s`. A container that exits non-zero or turns unhealthy fails the start. Compose's own `--wait` is not
  used, because it fails on a one-shot that finished. A start from a save waits the same way.
- **The clock file.** With `live: true`, the folder `$SHAKEDOWN_SAVES/live-service/.clock/<instance>/` is passed to
  compose as `SHAKEDOWN_CLOCK_DIR`, for the services to mount read-only. `clock.json` is
  `{epoch_ms, clock_ms, world_ms, speed, wall_ms, world}`. It is written before the project comes up and replaced
  whole (written beside, then renamed) on each `advance`, which then answers `changed: true`. `epoch_ms` and
  `speed` come from the start's `world_time`. A world that gives none (a base or gold world) starts at the wall
  clock at speed 1, with `world: false`. `stop` removes the folder. Probe paths use this clock's `world_ms` for
  `{world_ms}` and `{world_s}` (the wall clock without one).
- **`service_deploy {service, image, sha?, ref?}`** (effect `write`) answers `{accepted: true}` at once and writes
  the key `image/<service>`. In the background the service is recreated with its new `IMAGE_<SERVICE>`
  (`docker compose up -d --no-deps <service>`) and watched until it is running and healthy (or running, without a
  health check). `unhealthy`, `exited` or `dead` fail it, and so does waiting past `deploy_wait_s`. The next
  `advance` reports a `deployed` event with data
  `{by: "pipeline", result: {job_id: <ref>, service, image, sha, ok, error?}}`. One deploy runs at a time per
  instance. A failed deploy leaves the new image in place, as a real failed rollout does. A service that the
  config's `images` doesn't name is a tool error (`the project has no service <service>`).
- **Watches.** On an `advance` at or past a watch's next time (the first advance, then every `every_ms` of world
  time), its probe is read. The items are at JSON pointer `items` (`""` for the doc itself), each named by pointer
  `id`. An id not seen before gives an event of kind `on`, and a seen id that is gone gives one of kind `off`. Each
  carries `id` and the item's `fields` (pointers). `off` carries them as last seen. A probe that fails changes
  nothing.
- **Saves.** Beside what `service` keeps, a save keeps the image each service runs (`save.json`) and what each
  watch saw (`watches.json`). A start from the save runs the saved images and does not report again what was on
  at the save.

## How it's tested

```bash
cargo test -p shakedown-service --test live_part                                 # from the repository root, no Docker
cargo test -p shakedown-service --test live_platform -- --ignored live_          # needs Docker, the platform's images, python3 with PyYAML
```

`tests/live_part.rs` runs on a fake `docker` program. It covers the description, the plain package refusing
`live` and `watches`, the clock file, image variables, a deploy and a failed one, a deploy to an unknown service,
images saved and restored, probes and their saved docs, watches and a restored watch, a live copy that stops
nothing, and the start's wait (one-shots that finish well, and one that fails).

`live_platform_deploys_a_new_image` in `tests/live_platform.rs` starts the `software-factory` template's real
platform ([`benchmarks/world/templates/software-factory/platform/`](../../benchmarks/world/templates/software-factory/platform/),
lean profile, its `service.yaml` read as is) with the world 10 minutes behind the wall clock. It needs the images
of the template's `images.yaml`, pulled or built with the template's `images/build.sh` and
`tools/worldclock-gateway/build-image.sh`. It ran once on 2026-11-19 (Docker 29, Compose 5.0.2, 86 s): every
container came up, a new tag of the orders image deployed to `prod-orders` came up healthy, `deployed` gave
`ok: true` and `job_id: "j1"`, and `version/orders` answered the new sha. The shop's metrics, logs and traces,
read back through the world-time gateway, were stamped in world time. A save kept the new image, and a second
platform started from it answered the new sha.

## Limits

- Never run against real services: watches on real Alertmanager alerts, and the probes of the platform's
  `traffic` service.
- Deploys still running when the program restarts are not reported.
- One `live-service` part per world, because of the `service_deploy` tool name.
- Everything in `service`'s limits applies.
