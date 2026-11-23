# GitHub twin (`github-twin`)

Puts a GitHub-compatible code host in a Shakedown world, in GitHub Enterprise Server (GHES) mode: real bare git
repositories served over smart HTTP, the REST API under `/api/v3` and the GraphQL API at `/api/graphql`. Agents use
their own clients (`git`, `gh`, PyGithub) and reach it through `shaked`'s recording proxy, on the host
names its config gives. An optional Actions subset runs CI jobs in Docker. The software-factory world template
uses it as the company's code host.

| | |
|---|---|
| Program | `python -m github_twin` (Python, run with `uv`) |
| Stands in for | GitHub Enterprise Server: the subset of its REST and GraphQL APIs listed below, not the whole product |
| Restore | exact, from a save (the SQLite store and the repositories, git objects hard-linked between saves). Saved at save points. No approximate restore. |
| World clock | used: timestamps are world time, notices are reported at `advance`, and a deploy step times out in world time |
| Agent tools | none. Agents call it over HTTP. `github_act` is a world tool that links call. |

## What it needs

- `uv`, and `git` on `PATH` (the twin runs `git http-backend` and other git commands).
- No services and no fixed ports: each instance listens on its own port on `127.0.0.1`, and `shaked` routes the
  world's host names to it ([`docs/protocol.md`](../../docs/protocol.md), "Serving HTTP").
- Docker, only with `actions.enabled`. The twin never pulls an image: each step runs with `--pull never` and each
  build with `--pull=false`, so the images in `actions.runners` must already be on the machine.
  `shaked` clears a program's environment, so a `DOCKER_HOST` set for `shaked` does not reach the `docker` CLI.
- No GPU.

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (in the working directory) | Where saves and the live instances (`.live/`) are kept. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "github", integration: "github-twin", config: {...}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `hosts` | array | required | `[{host, env?}]`: the host names served. Each route's `env` gets `GH_HOST`, `GH_ENTERPRISE_TOKEN`, `GITHUB_API_URL`, `GITHUB_SERVER_URL`, a git credential helper for the host, and switches that keep `gh` quiet. `env` adds more. |
| `org` | string | required | The owner of the seeded repositories. |
| `token` | string | required | The one token accepted (`token`, `Bearer`, or a Basic password). |
| `default_user` | string | required | Who a request without an agent acts as, and who made the seed commits. |
| `installed_version` | string | `3.20.0` | What `/meta` answers. |
| `repos` | array | none | `[{name, source, default_branch, description, private}]`. `source` is a folder whose files become the first commit. `default_branch` defaults to `main`, `private` to true. |
| `protection` | object | none | `{<repo>: {required_reviews}}`: a merge needs that many approvals of the merged head. |
| `review_assignment` | object | none | `{reviewers: [login], count}` (`count` default 1): each new pull request asks the next `count` reviewers in turn, never its author, like a team's round-robin review assignment. |
| `actions` | object | off | `{enabled, runners: {<runs-on label>: <image>}, image_prefix, job_memory, job_cpus, job_pids, step_timeout_s, deploy_timeout_s}`. Defaults: `384m`, `1`, `256`, 600 s, 300 s. |

### Who a request is

