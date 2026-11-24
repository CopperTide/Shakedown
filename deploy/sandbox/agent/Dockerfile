# The image a sandbox's CLI agents run in (SHAKED_AGENT_IMAGE): Claude Code and Codex, run as user
# `agent` (uid 1000). The worker starts one container of it per agent process, on the agent
# network, with only the agent's work folder and variables (see core/crates/shaked/src/agents/jail.rs).
# bake.sh builds it inside the template, so a run pulls nothing.
FROM node:22.23.5-bookworm-slim
ARG CLAUDE_CODE_VERSION=2.1.331
ARG CODEX_VERSION=0.170.0
RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" "@openai/codex@${CODEX_VERSION}" \
    && npm cache clean --force
# The node image's uid 1000 is `node`: it becomes `agent`.
RUN usermod -l agent -d /home/agent -m node && groupmod -n agent node
USER agent
WORKDIR /home/agent
