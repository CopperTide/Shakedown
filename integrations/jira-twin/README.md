# jira-twin

**A twin.** Puts a Jira-compatible tracker in the world: the platform REST API under
`/rest/api/{2,3,latest}` and the agile API under `/rest/agile/1.0` that the `jira` Python client
(pycontribs) and REST agents call, in Cloud mode (default) or Server mode. Agents reach it only through
shaked's recording proxy, on the host names its config gives (`docs/protocol.md`, "Serving HTTP").
`describe.system` is `{kind: "tracker", product: "Jira twin (Cloud or Server mode)", stands_in_for: "Jira",
simulated: true}`. Restore: exact from a save (one SQLite store); saved at save points; `uses_clock`: true,
and every time it stamps (created, updated, resolved, comments, sprints) and every relative JQL date is world
time.

Each request acts as the agent the proxy names in `X-Shakedown-Agent` (a user made on first sight), else as the
config's `default_user`; the Basic user name does not choose (agents share one token, and must not pose as
each other). Every `self` and URL is built from the request's `X-Forwarded-Proto`/`X-Forwarded-Host`, and
dereferences: the client PUTs and GETs `self`. Its routes say `honours_readonly`: a request marked
`X-Shakedown-Readonly: 1` gets its reads (searches by POST too) and its writes answer
`403 {"errorMessages": ["You do not have permission to perform this operation: this agent may only read."]}`
with `X-Shakedown-Effect: read`.

## Config

| key | what |
|---|---|
| `hosts` | `[{host, env?}]`; each route's env gets `JIRA_URL`, `JIRA_SERVER` (`https://<host>`), `JIRA_API_TOKEN`, `JIRA_EMAIL` |
| `deployment` | `cloud` (accountIds, `search/jql`, `/search` is 410 Gone) or `server` (name/key, offset `/search` with `total`) |
| `token` | the one token accepted (a Basic password, or `Bearer`) |
| `default_user`, `email_domain` | who a request without an agent acts as; users' addresses are `<name>@<email_domain>` |
| `users` | `[{name, display_name, email?}]` |
| `projects` | `[{key, name, type, lead}]` |
| `workflow` | `{statuses: [{name, category: new/indeterminate/done}], transitions: [{id, name, from?, to}]}`; a transition without `from` is available from every status (default: To Do, In Progress, Done) |
| `issue_types`, `priorities` | optional; defaults Epic/Story/Task/Bug/Sub-task and Highest…Lowest |
| `boards`, `sprints` | `[{id, name, type, project}]`, `[{id, board, name, state, start?, end?, goal?}]` |
| `issues` | `[{project, summary, type, status, priority, assignee, reporter, labels, sprint, rank, description, created, updated}]`, keys given in order; times relative to world now (`-3d`) or ISO |

## What it serves

- The operations in `jira_twin/rest/covered.py` (39 platform, 13 agile): server info, myself, users and user
  search, projects, createmeta (Cloud `issueTypes`/`fields`, Server `values`), issue types, statuses, status
  categories, priorities, resolutions, link types, fields (with Rank and Sprint); issues (create, read, edit with
  `fields` and `update` verbs, delete), transitions (by the workflow; a done status sets a resolution), comments,
  the assignee, issue links; `search/jql` (token pages of at most 100, the last page has no `nextPageToken`, the
  default fields are `id`), offset `search`, approximate count; boards, board sprints, issues and backlog, sprints
  (made, started with dates, closed: open issues go back to the backlog), adding issues to a sprint or the
  backlog, ranking. Others answer `404`.
- API v2 answers and takes rich text (description, comment bodies) as text, v3 as ADF, from one store
  (`jira_twin/adf.py`: paragraphs, hard breaks, mentions); a v3 write with a string body is refused as Jira does.
- JQL (`jira_twin/jql/`): the fields, operators, functions and sorts of research §4.6; errors in Jira's words
  (`Error in the JQL Query: …`, `The value 'X' does not exist for the field 'project'.`). With no `ORDER BY`,
  issues come in the order they were made.

## Tool and notices

`jira_act {person, action, args}` (called by links): `create_issue {project, summary, type?, description?,
assignee?, priority?, labels?}`, `transition {key, to}` (a status or a transition name), `comment {key, body}`,
`assign {key, to}`. At the next `advance` the twin reports `notice` events, `data = {system: "jira", kind, data}`:
`issue_assigned {key, assignee}`, `mentioned {key, user, body}` (a `[~accountid:X]` or `[~name]` in a comment),
`issue_transitioned {key, from, to, by}`, from REST and acts alike.

## Docs

`issue/<KEY>` `{status, status_category, sprint, assignee, resolution, summary, type, priority, labels}`
(`sprint` is the sprint's id); `sprint/<id>` `{name, state, issues}`.

## Verified

`uv run --with pytest pytest -q` (169 tests): JQL, ADF, REST in both modes, acts and notices, save and restore,
read-only agents, every covered answer against Atlassian's descriptions (v2, v3, Software; fetched and pinned,
see `tests/schemas/SOURCE.md`), and the `jira` client 3.10.5, unchanged, against the twin in-process with a
transport standing in for the proxy (`tests/test_client.py`, `tests/fixtures/clients/jira_flow.py`). Not yet
verified through shaked's proxy (the core live test waits for the proxy's agent header).
