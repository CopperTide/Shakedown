# github-twin

**A twin.** Puts a GitHub-compatible code host in the world, in GitHub Enterprise Server mode: real bare git
repositories served over smart HTTP (`git http-backend`), and the REST API under `/api/v3` that `gh`, PyGithub and
Octokit call. Agents reach it only through arenad's recording proxy, on the host names its config gives
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
- Webhooks are sent after the request commits, signed with `X-Hub-Signature-256` when the hook has a secret, and
  only to `127.0.0.1`, `localhost` or `::1`; any other URL is recorded failed, `outside the world`.

## Tool and notices

`github_act {person, action, args}` (called by links): `open_pr {repo, branch, base?, patch, title, body}` (the
patch applied with `git apply` and committed as the person), `review {repo, number, event, body}`,
`merge {repo, number, method}`, `comment {repo, number, body}`, `close {repo, number}`.

At the next `advance` the twin reports `notice` events, `data = {system: "github", kind, data}`:
`review_requested {repo, number, reviewer, author, title, checks}`, `pr_opened`, `pr_merged {repo, number, by, sha}`,
`checks_completed {repo, sha, conclusion, prs}`, `mentioned {user, repo, number, body}`.

## Docs

`repo/<o>/<r>` `{default_branch, private, description}`; `branch/<o>/<r>/<b>` `{sha, forced_pushes}`;
`pull/<o>/<r>/<n>` `{state, merged, merged_by, author, head_sha, reviewed_at_merge, approvals_at_merge}`;
`issue/<o>/<r>/<n>` `{state, title, author, labels, assignees, comments}`; `check/<o>/<r>/<sha>` `{conclusion, runs}`;
`hook/<id>` `{deliveries, failed}`.

## Verified

`uv run --with pytest pytest -q` here: clone, fetch and push with git 2.43 against the twin's own server on
127.0.0.1 (with `X-Forwarded-*`, `X-Arena-Agent` and `X-Arena-Readonly` set as the proxy will set them),
webhooks to a receiver on 127.0.0.1, and every covered operation's answer against the schema. No GraphQL yet
(stage 4 Task 4), so `gh pr`/`gh issue` do not work yet. Not yet verified through arenad's proxy, nor with `gh` or PyGithub (stage 4 Task 4).
