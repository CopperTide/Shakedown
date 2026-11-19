# Software factory: refactor, production-grade worlds, world builder, and the end-to-end demo

Approved by the user on 2026-11-19 ("gO!"), after the 2026-11-18/19 conversation. Branch `factory`.

## Intent

Show a buyer building an AI software factory the whole loop in one sitting: from an empty Shakedown, build a
realistic copy of a software company's production (code host, issue tracker, CI, services, databases, the full
observability stack, chat and paging, simulated people), run two multi-agent designs on the same work, and compare
them on what reached production, what broke, what it cost, and why, down to the model's reasoning at any step.

The user's words: "from empty setup to full production comparisons of two agent designs for dealing with AI
Software Factories", "it is important that it seems realistic", "including setting up all the tooling and
inspecting the reasoning", and "use the runpod to run a custom model so we can inspect reasoning traces after the
fact". Before any of it: "refactor the code deeply so its very high quality, modular, DRY, similar to the top high
quality rust and react systems".

Customer framing ([[demo-customer-first]]): the buyer's bottleneck is review. Agent teams open more pull requests
than humans can check, and passing unit tests miss side effects in other services. The demo argues from that:
Shakedown scores the end state of production (error rate, incidents, broken contracts), not the diff.

## Goals

1. **Refactor** (behaviour-preserving) Rust core, React UI, kits/integrations and SDK to a top-tier standard, per
   the three audits of 2026-11-19 (recorded in the refactor plan). Includes one real fix: the model gateway must
   check the run's bearer token as the MCP endpoint does.
2. **Production-grade worlds**: three platform changes that make a full stack practical inside a world.
   - *Recording proxy*: agents' own clients (`git`, `gh`, `curl`, SDKs) reach world services through a proxy that
     records each request as a step (inspectable, forkable). Planned already as part 3 of the any-agent project;
     built here first for HTTP.
   - *Cheaper stack saves*: save at fork points and run end, not after every write; database volumes saved by
     template/dump, file volumes by hard-link copy, stateless containers skipped.
   - *World time for telemetry*: simulated traffic and twins stamp metrics, logs and traces with world time
     (Prometheus remote-write/backfill, Loki push, OTLP), so dashboards follow the world clock and pauses.
3. **World builder** in the UI: pick an industry template, name the company, set its size, choose systems, add
   simulated people and events, start. The world is ready in under a minute. Templates are data, not code.
4. **Software factory template and integrations**:
   - GitHub-compatible API twin over real git repos (REST subset agents and `gh` use: repos, branches, contents,
     pulls, reviews, checks, issues, webhooks) — so customer agents work unchanged.
   - Jira-compatible REST twin (projects, issues, transitions, comments, sprints, JQL subset).
   - Real CI runner executing the repo's tests; real staging/production services (a small multi-service app with
     its own Postgres) deployed by the pipeline.
   - Real observability stack: OpenTelemetry collector, Prometheus, Grafana (provisioned dashboards), Loki, Tempo,
     GlitchTip (Sentry-compatible).
   - Slack- and PagerDuty-compatible twins.
   - Simulated people engine (developers, reviewer, on-call, product manager) acting on the world clock, seeded,
     configurable in the builder ("fake users").
   - Synthetic user traffic against production so metrics and errors are real.
5. **Two agent designs**, real models only ([[no-demo-agents-real-llm-only]]), run through part 1's drivers:
   - A: hierarchical team (planner, coder, reviewer, release) in LangGraph.
   - B: single coding agent with a test loop in the OpenAI Agents SDK.
   Same model for both, so the comparison isolates design.
6. **Custom model on RunPod**: an open-weights reasoning model (default gpt-oss-120b on one H100 via vLLM with its
   reasoning parser) behind Shakedown's gateway, so reasoning is recorded per step and inspected after the run.
   Pod started only when needed, stopped when idle; hourly price told to the user before start.
7. **The demo**: recorded runs, a ~5 minute walkthrough (build world → world live → events → designs run →
   comparison → reasoning), and a video in the style of the airline and factory videos.

## Non-goals

