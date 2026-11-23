# Claude Code agents in an NVIDIA OpenShell sandbox

[OpenShell](https://docs.nvidia.com/openshell/latest/) (Apache 2.0) runs a program in a sandbox whose network,
files and processes follow a YAML policy, and logs every denial. With this folder first in shaked's `PATH`,
every Claude Code agent shaked starts runs in a fresh sandbox under a policy. The sandbox's denials land on the
run's timeline as world events (part `sandbox`, kind `denial`), with their reasons.

| File | What it is |
|---|---|
| `claude` | Stands in for `claude`: makes a sandbox, runs the real Claude Code in it, posts its denials, deletes it. |
| `denials.py` | Turns `openshell logs` lines into events and posts them to `POST /api/branches/{id}/sandbox-events`. |
| `factory-01.yaml` | The policy for Factory-01: the agent may reach only Shakedown's MCP endpoint and Anthropic's API. |
| `Dockerfile` | The sandbox image: OpenShell's Ubuntu base and this machine's Claude Code binary. |

## What it needs

- The OpenShell release binaries (`openshell` and `openshell-gateway`) and Docker. No root.
- Claude Code installed and signed in on this machine.
- These settings for shaked: `OPENSHELL_HOME`, and optionally `SHAKEDOWN_OPENSHELL_POLICY` (default
  `factory-01.yaml` here) and `SHAKEDOWN_OPENSHELL_IMAGE` (default `openshell-shakedown-claude:latest`).

## Set up (Docker driver)

1. Download the release binaries `openshell-x86_64-unknown-linux-musl.tar.gz` and
   `openshell-gateway-x86_64-unknown-linux-gnu.tar.gz` from
   [github.com/NVIDIA/OpenShell](https://github.com/NVIDIA/OpenShell) into `$OPENSHELL_HOME/bin`.
2. Write the gateway config, `$OPENSHELL_HOME/config/gateway.toml`: `[openshell] version = 2`, and under
   `[openshell.gateway]` set `bind_address = "127.0.0.1:17670"` and `compute_driver = "docker"`.
3. With `XDG_STATE_HOME=$OPENSHELL_HOME/state` and `XDG_CONFIG_HOME=$OPENSHELL_HOME/cliconf`:
   - `openshell-gateway generate-certs --output-dir $XDG_STATE_HOME/openshell/tls --server-san host.openshell.internal`
   - start `openshell-gateway --config config/gateway.toml`, with `OPENSHELL_LOCAL_TLS_DIR` set to that folder
   - `openshell gateway add https://127.0.0.1:17670 --local --name shakedown`
4. Build the image from `Dockerfile` and tag it `openshell-shakedown-claude:latest`.
5. Start shaked with `PATH=<this folder>:$PATH OPENSHELL_HOME=...`.

## How it works

- The sandbox can't reach its own loopback, and a policy can never allow loopback. It reaches the host as
  `host.openshell.internal`. The wrapper rewrites `127.0.0.1:` in Claude Code's arguments (the MCP URL) to that.
- Claude Code signs in with a copy of this user's `~/.claude/.credentials.json`, uploaded into the sandbox and
  deleted with it.
- OpenShell inspects TLS (it sets `NODE_EXTRA_CA_CERTS` in the sandbox). Claude Code worked with it unchanged.
- Denials are read from `openshell logs <sandbox> --source sandbox` when the agent ends, before the sandbox is
  deleted (its logs go with it). They land on the timeline just before the agent's end, at that clock. Lines
  look like `NET:OPEN [MED] DENIED /usr/local/bin/claude(0) -> mcp-proxy.anthropic.com:443
  [reason:transparent_tcp_policy_denied]`. Each denied host becomes one event, with the count and the reasons.

## Verified

On 2026-11-08, with OpenShell v0.1.7 and Claude Code 2.1.319 on Ubuntu (kernel 6.17), following the steps above.
This folder has no automated tests.

## Limits

- Only Claude Code agents go through this wrapper. Codex and API-mode agents are not sandboxed by OpenShell.
- The real credential enters the sandbox. OpenShell's built-in `claude-code` provider takes API keys only. A
  provider holding a `CLAUDE_CODE_OAUTH_TOKEN` would keep the credential out of the sandbox. That has not been
  tried.
- Each agent start pays for a new sandbox, about 3 s on the machine above.
- Denials arrive when the agent ends, not as they happen.
