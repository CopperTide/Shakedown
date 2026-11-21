# slack-twin

**A twin.** Puts a Slack-compatible workspace in the world: the Web API methods (`/api/<method>`) that
`slack_sdk` and chat agents call. Agents reach it only through shaked's recording proxy, on the host names its
config gives (`docs/protocol.md`, "Serving HTTP"). `describe.system` is `{kind: "chat", product: "Slack twin
(Web API)", stands_in_for: "Slack", simulated: true}`. Restore: exact from a save (one SQLite store); saved at
save points; `uses_clock`: true, and every `ts` is world time (µs, unique in the workspace).

Each call acts as the agent the proxy names in `X-Shakedown-Agent` (a user made on first sight), else as the
config's `default_user`. Arguments come from the query, a form body or a JSON body; the token from
`Authorization: Bearer` or a `token` argument. Every answer is HTTP 200, `{"ok": true, …}` or
`{"ok": false, "error": "<code>"}`. Its routes say `honours_readonly`: a call marked `X-Shakedown-Readonly: 1` gets
its reads, and its writes answer `{"ok": false, "error": "missing_scope", "needed": …, "provided": …}` with
`X-Shakedown-Effect: read`. The read methods are listed in its routes' `reads` (slack_sdk POSTs every call).

## Config

| key | what |
|---|---|
| `hosts` | `[{host, env?}]`; each route's env gets `SLACK_API_URL` (`https://<host>/api/`) and `SLACK_BOT_TOKEN` |
| `token` | the one token accepted |
| `default_user` | who a call without an agent acts as |
| `team` | `{name, email_domain?}` |
| `users` | `[{name, real_name}]` |
| `channels` | `[{name, topic?, purpose?, private?, general?}]` |
| `messages` | `[{channel, user, text, at}]`: earlier history, `at` relative to world now (`-2h`, `-1d`, `-30m`) |
| `apps` | `[{name, user, channel, messages: {<kind>: <text>}}]`: apps that post what another system tells the world (`slack_app`); `{/pointer}` in a text is that value of the event's data |

## What it serves

`auth.test`, `users.list`, `users.info`, `conversations.list`, `conversations.info`, `conversations.history`
(newest first, `latest`/`oldest`/`inclusive`/`limit`, `has_more`), `conversations.replies`, `chat.postMessage`
(a channel id, `name` or `#name`; `thread_ts`), `chat.update` (the author only: `cant_update_message`),
`reactions.add`, `search.messages` (every word, `in:#channel`, `from:@name`; newest first). The `conversations.*`
methods take channel ids only (`channel_not_found` for a name), as Slack does. Others answer `unknown_method`.
Answers follow Slack's published description (`tests/schemas/SOURCE.md`), which predates `blocks` and `edited`;
`conversations.replies` shows no reactions (the description has none there).

## Tool and notices

`slack_act {person, action, args}` (called by links): `post {channel, text, thread_ts?}`, `react {channel, ts,
name}`. `slack_app {app, kind, data}` (called by links, e.g. from GitHub's or PagerDuty's `notice` events): the
app posts, as its `user` in its `channel`, the text it has for `kind` (nothing for a kind it has no text for;
`{posted: false}`); an app the config lacks is refused. At the next `advance` the twin reports `notice` events, `data = {system: "slack", kind, data}`:
`mentioned {channel, user, text, ts}` for each `<@U…>` or `@name` (of a known user) in a posted message, the
author left out.

## Docs

`channel/<name>` `{messages: [{user, text, ts, thread_ts?}], last_ts}` (oldest first, users by name).

## Verified

`uv run --with pytest pytest -q` (39 tests): the methods with `slack_sdk` 3.47 `WebClient` unchanged (base URL
set to the twin, the proxy's headers added), errors, threads, reactions, search, mentions, acts, apps' posts, read-only agents,
save and restore, and every method's success and error answers against Slack's description. Not yet verified
through shaked's proxy.