- Bit-identical replay of real services (timestamps differ); scoring uses end-state checks.
- Running the customer's real SaaS accounts; every outside system is a labelled twin.
- Other industries now. Aviation follows on the same builder and engines.
- The per-install launch token (I8) — still the user's open question.

## Anti-goals

- Staged or scripted results; a demo where agents look good because the world is easy.
- Hardcoded domain data in code or tests ([[no-hardcoded-domain-data]]): company, services, backlog, people and
  events live in template data files.
- A refactor that changes behaviour or look. Tests stay green at every commit.
- CI/packaging fluff in the pitch ([[feedback-no-ci-packaging-fluff]]).

## Constraints

- Shared machine: `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, vitest `--maxWorkers=1`, ~4 GB free RAM, ~37 GB
  free disk. At most one cargo build at a time. One full world instance runs locally; comparisons at scale run on
  RunPod.
- `main` merges ship; merge only when the user asks. Push over SSH.
- clippy `-D warnings`, functions ≤ 30 lines.

## Approaches considered

- **Code host / tracker**: (a) open-source stand-ins (Forgejo, Plane) — real software, but customer agents built on
  the GitHub/Jira APIs would need changes; (b) API-compatible twins over real git — **chosen**, so agents work
  unchanged; (c) real GitHub/Jira sandboxes — not sealed, not forkable, rejected.
- **World creation**: (a) live builder in the product — **chosen** (user picked A); (b) guided tour of prebuilt
  worlds — cheaper but shows our work, not the customer's.
- **Telemetry time**: (a) libfaketime on real services — brittle across images; (b) world-time-stamped telemetry
  pushed by twins and traffic — **chosen**.
- **Refactor execution**: three tracks (Rust core, UI, kits/integrations) by fresh implementers in separate
  worktrees, Rust tracks serialized for memory, each step behaviour-preserving and test-gated, then one
  whole-branch review.

## Order

1. Refactor (plan: `docs/superpowers/plans/2026-11-19-refactor.md`).
2. Platform: recording proxy, cheaper saves, world time for telemetry.
3. World builder + template format.
4. Software factory integrations and template.
5. RunPod model + two designs.
6. Recorded demo and video.

Each stage gets its own plan when the previous one lands.

## Calls made after the builder check (author's calls, correctable)

- **Scoring** uses part 4 of the any-agent project (expected end state: deterministic checks over start and end
  `inspect` state, confirmed once). The template's data seeds the tasks *and* the traps that make the world hard:
  a change that passes its own tests but breaks another service's contract, a flaky test, a dependency upgrade
  with a breaking API, a migration that locks a hot table, an error spike after deploy. Checks score production
  (error rate and incidents from the observability stack, contract tests across services, tickets done, nothing
  merged without review) — never the diff. Traps live in template data, not code.
- **Critical path**: stage 2 starts after Track R of the refactor lands (same core files). Tracks U and K overlap.
- **Proxy routing**: drivers and agents get `HTTP(S)_PROXY` and base-URL env vars (`GITHUB_API_URL`, `GH_HOST`,
  Jira base URL…); git uses smart HTTP through the GitHub twin (no SSH); TLS through a per-world CA the driver
  trusts. Sandboxes (any-agent part 3) are not a dependency. Each proxied request is a step owned by the part whose
  host it targets; requests to unknown hosts are refused and recorded.
- **Saves**: `service` gains per-volume save strategies (database: dump/template; files: hard-link copy;
  stateless: skipped) and saves at fork points and run end only. Observability data is saved with the stack —
  it is the evidence a comparison cites.
- **Twin depth**: the endpoints the common clients use (`gh`, PyGithub, Octokit core; `jira` Python client), each
  covered endpoint tested against the vendor's published OpenAPI schema. Not every endpoint.
- **Model**: gpt-oss-120b supports tool calls under vLLM; both designs share one pod, run one after the other.
- **Builder UI**: mockups shown to the user before it is built, in the existing Shakedown look.

## Left to the builder

Exact REST subsets of the GitHub and Jira twins (driven by what `gh`, PyGithub, and the Jira Python client call in
the designs), the sample application's services, dashboard contents, and the people engine's rule format — each
settled in its stage's plan and recorded there.
