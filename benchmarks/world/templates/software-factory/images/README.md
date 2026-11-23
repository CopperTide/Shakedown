# Images

The software company's own Docker images: the base image every service and CI job runs on, the wheelhouse it
installs from, and the script that builds them. Every image the template runs, built here or pulled, is pinned once
in [`../images.yaml`](../images.yaml). Nothing is pulled at run time.

| Image | Built by | What it is |
|---|---|---|
| `shakedown-sf/base:1.0.0` | `build.sh base` | `python:3.12.16-slim` (pinned by digest), the wheelhouse in `/opt/wheels`, `platform-lib` installed, user `app` (uid 10001) |
| `shakedown-sf/{storefront,orders,inventory}:seed` | `build.sh apps` | each repository of `../repos/` built on the base image, as CI builds it |
| `shakedown-workbench:1` | `build.sh workbench` | the workbench of [`designs/software-factory`](../../../../../designs/software-factory/README.md), built on the base image so its agents' tests have the services' dependencies offline |

## What's here

- `base/`: the base image's `Dockerfile` and `platform_lib`, what the services share. It holds their settings from
  the environment, the world clock (read from `/shakedown/clock.json`), OTLP traces, metrics and logs, error reports
  to GlitchTip through the Sentry SDK with each event stamped in world time, the connection pool and schema,
  migrations, `/healthz` and `/version`, request and query metrics (their Prometheus names are in
  [`../platform/metrics.txt`](../platform/metrics.txt)), and the consumer-contract check.
- `wheelhouse/`: `requirements.txt` (pinned with hashes, compiled from `requirements.in` by
  `uv pip compile --python-version 3.12 --python-platform x86_64-manylinux_2_28 --exclude-newer 2026-11-01 --generate-hashes`)
  and the sources of the shop's internal `pricing-lib` 1.4.0 and 2.0.0 (2.0's `round_price` needs the currency).
  The base image downloads the pinned wheels and builds `pricing-lib` 1.4.0, 2.0.0 and `platform-lib` into
  `/opt/wheels`, so app builds and CI jobs install with `--no-index` and no network.
- `build.sh [base|apps|workbench|all]` (default `all`): builds the images under the names `images.yaml` gives.
  `base` and `workbench` need the network (pip downloads, and apt, pip and gh for the workbench). `apps` builds with
  `--network none`, as CI builds them.

## How it's tested

```bash
cd benchmarks/world/templates/software-factory/images
uv run --python 3.12 --no-project --with-requirements wheelhouse/requirements.txt --with-editable base pytest -q base
uv run --no-project --with pytest pytest -q wheelhouse/tests
cd ../../../ && uv run --with pytest pytest -q -m live tests/software_factory/test_live_app_images.py
```

- `base/tests`: `platform-lib`'s 17 tests. The migration test on a real Postgres runs only when `TEST_DATABASE_URL`
  names one.
- `wheelhouse/tests`: 3 tests of both `pricing-lib` releases.
- `test_live_app_images.py` (live, needs Docker): runs `build.sh all`, then each repository's CI test step in the
  base image under the runner's limits (no network, 384 MiB, this user, `HOME=/tmp`), and reads each app image's
  `/version` doc. Inventory's flaky test is an expected failure.

On 2026-11-19 (Linux x86_64, Docker 29.1.3), before the workbench was added to `all`: the base and the three app
images built (202 MB each, the layers shared), each repository's CI test step passed, and each app image's
`/version` named its tag and its dependency majors. The `platform-lib` tests ran with `TEST_DATABASE_URL` on a
Postgres 17 container.

## Limits

- The images were built and tested on Linux x86_64 only. `requirements.txt` was resolved for
  `x86_64-manylinux_2_28`. Other platforms were not tried.
- `build.sh base` and `build.sh workbench` need the network. Nothing else here does.
