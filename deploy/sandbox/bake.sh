#!/bin/sh
# Run once while the template is built, as root: what a run needs that would otherwise come from
# the network. The packages' Python environments, the Postgres server packages use, the agent
# image, and every image a world starts (images.txt and the world templates' images.yaml).
# The template's VM boots systemd, also while it is built: docker.service and postgresql.service
# run as they will in each sandbox, so what is made here is where the sandbox looks.
set -eu
set -a
. /etc/shaked/worker.env
set +a
here=/opt/shakedown/sandbox
templates=/opt/shakedown/benchmarks/world/templates

sync_packages() {
  for lock in /opt/shakedown/integrations/*/uv.lock /opt/shakedown/benchmarks/*/uv.lock; do
    (cd "$(dirname "$lock")" && UV_OFFLINE=0 uv sync --frozen --no-dev --quiet)
  done
}

# Ubuntu's server, at the port and with the login SHAKEDOWN_PG_URL names (retail's parts use it).
postgres_server() {
  login=${SHAKEDOWN_PG_URL#postgres://}
  login=${login%%@*}
  port=${SHAKEDOWN_PG_URL#*@}
  port=${port%%/*}
  set -- $(pg_lsclusters -h | head -1)
  unit="postgresql@$1-$2"
  # The boot may be starting the cluster: stop it before changing its port. The cluster's own
  # unit, as `postgresql` returns before the server is up.
  systemctl stop "$unit"
  pg_conftool "$1" "$2" set port "${port##*:}"
  pg_conftool "$1" "$2" set listen_addresses localhost
  # Loopback only; the package's snakeoil certificate is not installed.
  pg_conftool "$1" "$2" set ssl off
  install -d -o postgres -g postgres -m 2775 /var/run/postgresql
  systemctl start "$unit" || { tail -20 "/var/log/postgresql/postgresql-$1-$2.log"; exit 1; }
  until pg_isready -q -p "${port##*:}"; do sleep 0.5; done
  cd /tmp && su postgres -c "psql -q -p ${port##*:} -c \"CREATE ROLE ${login%%:*} LOGIN SUPERUSER PASSWORD '${login#*:}'\""
  systemctl stop "$unit"
}

# The images a world template pulls: each images.yaml entry not marked `build: true`.
pulled_by_templates() {
  awk '$1 == "-" && $2 == "id:" { if (image && !built) print image; image = ""; built = 0 }
       $1 == "image:" { image = $2 } $1 == "build:" && $2 == "true" { built = 1 }
       END { if (image && !built) print image }' "$templates"/*/images.yaml
}

# The world-time gateway's image (software-factory), from its release build in bin/.
worldclock_gateway() {
  ctx=$(mktemp -d)
  cp /opt/shakedown/tools/worldclock-gateway/Dockerfile /opt/shakedown/bin/worldclock-gateway "$ctx/"
  image=$(awk '$1 == "image:" && $2 ~ /worldclock-gateway/ { print $2; exit }' "$templates"/*/images.yaml)
  docker pull -q debian:bookworm-slim
  docker build -q -t "$image" "$ctx"
  rm -rf "$ctx"
}

images() {
  systemctl start docker
  docker build -q -t "$SHAKED_AGENT_IMAGE" "$here/agent"
  { grep -v -e '^#' -e '^$' "$here/images.txt"; pulled_by_templates; } | sort -u |
    while read -r image; do docker pull -q "$image"; done
  for build in "$templates"/*/images/build.sh; do "$build" all; done
  worldclock_gateway
  systemctl stop docker docker.socket
}

sync_packages
postgres_server
images
