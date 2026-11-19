# github-twin

**A twin.** Puts a GitHub-compatible code host in the world, in GitHub Enterprise Server mode: real bare git
repositories served over smart HTTP (`git http-backend`), and the REST API under `/api/v3` that `gh`, PyGithub and
Octokit call. Agents reach it only through arenad's recording proxy, on the host names its config gives
(`docs/protocol.md`, "Serving HTTP"). `describe.system` is `{kind: "code", product: "GitHub twin (GHES mode)",
stands_in_for: "GitHub Enterprise Server", simulated: true}`. Restore: exact from a save (the SQLite store, and the
repositories with their git objects hard-linked between saves); saved at save points; `uses_clock`: true.

Each request acts as the agent the proxy names in `X-Arena-Agent` (a user made on first sight), else as the
config's `default_user`. Every absolute URL is built from the request's `X-Forwarded-Proto`/`X-Forwarded-Host`.

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
  3.20 (`tests/schemas/`, see `SOURCE.md` there). Others answer `404 {"message": "Not Found"}`.

## Docs

`repo/<o>/<r>` `{default_branch, private, description}`; `branch/<o>/<r>/<b>` `{sha, forced_pushes}`.

## Verified

`uv run --with pytest pytest -q` here: clone, fetch and push with git 2.43 against the twin's own server on
127.0.0.1 (with `X-Forwarded-*` set as the proxy sets them), and every covered operation's answer against the
schema. Not yet verified through arenad's proxy, nor with `gh` or PyGithub (stage 4 Task 4).
