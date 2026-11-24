#!/bin/sh
# Run once while the template is built, as root: what a run needs that would otherwise come from
# the network. The packages' Python environments, the agent image, the world images (images.txt).
set -eu
set -a
. /etc/shaked/worker.env
set +a
here=/opt/shakedown/sandbox

for lock in /opt/shakedown/integrations/*/uv.lock /opt/shakedown/benchmarks/*/uv.lock; do
  (cd "$(dirname "$lock")" && UV_OFFLINE=0 uv sync --frozen --no-dev --quiet)
done

# The template's VM boots systemd, also while it is built: docker.service runs dockerd, as it
# will in each sandbox, so the images land where the sandbox's dockerd looks.
systemctl start docker
docker build -q -t "$SHAKED_AGENT_IMAGE" "$here/agent"
grep -v -e '^#' -e '^$' "$here/images.txt" | while read -r image; do docker pull -q "$image"; done
systemctl stop docker docker.socket
