# Slack twin (`slack-twin`)

Puts a Slack-compatible workspace in a Shakedown world: eleven Web API methods (`/api/<method>`) for users,
channels, messages, threads, reactions and search. Agents use their own clients (`slack_sdk`, plain HTTP) and
reach it through `shaked`'s recording proxy, on the host names its config gives. Apps in the config can post what
other systems report, as the GitHub or PagerDuty apps for Slack do. The software-factory world template uses it as
the company's chat.

| | |
|---|---|
| Program | `python -m slack_twin` (Python, run with `uv`) |
| Stands in for | Slack: the Web API methods listed below, not the whole product |
| Restore | exact, from a save (one SQLite store). Saved at save points. No approximate restore. |
| World clock | used: every message `ts` is world time (microseconds, unique in the workspace). Notices are reported at `advance`. |
| Agent tools | none. Agents call it over HTTP. `slack_act` and `slack_app` are world tools that links call. |

## What it needs

- `uv`. No services, no Docker, no GPU and no fixed ports: each instance listens on its own port on `127.0.0.1`,
  and `shaked` routes the world's host names to it ([`docs/protocol.md`](../../docs/protocol.md), "Serving HTTP").

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | `saves` (in the working directory) | Where saves and the live instances (`.live/`) are kept. `shaked` always passes its own. |

## Use it

A world names it as a part, for example `{part: "slack", integration: "slack-twin", config: {...}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `hosts` | array | required | `[{host, env?}]`: the host names served. Each route's `env` gets `SLACK_API_URL` (`https://<host>/api/`) and `SLACK_BOT_TOKEN`. `env` adds more. |
| `token` | string | required | The one token accepted. |
| `default_user` | string | required | Who a call without an agent acts as, and who made the channels. |
| `team` | object | none | `{name, email_domain?}`. `email_domain` defaults to `example.test`. |
| `users` | array | none | `[{name, real_name}]`. |
| `channels` | array | none | `[{name, topic?, purpose?, private?, general?}]`. |
| `messages` | array | none | `[{channel, user, text, at, thread_ts?}]`: earlier history. `at` is relative to world now (`-2h`, `-1d`, `-30m`). |
| `apps` | array | none | `[{name, user, channel, messages: {<kind>: <text>}}]`: apps that post, as `user` in `channel`, the text for each kind of event (see `slack_app`). `{/pointer}` in a text is that value of the event's data. |

### Who a call is

Each call acts as the agent the proxy names in `X-Shakedown-Agent` (a user is made on first sight), else as
`default_user`. Arguments come from the query, a form body or a JSON body, and the token from
`Authorization: Bearer` or a `token` argument. Every answer is HTTP 200, `{"ok": true, ...}` or
`{"ok": false, "error": "<code>"}`. `slack_sdk` POSTs every call, so the routes list the read methods as reads. The
routes say `honours_readonly`: a call marked `X-Shakedown-Readonly: 1` gets its reads, and its writes answer
`{"ok": false, "error": "missing_scope", "needed": ..., "provided": ...}` with `X-Shakedown-Effect: read`.

### What it serves

| Method | Notes |
|---|---|
| `auth.test` | |
| `users.list`, `users.info` | `users.list` pages by cursor |
| `conversations.list`, `conversations.info` | `conversations.list` pages by cursor and takes `types` |
| `conversations.history` | newest first, `latest`, `oldest`, `inclusive`, `limit`, `has_more` |
| `conversations.replies` | a thread |
| `chat.postMessage` | a channel id, `name` or `#name`, and `thread_ts` |
| `chat.update` | the author only, else `cant_update_message` |
| `reactions.add` | |
| `search.messages` | every word, `in:#channel`, `from:@name`, newest first |

The `conversations.*` methods take channel ids only (`channel_not_found` for a name), as Slack does. Any other
method answers `unknown_method`. Answers follow Slack's published Web API description (`tests/schemas/SOURCE.md`).
That description predates `blocks` and `edited`, so the twin does not answer them, and
`conversations.replies` shows no reactions.

### The world tools and notices

Both are called by links:

- `slack_act {person, action, args}`, usually for the world's simulated people (see
  [`people`](../people/README.md)): `post {channel, text, thread_ts?}` and `react {channel, ts, name}`.
- `slack_app {app, kind, data}`, usually from another twin's `notice` events: the app posts the text it has for
  `kind`, with `data` filled in. For a kind it has no text for it posts nothing and answers `{posted: false}`. An
  app the config lacks is refused.

At the next `advance` the twin reports `notice` events with `data = {system: "slack", kind, data}`:
`mentioned {channel, user, text, ts}` for each `<@U...>`, or `@name` of a known user, in a posted message. The
author is left out.

### inspect and diff

| Key | Doc |
|---|---|
| `channel/<name>` | `{messages: [{user, text, ts, thread_ts?}], last_ts}`, oldest first, users by name |

## How it's tested

```bash
cd integrations/slack-twin
uv run --with pytest pytest -q
```

The tests need no services. They run `slack_sdk` 3.47's `WebClient` unchanged, with its base URL set to the twin and
the proxy's headers added. They cover errors, tokens, threads, reactions, search, mentions, acts, apps' posts,
read-only agents, save and restore, and every method's success and error answers against Slack's description
(vendored in `tests/schemas/`).

The twin is a part of the software-factory world, whose live tests under `shaked`
(`benchmarks/world/tests/software_factory/test_live_world.py`) start it. No test drives `slack_sdk` through
`shaked`'s proxy.

## Limits

- Eleven methods. No events API, Socket Mode, files, user groups, direct messages, pins, bookmarks or Block Kit.
- One token for everyone. Who a call is comes from the proxy's agent header.
- Slack's description was archived in 2021, so the answers lack fields Slack has added since.
