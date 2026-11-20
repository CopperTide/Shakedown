# github-twin

**A twin.** Puts a GitHub-compatible code host in the world, in GitHub Enterprise Server mode: real bare git
repositories served over smart HTTP (`git http-backend`), the REST API under `/api/v3` that `gh`, PyGithub and
Octokit call, and the GraphQL API at `/api/graphql` that `gh pr` and `gh issue` call. Agents reach it only
through arenad's recording proxy, on the host names its config gives
(`docs/protocol.md`, "Serving HTTP"). `describe.system` is `{kind: "code", product: "GitHub twin (GHES mode)",
stands_in_for: "GitHub Enterprise Server", simulated: true}`. Restore: exact from a save (the SQLite store, and the
repositories with their git objects hard-linked between saves); saved at save points; `uses_clock`: true.

Each request acts as the agent the proxy names in `X-Arena-Agent` (a user made on first sight), else as the
config's `default_user`. Every absolute URL is built from the request's `X-Forwarded-Proto`/`X-Forwarded-Host`.
Its routes say `honours_readonly`: a request marked `X-Arena-Readonly: 1` (a read-only agent) gets its reads, and
its writes (any REST method but GET/HEAD/OPTIONS, and a push) answer `403 {"message": "Resource not accessible by
integration"}` with `X-Arena-Effect: read`. The proxy does not send either header yet (the Rust half is to come).

## Config

| key | what |
|---|---|
| `hosts` | `[{host, env?}]`: the host names served; each route's env gets `GH_HOST`, `GH_ENTERPRISE_TOKEN`, `GITHUB_API_URL`, `GITHUB_SERVER_URL`, git's credential helper and gh's quiet switches |
| `org` | the owner of the seeded repositories |
| `token` | the one token accepted (`token`, `Bearer`, or a Basic password) |
| `default_user` | who a request without an agent acts as, and who made the seed commits |
| `installed_version` | what `/meta` answers (default `3.20.0`, gh's leanest GHES path) |
| `repos` | `[{name, source, default_branch, description, private}]`: `source` is a folder whose files become the first commit |
| `protection` | `{<repo>: {required_reviews}}` (none by default) |
| `review_assignment` | `{reviewers: [login], count: 1}`: each new pull request asks the next `count` reviewers in turn (the n-th pull request of the instance starts at the n-th), never its author, as a team's round-robin code review assignment; none by default |
| `actions` | `{enabled, runners: {<runs-on label>: <image>}, image_prefix, job_memory: "384m", job_cpus: "1", job_pids: 256, step_timeout_s: 600, deploy_timeout_s: 300}`: off unless `enabled` |

## What it serves

- Git: `GET /{o}/{r}.git/info/refs`, `POST …/git-upload-pack` (a read), `POST …/git-receive-pack`, with or without
  `.git`, protocol v0 and v2; a request without the token gets `401` and `WWW-Authenticate: Basic realm="GitHub"`.
- REST: the operations in `github_twin/rest/covered.py`, each answer checked against GitHub's description of GHES
  3.20 (`tests/schemas/`, see `SOURCE.md` there): repos, branches, contents, commits, compare, the git database,
  pull requests (merge, squash, rebase; `.diff`/`.patch` media types), reviews, review comments, requested
  reviewers, issues, comments, labels, `search/issues` (a subset of the query syntax), statuses, check runs and
  hooks. Others answer `404 {"message": "Not Found"}`.
- Rules: an author cannot approve or request changes on their own pull request; at merge the twin records who
  approved the merged head (an approval of an older head does not count). `protection: {<repo>: {required_reviews}}`
  blocks a merge without them; there is none by default.
- GraphQL: `POST /api/graphql` over GitHub's GHES 3.20 schema pruned to what gh uses (`github_twin/graphql/schema.graphql`,
  written by `scripts/prune_sdl.py`; checked against GitHub's own definitions in `tests/schemas/`): the roots
  `viewer`, `repository` (pull requests, issues, labels, assignable users, refs), `node(s)`, `search` (issues and
  pull requests, the REST search subset plus `review-requested:` and `@me`), `organization`, `user` and `__type`;
  the mutations `createPullRequest`, `updatePullRequest`, `mergePullRequest`, `closePullRequest`,
  `reopenPullRequest`, `markPullRequestReadyForReview`, `addPullRequestReview`, `requestReviews`, `addComment`,
  `createIssue`, `updateIssue`, `closeIssue`, `reopenIssue`, `addLabelsToLabelable`, `removeLabelsFromLabelable`,
  each by the same rules as REST. A field the twin does not model answers its type's empty value (null, an empty
  connection, `false`, `""`), so any field of the schema may be asked for. The fields gh probes for optional code
  paths (`isInMergeQueue`, `checkRunCount`, `blockedBy`, `WorkflowRun.event`, `pullRequestTemplates`,
  `autoMergeAllowed`, `SearchType.ISSUE_ADVANCED`) are left out. Validation errors are in graphql-ruby's words
  (`Field 'x' doesn't exist on type 'Y'`), execution errors carry GitHub's `type` (`NOT_FOUND`, `FORBIDDEN`,
  `UNPROCESSABLE`). Queries answer `X-Arena-Effect: read`, a mutation that changed something `write`; a read-only
  agent's mutation gets `FORBIDDEN` `Resource not accessible by integration` and changes nothing.
