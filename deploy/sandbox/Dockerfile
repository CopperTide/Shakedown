# The E2B template a hosted branch runs in: `shakedown-worker-<shaked rev>`.
# Built by build-template.sh from a staged context (never from this folder directly):
#   bin/        shaked and the package programs, release builds for Linux x86_64
#   shakedown/  the packages (integrations/, benchmarks/) and kits/python
#   sandbox/    this folder: start.sh, bake.sh, agent-network.sh, worker.env, images.txt, agent/
# The sandbox holds no key: its only credential is the branch's worker token, from its ticket.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils git iptables python3 docker.io docker-buildx docker-compose-v2 \
    && rm -rf /var/lib/apt/lists/*

ARG NODE_VERSION=22.23.5
RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
    | tar -xJ -C /usr/local --strip-components=1 --exclude CHANGELOG.md --exclude README.md --exclude LICENSE
ARG UV_VERSION=0.10.9
RUN curl -LsSf "https://astral.sh/uv/${UV_VERSION}/install.sh" \
    | env UV_INSTALL_DIR=/usr/local/bin UV_NO_MODIFY_PATH=1 sh

COPY bin/ /opt/shakedown/bin/
COPY shakedown/ /opt/shakedown/
COPY sandbox/ /opt/shakedown/sandbox/
RUN install -D -m 0644 /opt/shakedown/sandbox/worker.env /etc/shaked/worker.env \
    && chmod 0755 /opt/shakedown/bin/* /opt/shakedown/sandbox/*.sh \
    && /opt/shakedown/sandbox/bake.sh

# The start command, the ready command (`docker info`) and the sandbox's commands run as root.
USER root
WORKDIR /root
