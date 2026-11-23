# Jira twin (`jira-twin`)

Puts a Jira-compatible issue tracker in a Shakedown world: the platform REST API under `/rest/api/{2,3,latest}`
and the agile API under `/rest/agile/1.0`, in Jira Cloud mode (the default) or Jira Server mode, with a JQL subset,
boards and sprints. Agents use their own clients (the `jira` Python client, plain REST) and reach it through
`shaked`'s recording proxy, on the host names its config gives. The software-factory world template uses it as
the company's tracker.

| | |
|---|---|
| Program | `python -m jira_twin` (Python, run with `uv`) |
| Stands in for | Jira (Cloud or Server): the subset of its REST and agile APIs listed below, not the whole product |
| Restore | exact, from a save (one SQLite store). Saved at save points. No approximate restore. |
| World clock | used: every time it stamps (created, updated, resolved, comments, sprints) and every relative JQL date is world time. Notices are reported at `advance`. |
| Agent tools | none. Agents call it over HTTP. `jira_act` is a world tool that links call. |

## What it needs

- `uv`. No services, no Docker, no GPU and no fixed ports: each instance listens on its own port on `127.0.0.1`,
  and `shaked` routes the world's host names to it ([`docs/protocol.md`](../../docs/protocol.md), "Serving HTTP").

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (in the working directory) | Where saves and the live instances (`.live/`) are kept. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "jira", integration: "jira-twin", config: {...}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `hosts` | array | required | `[{host, env?}]`: the host names served. Each route's `env` gets `JIRA_URL` and `JIRA_SERVER` (`https://<host>`), `JIRA_API_TOKEN` and `JIRA_EMAIL`. `env` adds more. |
| `deployment` | string | `cloud` | `cloud`: accountIds, token-paged `search/jql`, and `/search` answers `410 Gone`. `server`: names and keys, offset-paged `/search` with `total`. |
| `token` | string | required | The one token accepted (a Basic password, or `Bearer`). |
| `default_user` | string | required | Who a request without an agent acts as. |
| `email_domain` | string | `example.test` | Users' addresses are `<name>@<email_domain>` unless given. |
| `users` | array | none | `[{name, display_name, email?}]`. Project leads, assignees and reporters are made too. |
| `projects` | array | required | `[{key, name, type, lead}]`. `type` defaults to `software`. |
| `workflow` | object | To Do, In Progress, Done | `{statuses: [{name, category}], transitions: [{id, name, from?, to}]}`. `category` is `new`, `indeterminate` or `done`. A transition without `from` is available from every status. |
| `issue_types` | array | Epic, Story, Task, Bug, Sub-task | The issue types. |
| `priorities` | array | Highest, High, Medium, Low, Lowest | The priorities, highest first. |
| `boards` | array | none | `[{id, name, type, project}]`. `type` defaults to `scrum`. |
| `sprints` | array | none | `[{id, board, name, state, start?, end?, goal?}]`. `state` defaults to `future`. |
| `issues` | array | none | `[{project, summary, type, status, priority, assignee, reporter, labels, sprint, rank, description, created, updated}]`, keyed in order. Times are relative to world now (`-3d`) or ISO. |

### Who a request is

Each request acts as the agent the proxy names in `X-Shakedown-Agent` (a user is made on first sight), else as
`default_user`. The Basic user name does not choose: agents share one token and must not pose as each other. Every
`self` and URL is built from the request's `X-Forwarded-Proto` and `X-Forwarded-Host`, and dereferences, since the
`jira` client PUTs and GETs `self`. The routes say `honours_readonly`: a request marked `X-Shakedown-Readonly: 1`
gets its reads (searches by POST too, which the routes list as reads), and its writes answer
`403 {"errorMessages": ["You do not have permission to perform this operation: this agent may only read."]}` with
`X-Shakedown-Effect: read`.

### What it serves

- **Operations.** The 52 in `jira_twin/rest/covered.py`, 39 platform and 13 agile: server info, myself, users and
  user search, projects, createmeta (Cloud `issueTypes` and `fields`, Server `values`), issue types, statuses,
  status categories, priorities, resolutions, link types, fields (with Rank and Sprint), issues (create, read,
  edit with `fields` and `update` verbs, delete), transitions (by the workflow, and a done status sets a resolution),
  comments, the assignee, issue links, `search/jql`, offset `search`, the approximate count, boards, a board's
  sprints, issues and backlog, sprints (made, started with dates, closed), adding issues to a sprint or the
  backlog, and ranking. Every other path answers `404`.
