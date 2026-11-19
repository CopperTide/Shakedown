# Images

- `base/`: `shakedown-sf/base:1.0.0`, the image every service and CI job runs on: `python:3.12.16-slim` (pinned
  in images.yaml), the wheelhouse in `/opt/wheels` and `platform-lib` installed, user `app` (uid 10001).
  `platform_lib/` is what the services share: settings, the world clock (`/shakedown/clock.json`), OTLP traces,
  metrics and logs, Sentry with world-time `before_send`, the connection pool and schema, migrations, `/healthz`
  and `/version`, request and query metrics (their Prometheus names are `platform/metrics.txt`), and the
  consumer-contract check.
- `wheelhouse/`: `requirements.txt` (pinned with hashes, compiled from `requirements.in` by `uv pip compile
  --python-version 3.12 --python-platform x86_64-manylinux_2_28 --exclude-newer 2026-11-01 --generate-hashes`)
  and the internal `pricing-lib` 1.4.0 and 2.0.0 sources (2.0's `round_price` needs the currency). The base
  image downloads the pinned wheels and builds the three local packages into `/opt/wheels`, so app builds and
  CI jobs install with `--no-index` and no network.
- `build.sh [base|apps|all]`: builds the base (network: pip downloads) and each repository's image under the names
  images.yaml gives (no network, as CI builds them).

## Verified (2026-11-19, Linux x86_64, Docker 29.1.3)

- `uv run --python 3.12 --no-project --with-requirements wheelhouse/requirements.txt --with-editable base pytest -q`
  in `base/`: platform-lib's tests (17, with `TEST_DATABASE_URL` on a Postgres 17 container); `wheelhouse/tests`: 3.
- `pytest -m live tests/software_factory/test_live_app_images.py` (from `benchmarks/world`): `build.sh all` built
  the four images (202 MB each, the layers shared), each repository's CI test step passed in the base image
  under the runner's limits (no network, 384 MiB, this user, `HOME=/tmp`), and each app image's `/version` doc
  named its tag and dependency majors.