Each request acts as the agent the proxy names in `X-Shakedown-Agent` (a user is made on first sight), else as
`default_user`. Every absolute URL is built from the request's `X-Forwarded-Proto` and `X-Forwarded-Host`. The
routes say `honours_readonly`: a request marked `X-Shakedown-Readonly: 1` (a read-only agent's) gets its reads, and
its writes (any REST method but GET, HEAD and OPTIONS, and a push) answer
`403 {"message": "Resource not accessible by integration"}` with `X-Shakedown-Effect: read`. Its GraphQL mutations
get the error `FORBIDDEN`, `Resource not accessible by integration`, and change nothing.

### What it serves

- **Git.** `GET /{owner}/{repo}.git/info/refs`, `POST .../git-upload-pack` (a read) and `POST .../git-receive-pack`,
  with or without `.git`, protocol v0 and v2. A request without the token gets `401` with
  `WWW-Authenticate: Basic realm="GitHub"`.
- **REST.** The 87 operations in `github_twin/rest/covered.py`: repositories, branches, contents, commits, compare,
  the git database, pull requests (merge, squash and rebase, `.diff` and `.patch` media types), reviews, review
  comments, requested reviewers, issues, comments, labels, `search/issues`, statuses, check runs, the Actions
  workflows, runs, jobs and logs, and hooks. Every other path answers `404 {"message": "Not Found"}`.
- **Search.** `search/issues` and GraphQL `search` take a subset of GitHub's query syntax: `repo:`,
  `is:pr|issue|open|closed|merged`, `type:`, `state:`, `author:`, `assignee:`, `label:`, `head:`, `base:`,
  `review-requested:`, `@me` and words in the title or body.
- **Review rules.** An author cannot approve or request changes on their own pull request. At merge the twin
  records who approved the merged head. An approval of an older head does not count.
- **GraphQL.** `POST /api/graphql` over GitHub's GHES 3.20 schema, pruned to what `gh` uses
  (`github_twin/graphql/schema.graphql`, written by `scripts/prune_sdl.py`). Roots: `viewer`, `repository`,
  `repositoryOwner`, `node`, `nodes`, `search`, `organization`, `user` and `__type`. Mutations:
  `createPullRequest`, `updatePullRequest`, `mergePullRequest`, `closePullRequest`, `reopenPullRequest`,
  `markPullRequestReadyForReview`, `addPullRequestReview`, `requestReviews`, `addComment`, `createIssue`,
  `updateIssue`, `closeIssue`, `reopenIssue`, `addLabelsToLabelable` and `removeLabelsFromLabelable`, each by the
  REST rules. A field the twin does not model answers its type's empty value (null, an empty connection, `false`,
  `""`). The fields `gh` probes for optional features (`isInMergeQueue`, `checkRunCount`, `blockedBy`,
  `WorkflowRun.event`, `pullRequestTemplates`, `autoMergeAllowed`, `SearchType.ISSUE_ADVANCED`) are left out, so
  `gh` takes its plain paths. Validation errors use graphql-ruby's words (`Field 'x' doesn't exist on type 'Y'`),
  execution errors GitHub's `type` (`NOT_FOUND`, `FORBIDDEN`, `UNPROCESSABLE`). A query answers
  `X-Shakedown-Effect: read`, a mutation that changed something `write`.
- **Webhooks.** Sent after the request commits, signed with `X-Hub-Signature-256` when the hook has a secret, and
  only to `127.0.0.1`, `localhost` or `::1`. Any other URL is recorded as failed, `outside the world`.

### Actions

With `actions.enabled`, a push, and a pull request opened, synchronised or reopened, start a run of each workflow
in the commit's `.github/workflows/*.yml` whose `on` matches (`push` or `pull_request`, with `branches`).

- **The subset.** `name`, `on`, `env`, and jobs with `name`, `runs-on` (an image from `actions.runners`),
  `container.image`, `needs`, `if` (`github.ref` or `github.event_name`, `==` or `!=` a quoted string), `env` and
  steps. A step is `run`, `actions/checkout@v4` (the workspace is always the commit's files),
  `shakedown/build@v1 {image, context?}` or `shakedown/deploy@v1 {service, image}`. Anything else, and any
  `${{ }}` outside `if`, ends the run `startup_failure`, saying where.
- **How jobs run.** One job at a time per instance, oldest run first. Each `run` step is `docker run --rm --pull
  never --network none` with the memory, CPU and process caps, as the twin's own user, and is removed after
  `step_timeout_s` of wall time. A build is `docker build --pull=false --network none -t
  <image_prefix>/<image>:<sha>`.
- **Results.** Each job is a check run `<workflow> / <job>` and a status `<workflow> / <job> (<event>)`. Logs are
  kept in the store, so a save holds them.
- **Deploys.** A deploy step reports the world event `deploy {service, image, sha, repo, run_id, job_id, by}` and
  waits for `github_act deploy_result {job_id, ok, error?}`. It fails after `deploy_timeout_s` of world time.
- **Saves.** A run in progress when a save was made runs again, as its next attempt, when an instance starts from
  that save.

### The world tool and notices

`github_act {person, action, args}` is called by links, usually for the world's simulated people (see
[`people`](../people/README.md)). Each action follows the REST rules:

| Action | Args | What it does |
|---|---|---|
| `open_pr` | `{repo, branch, base?, patch, title, body}` | applies the patch with `git apply`, commits it as the person and opens a pull request |
| `review` | `{repo, number, event, body}` | reviews a pull request |
| `merge` | `{repo, number, method}` | merges a pull request |
| `comment` | `{repo, number, body}` | comments on an issue or pull request |
| `close` | `{repo, number}` | closes an issue or pull request |
| `deploy_result` | `{job_id, ok, error?}` | the world's answer to a deploy step |

At the next `advance` the twin reports `notice` events with `data = {system: "github", kind, data}`:
`review_requested {repo, number, reviewer, author, title, checks}`, `review_submitted {repo, number, author,
reviewer, state, body}`, `pr_opened {repo, number, author, title, head, base}`, `pr_merged {repo, number, by, sha}`,
`checks_completed {repo, sha, conclusion, prs}` and `mentioned {user, repo, number, body}`.

### inspect and diff

| Key | Doc |
|---|---|
| `repo/<owner>/<repo>` | `{default_branch, private, description, seed_sha}` |
| `branch/<owner>/<repo>/<branch>` | `{sha, forced_pushes, direct_pushes}`. `direct_pushes` counts moves no pull request's merge made: pushes, API ref and contents writes. |
| `pull/<owner>/<repo>/<n>` | `{state, merged, merged_by, author, head_sha, reviewed_at_merge, approvals_at_merge}` |
| `issue/<owner>/<repo>/<n>` | `{state, title, author, labels, assignees, comments}` |
| `check/<owner>/<repo>/<sha>` | `{conclusion, runs}` |
| `run/<owner>/<repo>/<id>` | `{workflow, event, head_branch, head_sha, status, conclusion, attempt, error, jobs}` |
| `hook/<id>` | `{deliveries, failed}` |

## How it's tested

```bash
cd integrations/github-twin
uv run --with pytest pytest -q                          # gh tests skip without gh on PATH
uv run --with pytest pytest -q -m live tests/live/      # Actions on real Docker
```

The default run needs `git` and `openssl` (for the test CA), and no services. It covers clone, fetch and push with git against the twin's own
server on `127.0.0.1`, with the proxy's headers set by the tests. It also covers webhooks to a receiver on
`127.0.0.1`, every covered REST answer against GitHub's GHES 3.20 REST description, and every GraphQL query and
mutation `gh` sent (`tests/gh_queries/`) validated and run. The vendored schemas and their sources are in
`tests/schemas/` (see `SOURCE.md` there). Actions run on a fake Docker: runs, `needs` and `if`, the deploy result
and its world-time timeout, cancel, re-run, a run caught by a save running again after a restore, and two twins
giving the same docs for the same runs.

`tests/test_clients.py` and `tests/test_gh_run.py` run real clients over HTTPS through a TLS front on `127.0.0.1`
with a test CA, one listener per agent, that sets the headers as the proxy does. PyGithub 2.10 opens, approves and
squash-merges a pull request, and the author's own approval is refused. The tests marked `live` run `gh` in GHES
mode (`tests/gh_queries/` holds what gh 2.45.0 sent): `auth status`, `repo clone`, `pr create`, `pr view --json`,
`pr review --approve`, `pr merge --squash --delete-branch`, a read-only agent's `pr list` served and its `pr merge` refused, then `run list`, `run view`
(with `--log`, `--log-failed`, `--job <id> --log`), `run watch`, `run rerun` and `pr checks`. The front listens on a
port, not 443, so those tests pass `-R <host:port>/acme/app`. They run in the default command when `gh` is on
`PATH`.

`tests/live/` (marked `live`, run only with `-m live`) uses real Docker and `python:3.12-slim`, pinned by digest in
`tests/live/images.json` (pulled by the test if missing). The fixture repository's tests pass, then fail with
unittest's lines in the log. A job cannot reach the network, a step past its time is removed, and no container or
folder is left. These ran on Docker on 2026-11-19.

Through `shaked`'s proxy, from the repository root:

```bash
(cd integrations/github-twin && uv sync)
(cd core && cargo test -p shaked --test live_twins -- --ignored live_)
```

`core/crates/shaked/tests/live_twins.rs` drives the twin with `gh` over `CONNECT` on 443 with the run's CA, as two
agents, checks that a read-only agent's merge is refused without failing the branch, and runs PyGithub unchanged.
`benchmarks/world/tests/software_factory/test_live_tools.py` does the same inside a software-factory run. The
commit that added the latter records it as passed once, on 2026-11-20.

## Limits

- A subset of GHES: no organisations' settings, teams' APIs, releases, packages, Pages, apps or OAuth flows. A
  REST path outside `covered.py` is `404`.
- One token for everyone. Who a request is comes from the proxy's agent header, not from the token.
- Actions is a small subset: two events, three actions, no `${{ }}` expressions outside `if`, no matrix and no service
  containers. Jobs run one at a time per instance. A step's timeout is wall time, not world time.
- Webhooks reach only loopback addresses.
- The REST answers follow GitHub's description of GHES 3.20, API version 2022-11-28. Newer fields are not there.
