#!/bin/sh
# The Docker network a sandbox's CLI agents run on (SHAKED_AGENT_NETWORK), sealed so that it
# reaches only the worker's agent port (SHAKED_AGENT_LISTEN: its MCP endpoint and gateway).
#   - `--internal`: nothing is routed off the network (no internet, no other Docker network).
#   - The SD-AGENTS chain: of what reaches this machine from the network, or for the network's
#     gateway address from anywhere else (world containers, local programs), only TCP from the
#     network to the agent port is accepted.
# Idempotent: run again, it leaves one network and one set of rules.
set -eu
: "${SHAKED_AGENT_NETWORK:?}" "${SHAKED_AGENT_LISTEN:?}" "${SHAKED_AGENT_SUBNET:?}"
bridge=${SHAKED_AGENT_BRIDGE:-sd-agents}
gateway=${SHAKED_AGENT_LISTEN%:*}
port=${SHAKED_AGENT_LISTEN##*:}

docker network inspect "$SHAKED_AGENT_NETWORK" >/dev/null 2>&1 ||
  docker network create --internal --subnet "$SHAKED_AGENT_SUBNET" --gateway "$gateway" \
    -o com.docker.network.bridge.name="$bridge" "$SHAKED_AGENT_NETWORK" >/dev/null

iptables -N SD-AGENTS 2>/dev/null || iptables -F SD-AGENTS
iptables -A SD-AGENTS -i "$bridge" -p tcp --dport "$port" -j ACCEPT
iptables -A SD-AGENTS -j DROP
for match in "-i $bridge" "-d $gateway"; do
  # shellcheck disable=SC2086 # each match is two words
  iptables -C INPUT $match -j SD-AGENTS 2>/dev/null || iptables -I INPUT $match -j SD-AGENTS
done
