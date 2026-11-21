# The workbench: where the designs' agents run their commands. Each command runs in a fresh
# container with --network none; the only way out is the recording proxy's socket, which
# bridge-then relays to 127.0.0.1:3128 (the agents' HTTPS_PROXY in the container).
# BASE: an image with the company's application test dependencies, so run_tests works offline
# (see README, "The workbench image").
ARG BASE=python:3.12-slim
FROM ${BASE}

ARG GH_VERSION=2.45.0
# sha256 of gh_2.45.0_linux_amd64.deb, from the release's gh_2.45.0_checksums.txt.
ARG GH_SHA256=3e5410ca55f5a56c88516f53cd4cc9dc1bb106288be2b9ac2d5d79f6e20915a4

USER root
RUN apt-get update \
 && apt-get install -y --no-install-recommends git curl socat ca-certificates \
 && curl -fsSL -o /tmp/gh.deb \
      "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.deb" \
 && echo "${GH_SHA256}  /tmp/gh.deb" | sha256sum -c - \
 && dpkg -i /tmp/gh.deb \
 && rm -f /tmp/gh.deb \
 && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir \
      pytest==9.2.0 PyGithub==2.10.0 jira==3.10.5 slack_sdk==3.47.0 pdpyras==5.4.1

COPY bridge-then.sh /usr/local/bin/bridge-then
RUN chmod 0755 /usr/local/bin/bridge-then \
 && (getent passwd 1000 >/dev/null || useradd -u 1000 -M -d /tmp agent)

# The root is read-only and /tmp a fresh tmpfs per command: HOME there keeps gh's and git's
# config and caches from one command only (gh's 24 h cache never outlives a command).
ENV HOME=/tmp \
    GH_NO_UPDATE_NOTIFIER=1 GH_NO_EXTENSION_UPDATE_NOTIFIER=1 GH_TELEMETRY=0 DO_NOT_TRACK=1 \
    PYTHONDONTWRITEBYTECODE=1 PIP_NO_INPUT=1
USER 1000:1000
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/bridge-then"]
