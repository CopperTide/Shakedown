# The image every service and CI job of the shop runs on: Python, the pinned wheelhouse (in /opt/wheels, so
# builds and CI jobs install offline) with both pricing-lib releases, and platform-lib installed.
# Built by images/build.sh with images/ as the context.
FROM python:3.12.16-slim@sha256:5e2bdb1b0d542d5039cffb8181c65930093949194a65c365dfb1b0fe13a4fa5b
ENV PIP_DISABLE_PIP_VERSION_CHECK=1 PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
COPY wheelhouse/ /tmp/wheelhouse/
COPY base/ /tmp/platform-lib/
RUN pip download --no-cache-dir --only-binary=:all: --require-hashes -d /opt/wheels -r /tmp/wheelhouse/requirements.txt \
 && for package in /tmp/wheelhouse/pricing-lib-1.4.0 /tmp/wheelhouse/pricing-lib-2.0.0 /tmp/platform-lib; do \
        pip wheel --no-cache-dir --no-deps -w /opt/wheels "$package" || exit 1; done \
 && pip install --no-cache-dir --no-index --find-links /opt/wheels -r /tmp/wheelhouse/requirements.txt \
 && pip install --no-cache-dir --no-index --find-links /opt/wheels --no-deps platform-lib==1.0.0 \
 && useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin app \
 && rm -rf /tmp/wheelhouse /tmp/platform-lib