- Actions (`actions.enabled`): a push, and a pull request opened, synchronised or reopened, start a run of each
  workflow in the commit's `.github/workflows/*.yml` whose `on` matches (`push`/`pull_request`, `branches`). The
  subset: `name`, `on`, `env`, jobs with `name`, `runs-on` (an image from `actions.runners`), `container.image`,
  `needs`, `if` (`github.ref`/`github.event_name` `==`/`!=` a string), `env`, and steps of `run`,
  `actions/checkout@v4` (the workspace is always the commit's files), `shakedown/build@v1 {image, context?}`
  (`docker build --pull=false --network none -t <image_prefix>/<image>:<sha>`) and `shakedown/deploy@v1
  {service, image}`. Anything else (and any `${{ }}` outside `if`) ends the run `startup_failure` saying where.
  One job at a time per instance, oldest run first; each `run` step is `docker run --rm --pull never --network
  none` with the memory, CPU and process caps, the twin's own user, and is removed past `step_timeout_s`. Each job
  is a check run `<workflow> / <job>` and a status `<workflow> / <job> (<event>)`; logs are kept in the store
  (saved with it). A deploy step queues the event `deploy {service, image, sha, repo, run_id, job_id, by}` and
  waits for `github_act deploy_result {job_id, ok, error?}`, failing after `deploy_timeout_s` of world time. A run
  in progress when a save was made runs again, as its next attempt, when an instance starts from that save.
- Webhooks are sent after the request commits, signed with `X-Hub-Signature-256` when the hook has a secret, and
  only to `127.0.0.1`, `localhost` or `::1`; any other URL is recorded failed, `outside the world`.

## Tool and notices

`github_act {person, action, args}` (called by links): `open_pr {repo, branch, base?, patch, title, body}` (the
patch applied with `git apply` and committed as the person), `review {repo, number, event, body}`,
`merge {repo, number, method}`, `comment {repo, number, body}`, `close {repo, number}`, `deploy_result {job_id, ok,
error?}` (the world's answer to a deploy step).

With Actions on, a deploy step also reports a `deploy` event (above).

At the next `advance` the twin reports `notice` events, `data = {system: "github", kind, data}`:
`review_requested {repo, number, reviewer, author, title, checks}`, `pr_opened`, `pr_merged {repo, number, by, sha}`,
`checks_completed {repo, sha, conclusion, prs}`, `mentioned {user, repo, number, body}`,
`review_submitted {repo, number, author, reviewer, state, body}` (a review leaving PENDING).

## Docs

`repo/<o>/<r>` `{default_branch, private, description, seed_sha}` (`seed_sha`: the commit it was seeded with); `branch/<o>/<r>/<b>` `{sha, forced_pushes, direct_pushes}` (`direct_pushes`: moves that no pull request's merge made: pushes, API ref and contents writes);
`pull/<o>/<r>/<n>` `{state, merged, merged_by, author, head_sha, reviewed_at_merge, approvals_at_merge}`;
`issue/<o>/<r>/<n>` `{state, title, author, labels, assignees, comments}`; `check/<o>/<r>/<sha>` `{conclusion, runs}`;
`run/<o>/<r>/<id>` `{workflow, event, head_branch, head_sha, status, conclusion, attempt, error, jobs}`;
`hook/<id>` `{deliveries, failed}`.

## Verified

`uv run --with pytest pytest -q` here: clone, fetch and push with git 2.43 against the twin's own server on
127.0.0.1 (with `X-Forwarded-*`, `X-Arena-Agent` and `X-Arena-Readonly` set as the proxy will set them),
webhooks to a receiver on 127.0.0.1, every covered REST operation's answer against the schema, and every GraphQL
query and mutation gh sent (`tests/gh_queries/`) validated and run over the twin.

`tests/test_clients.py`, over HTTPS on 127.0.0.1 through a TLS front with a test CA that sets the headers as the
proxy will (one listener per agent): PyGithub 2.10 with `base_url` (branch, file, pull request, the author's own
approval refused, the reviewer's approval, squash merge); and, marked `live`, gh 2.45.0 in GHES mode
(`GH_HOST`, `GH_ENTERPRISE_TOKEN`, `SSL_CERT_FILE`): `auth status`, `repo clone`, `git push`, `pr create`,
`pr view --json`, `pr review --approve` as the author (refused) and as the reviewer, `pr merge --squash
--delete-branch`; a read-only agent's `pr list` served and `pr merge` refused. Because the front listens on a
port (not 443), gh cannot match the clone's remote to `GH_HOST`, so the tests pass `-R <host:port>/acme/app`.
Not yet verified through arenad's proxy (it does not send `X-Arena-Agent` yet).

Actions: `tests/test_actions_*.py` on a fake Docker (runs, needs and `if`, deploy result and world-time timeout,
cancel, re-run, a run caught by a save run again after restore, the same runs from two twins giving the same docs);
`tests/test_gh_run.py` (`live`, gh 2.45.0 through the TLS front): `run list` (table and `--json`), `run view`,
`--log`, `--log-failed`, `--job <id> --log`, `run watch` following a run to its end, `run rerun`, `pr checks`.
`uv run --with pytest pytest -q -m live tests/live/` (Docker 2026-11-19, `python:3.12-slim` pinned by digest in
`tests/live/images.json`): the fixture repo's `python -m unittest` passes, then fails with unittest's lines in the
log; a job cannot reach the network; a step past its time is removed; no container or folder is left.
