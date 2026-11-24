#!/usr/bin/env bash
# The template's live check, `live_e2b_template_boots_the_worker`: a sandbox made from it has
# dockerd answering, the agent and world images, `shaked worker` waiting for its ticket, and an
# agent network that reaches the agent port and nothing else. The sandbox is killed afterwards, also on failure (it costs a cent).
#   check-template.sh [template]   (default shakedown-worker-<rev>)
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
: "${E2B_API_KEY:?set E2B_API_KEY (the owner keeps it in ~/.bashrc)}"
template=${1:-shakedown-worker-$(git -C "$repo" rev-parse --short=12 HEAD)}

id=$(e2b sandbox create "$template" --detach --timeout 300)
[[ $id =~ ^[a-z0-9]+$ ]] || { echo "no sandbox: $id" >&2; exit 1; }
trap 'e2b sandbox kill "$id" >/dev/null' EXIT
# Each check runs as root with the worker's settings and prints what it saw.
# (`e2b sandbox exec` waits for its stdin to close.)
check() { e2b sandbox exec -u root "$id" "set -a; . /etc/shaked/worker.env; set +a; $1" </dev/null; }

check 'docker info --format "dockerd {{.ServerVersion}} answers"'
check 'docker image inspect -f "agent image {{.RepoTags}}" "$SHAKED_AGENT_IMAGE"'
check 'grep -v -e "^#" -e "^\$" /opt/shakedown/sandbox/images.txt |
  xargs docker image inspect -f "world image {{.RepoTags}}"'
check 'pgrep -f "shaked worker --link" >/dev/null && echo "shaked worker runs, link $SHAKED_LINK"'
check 'test -d /run/shaked && ! test -e /run/shaked/ticket && echo "it waits for /run/shaked/ticket"'
# The worker listens on the agent port only once it has its branch: stand-ins listen there and on
# the next port. A container as uid 1000 reaches the first, not the second nor the internet.
check 'host=${SHAKED_AGENT_LISTEN%:*} port=${SHAKED_AGENT_LISTEN##*:}
  for p in "$port" "$((port + 1))"; do python3 -m http.server "$p" --bind "$host" >/dev/null 2>&1 & done
  sleep 1
  docker run --rm --network "$SHAKED_AGENT_NETWORK" --user 1000:1000 "$SHAKED_AGENT_IMAGE" node -e "
    const get = (u) => fetch(u, {signal: AbortSignal.timeout(3000)}).then((r) => r.status, () => 0);
    Promise.all([\"http://$host:$port/\", \"http://$host:$((port + 1))/\", \"https://registry.npmjs.org/\"]
      .map(get)).then(([agent, other, out]) => {
        console.log(\"uid\", process.getuid(), \"agent port\", agent, \"other port\", other, \"internet\", out);
        process.exit(agent === 200 && other === 0 && out === 0 ? 0 : 1);
      });"'
echo "PASS: $template boots the worker (sandbox $id)"
