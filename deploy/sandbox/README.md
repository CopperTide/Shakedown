# The sandbox template

Hosted Shakedown runs each branch in its own [E2B](https://e2b.dev) sandbox. This folder builds the
E2B template those sandboxes start from, `shakedown-worker-<shaked rev>`. How the worker and the
control box talk is in [docs/internals/sandbox-runs.md](../../docs/internals/sandbox-runs.md).

## What a sandbox holds

- Ubuntu 24.04 with Docker Engine, Buildx and Compose.
- `shaked` and the package programs (release builds) in `/opt/shakedown/bin`, and the packages
  (`integrations/`, `benchmarks/`, `kits/python`) in `/opt/shakedown`. Each Python package's
  environment is synced at build time, so `uv` runs offline.
- Python 3.12 with `uv`, and Node 22.
- The agent image (`agent/Dockerfile`), with Claude Code and Codex, and the world images listed in
  `images.txt`. All are in the template, so a run pulls nothing from a registry.

It holds no key. The sandbox's only credential is its branch's worker token, which the worker gets
for the one-use ticket that hosted `shaked` writes to `/run/shaked/ticket`. The control box adds the
run's model key to each model call.

## How it starts

`start.sh` is the template's start command, run as root while E2B builds the template. E2B snapshots
the running sandbox, so each sandbox starts with all of this already running:

1. It makes sure `docker.service` runs (the VM boots systemd). The ready command is `docker info`.
2. `agent-network.sh` creates the agent network and seals it.
3. It starts `shaked worker --link "$SHAKED_LINK" --ticket-file /run/shaked/ticket`, which waits for
   its ticket. `SHAKED_LINK` is baked in at build time.

The settings are in `worker.env`, copied to `/etc/shaked/worker.env`. Logs go to
`/var/log/dockerd.log` and `/var/log/shaked-worker.log`.

## CLI agents run in containers

In a sandbox, each Claude Code or Codex process runs in a throwaway container of the agent image
(`core/crates/shaked/src/agents/jail.rs`):

```
docker run --rm -i --name shaked-agent-<id> --user 1000:1000 --network shaked-agents \
  --cap-drop ALL --security-opt no-new-privileges -v <workdir>:<workdir> -w <workdir> \
  -e ANTHROPIC_BASE_URL -e ANTHROPIC_API_KEY … shakedown-agent:<rev> claude -p …
```

- It runs as `agent` (uid 1000) with no capabilities. It sees only its own work folder.
- It gets only the variables its command sets: its routes to the worker and its agent token. They
  are passed by name, so no token shows in argv.
- The container is removed when its process ends, or when the worker kills it (time limit, session
  closed).

The `shaked-agents` network (`10.250.0.0/24`) is `--internal`: nothing on it is routed to the
internet or to another Docker network. The `SD-AGENTS` iptables chain lets the network reach only
the worker's agent port, `10.250.0.1:4610`, where the worker serves its MCP endpoint and gateway.
Nothing else, on the network or off it, reaches that address. The worker reads the jail from
`SHAKED_AGENT_NETWORK`, `SHAKED_AGENT_IMAGE` and `SHAKED_AGENT_LISTEN`. With none of them set (a
worker outside a sandbox), CLI agents run as plain processes.

The world's own programs run on the sandbox as root, beside `shaked worker`. The sandbox's egress
allowlist (set when hosted `shaked` creates the sandbox) applies to all of it.

## Build it

You need the E2B CLI (`npm i -g @e2b/cli`), `E2B_API_KEY`, and release builds of `shaked` and the
package programs for Linux x86_64, glibc 2.39 or older (Ubuntu 24.04):

```bash
(cd core && cargo build --release -p shaked) && cargo build --release
deploy/sandbox/build-template.sh
deploy/sandbox/check-template.sh
```

`build-template.sh` stages a build context (the binaries, the packages without their tests and
virtual environments, and this folder) and runs `e2b template create`. It ends by listing the
template and its id. It takes these variables:

| Variable | Default | Meaning |
|---|---|---|
| `SHAKED_BIN_DIR` | `core/target/release` | Where the release builds are. |
| `SHAKED_LINK` | `wss://sd.coppertide.ai/worker` | The control box's `/worker`. A test template names a tunnel. |
| `TEMPLATE` | `shakedown-worker-<rev>` | The template's name. `<rev>` is the commit, `-dirty` with uncommitted changes. |
| `CPU_COUNT`, `MEMORY_MB` | `2`, `2048` | Each sandbox's size. |

The build takes about two and a half minutes. Unchanged layers come from E2B's cache.

`check-template.sh [template]` is the template's live check. It makes a sandbox from the template
and checks that dockerd answers, that the agent and world images are there, that the worker is
waiting for its ticket, and that a container on the agent network, as uid 1000, reaches the agent
port and nothing else. Then it kills the sandbox, also on failure.

## Change it

- **A world needs an image:** add it to `images.txt`.
- **A newer Claude Code or Codex:** change the versions in `agent/Dockerfile`.
- **Another setting for the worker:** add it to `worker.env`. Never add a key: no provider, vault,
  E2B or R2 key may enter a sandbox.