- **Search.** `search/jql` pages by token, at most 100 issues a page. The last page has no `nextPageToken`, and the
  default fields are `id`. Closing a sprint moves its unresolved issues back to the backlog.
- **Rich text.** API v2 takes and answers rich text (descriptions, comment bodies) as text, v3 as the Atlassian
  Document Format, from one store (`jira_twin/adf.py`: paragraphs, hard breaks, mentions). A v3 write with a
  string body is refused, as Jira does.
- **JQL** (`jira_twin/jql/`). `AND`, `OR`, `NOT` and `ORDER BY`. The operators `=`, `!=`, `~`, `!~`, `>`, `>=`, `<`, `<=`,
  `in`, `not in`, `is` and `is not`, with `EMPTY`. The fields project, key, status, statusCategory, issuetype, assignee, reporter, creator,
  priority, labels, resolution, sprint, rank, parent, the dates created, updated, resolved and duedate, and the
  text fields summary, description, comment and text. Functions: `currentUser()`, `openSprints()`,
  `closedSprints()`, `futureSprints()`, `now()` and the `startOf...` and `endOf...` day, week and month functions.
  Errors use Jira's words (`Error in the JQL Query: ...`, `The value 'X' does not exist for the field
  'project'.`). With no `ORDER BY`, issues come in the order they were made.

### The world tool and notices

`jira_act {person, action, args}` is called by links, usually for the world's simulated people (see
[`people`](../people/README.md)). Each action follows the REST rules:

| Action | Args | What it does |
|---|---|---|
| `create_issue` | `{project, summary, type?, description?, assignee?, priority?, labels?}` | creates an issue |
| `transition` | `{key, to}` | moves an issue to a status, or by a transition's name |
| `comment` | `{key, body}` | comments on an issue |
| `assign` | `{key, to}` | assigns an issue (null unassigns) |

At the next `advance` the twin reports `notice` events with `data = {system: "jira", kind, data}`, from REST and
acts alike: `issue_assigned {key, assignee}`, `issue_transitioned {key, from, to, by}` and `mentioned {key, user,
body}` (a `[~accountid:X]` or `[~name]` in a comment).

### inspect and diff

| Key | Doc |
|---|---|
| `issue/<KEY>` | `{status, status_category, sprint, assignee, resolution, summary, type, priority, labels}` (`sprint` is the sprint's id) |
| `sprint/<id>` | `{name, state, issues}` |

## How it's tested

```bash
cd integrations/jira-twin
uv run --with pytest pytest -q
```

The tests need no services. They cover JQL, ADF, REST in both modes, acts and notices, save and restore, and
read-only agents. `tests/test_client.py` runs the `jira` client 3.10.5 unchanged
(`tests/fixtures/clients/jira_flow.py`) against the twin in the same process, with a transport that stands in for
the proxy. `tests/test_schema.py` checks every covered answer against Atlassian's OpenAPI descriptions of Jira
Cloud (platform v2 and v3, and Jira Software). They are not committed: the test fetches pinned Wayback Machine
copies on first use into `tests/.vendor-cache/`, checks their sha256, and skips, saying why, when it cannot (see
`tests/schemas/SOURCE.md`).

The twin is a part of the software-factory world, whose live tests under `shaked`
(`benchmarks/world/tests/software_factory/test_live_world.py`) start it. No test drives the `jira` client through
`shaked`'s proxy.

## Limits

- A subset of Jira: no custom fields beyond Rank and Sprint, no permissions or roles, no attachments, worklogs,
  versions, components, filters, dashboards or webhooks. `fixVersion` and `component` are
  known to JQL but always empty.
- One token for everyone. Who a request is comes from the proxy's agent header.
- JQL is a subset. History operators (`WAS`, `CHANGED`) are not there.
- The answers follow Atlassian's Cloud descriptions. Server mode changes ids, paging and search, not every shape.
