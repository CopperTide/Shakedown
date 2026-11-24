#!/bin/sh
# The template's start command (run as root, before the sandbox is handed out): dockerd, the
# sealed agent network, then `shaked worker` waiting for its ticket in /run/shaked/ticket.
# The template's ready command is `docker info`. The worker logs to /var/log/shaked-worker.log.
set -eu
set -a
. /etc/shaked/worker.env
set +a

# The VM boots systemd: docker.service runs dockerd (already up, or started here).
systemctl start docker
/opt/shakedown/sandbox/agent-network.sh
install -d -m 0700 /run/shaked "$SHAKEDOWN_WORLDS" "$SHAKEDOWN_SAVES"
exec /opt/shakedown/bin/shaked worker --link "$SHAKED_LINK" --ticket-file /run/shaked/ticket \
  >>/var/log/shaked-worker.log 2>&1
