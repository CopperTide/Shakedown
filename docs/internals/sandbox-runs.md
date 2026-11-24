# Sandbox runs

How a branch runs in a sandbox (hosted Shakedown, milestone M2a) while the **control box**, the shaked that
owns the database, stays the authority. Paths are relative to `core/crates/shaked/`.

## Job

In a sandbox run the world and the agents run inside a sandbox. The control box keeps the record, the keys,
the rules and the final word. The two talk over one outbound WebSocket from the sandbox to the control box's
`GET /worker`, and the sandbox's model calls go to the control box's gateway with a worker token.

What exists in the engine today: `shaked worker`, the link on both ends, gateway forwarding, linked-branch
enforcement on the control box, holds and the sweep, and the E2B sandbox backend (`src/hosting/e2b.rs`:
create, write a file, kill, state). What does not exist yet: `Hosting`, the code that creates a sandbox for a
branch and hands it its ticket, and the reconciler (M2a Tasks 7, 8 and 12 in the Cloud plan
`docs/superpowers/plans/2026-11-21-hosted-shakedown-m2a-sandbox-runner.md`). Until then the tests play that
part (`tests/support/link.rs`, `ticketed`: `record_start`, `links.expect(plan)`, `links.tickets.issue`).

## Modules

| Module | Job |
|---|---|
| `src/worker/run.rs` | `shaked worker --link <ws-url> --ticket-file <path>`: waits for the ticket file, opens the link, builds the worker's own shaked, stands the branch up and drives it, obeys controls. |
| `src/worker/client.rs` | The worker's end of the link: hello, `Start`, reconnect, resend, give up (`Patience`, `LinkClient`). |
| `src/worker/link_records.rs` | `LinkRecords`: `BranchRecords` over the link. The outbox, the window, frame cutting. |
| `src/worker/frames.rs` | The frames (`ToControl`, `ToWorker`), `WINDOW` (64), `GIVE_UP` (5 min). |
| `src/worker/link.rs` | The control box's end: `GET /worker`, hello with a ticket or a worker token, append and ack, `Ended`, keepalive, rebuilding a link after a restart. |
| `src/worker/links.rs` | `Links` and `Link`: per branch the plan, caller, budget, hold, in-flight permits, the open connection and the last events appended. |
| `src/worker/holds.rs` | Pause, resume and stop of a linked branch: `put`, `keep`, `tell`, `arm`, `end`, `end_locked`, `sweep`. |
| `src/worker/tickets.rs` | One-use tickets traded for a worker token. Ten minutes, in memory, sha256 only. |
| `src/gateway/forward.rs` | The worker's gateway: forwards each call to the control box with the worker token only. |
| `src/gateway/linked.rs` | The control box's view of a linked call: worker token, link, limits, in-flight permit, hold, `settle`. |
| `src/gateway/charge.rs` | What a linked call is charged when its cost is unknown. |
| `src/store/linked.rs` | `append_linked`: one event per `link_n`, only while the branch runs. |
| `src/store/control.rs` | `branches.rules` and `branches.hold`: what the control box decided, kept from the worker. |
| `src/jobs.rs` | `linked_branches`, `end_linked`, `EndIf`: system jobs for the sweep and ends. |
| `src/layer.rs` | `WorkerLayer`: no vault, no environment keys. |
| `src/model/hold.rs` | `Hold`: running, paused, or stopping with the status the branch ends with. |
| `src/hosting/mod.rs` | `SandboxSpec`, `ProviderRef`, `ProviderState`, the `SandboxBackend` trait, `TICKET_PATH` (`/run/shaked/ticket`). |
| `src/hosting/e2b.rs` | `E2bBackend`: sandboxes on E2B. |

## The whole flow

```mermaid
sequenceDiagram
    participant H as Hosting (Task 7, next step)
    participant CB as control box
    participant WK as shaked worker (sandbox)
    participant P as provider
    H->>CB: record_start(spec): run, branch, rules on record
    H->>CB: links.expect(plan), tickets.issue(branch, sandbox)
    H->>WK: start the sandbox, write the ticket file
    WK->>CB: WS /worker, Hello {ticket sdt_, resume_from 0}
    CB->>CB: take_ticket, start_for(caller), mint worker token sdw_
    CB-->>WK: Start {plan, token, gateway}, then the current hold if any
    WK->>WK: stand_up(plan, LinkRecords): world, RunStart, setup
    loop every event
        WK->>CB: Event {n, agent, kind, payload, overhead}
        CB->>CB: append_linked under the branch lock
        CB-->>WK: Ack {n, seq}
    end
    WK->>CB: POST /gateway/... with Bearer sdw_ only
    CB->>CB: linked::open, held_back, admit, provider_for (caller's key)
    CB->>P: the call with the run's key
    P-->>CB: answer
    CB->>CB: bill, book (one model_call), settle
    CB-->>WK: answer, relayed chunk by chunk
    WK->>CB: Ended {status, error} once every event is acked
    CB->>CB: hold wins over status, end_locked: status, revoke, forget
    CB-->>WK: link closed
```

## The worker

`worker::run::command` (`src/worker/run.rs`):

1. **Ticket file.** It polls `--ticket-file` every 200 ms. A file is read only once it ends with a newline (a
   file still being written is left alone), then trimmed and deleted. Whoever hands the ticket over writes a
   temporary file in the same folder and renames it.
2. **Open.** `client::open` says hello with the ticket and waits up to 60 s for `Start`.
3. **Worker app.** `worker_app` builds a shaked with `Store::detached` (no database: any query fails at once),
   `WorkerLayer`, `ProviderConfig::without_environment()` and `forward_to` set. Its world MCP endpoint, gateway
   and proxy listen on 127.0.0.1 only.
4. **Drive.** `runner::stand_up(plan, LinkRecords)`, then `run_branch`. `obey` applies `Pause`, `Resume`,
   `Stop` and `OverBudget` from the control box.
5. **End.** `until_end` waits for the branch and for the link. A link lost for good stops the flow and the
   world, and the worker exits 1.

## Link frames

One JSON object per WebSocket text message, tagged by `type` (`src/worker/frames.rs`).

| Direction | Frame | Meaning |
|---|---|---|
| worker → control | `hello {ticket, resume_from}` | First frame. `ticket` is a ticket `sdt_` or, on a reconnect, the worker token `sdw_`. `resume_from` is the last `n` acked. |
| worker → control | `event {n, agent, kind, payload, overhead}` | One event. `n` is the worker's counter from 1. |
| worker → control | `ended {status, error}` | How the branch ended. Sent only after every event is acked. |
| control → worker | `start {plan, token, gateway}` | The `BranchPlan`, the worker token, the control box's gateway base. |
| control → worker | `ack {n, seq}` | Event `n` is recorded as `seq`. |
| control → worker | `pause`, `resume`, `stop` | A hold pressed in the UI or by the control box. |
| control → worker | `over_budget {why}` | The spend estimate on the control box passed the limit. The worker stops, the branch ends `budget_exceeded`. |
| control → worker | `refused {why}` | The control box closes the link after this. |

A frame over `MAX_LINK_FRAME` (8 MiB) closes the link with no reason given. So `LinkRecords` cuts an event
whose frame would pass `MAX_EVENT_FRAME` (8 MiB less 64 KiB): long strings are cut to 16 KiB, and a payload of
many short strings is replaced by a summary (`event_frame`). What the worker keeps is what the control box
records.

## The window, resends and give-up

```mermaid
stateDiagram-v2
    [*] --> Linked: Start received
    Linked --> Linked: send while fewer than 64 unacked, ack frees a slot
    Linked --> Down: connection dropped or closed
    Down --> Rejoining: first try at once, then every 2 s
    Rejoining --> Linked: hello with sdw_ and resume_from, resend every unacked event in n order
    Rejoining --> Down: connect failed
    Down --> GaveUp: down for 5 min
    GaveUp --> [*]: stop the world, exit 1
    Linked --> [*]: Ended sent, then refused as ended
```

- **Window.** At most `WINDOW` (64) events are on the wire unacknowledged. The rest wait in the outbox
  (`src/worker/link_records.rs`). An event is recorded on the worker only once its ack brings the seq, so
  `append` returns the control box's seq.
- **When the link counts as up again.** An ack, a pause, resume or stop, or a ping while nothing waits on the
  control box (`outbox.owes()` false). A connection that is accepted and then closed, or that pings but never
  acks, does not reset the down clock (`src/worker/client.rs`, `heard`). Tests:
  `a_link_that_is_accepted_then_closed_still_gives_up_and_does_not_spin`,
  `a_link_that_pings_but_never_acks_still_gives_up` in `tests/worker_run.rs`.
- **Give-up.** After `GIVE_UP` (5 min) down, the worker stops its world and exits 1.
  `SHAKEDOWN_WORKER_RETRY_MS` and `SHAKEDOWN_WORKER_GIVE_UP_MS` override both for tests. On the control box the
  branch then stays `running` until a stop, or until the sweep finds its worker token dead (a day after it was
  made). Ending it sooner is the sandbox reconciler's job (M2a Task 8, not built).
- **Idempotent resends.** The control box acks a resent `n` again with the seq it got: from `Link.appended`
  (the last 64) or from the store (`append_linked` finds the row by `(branch_id, link_n)`). A gap (`n` past
  `last_n + 1`) is refused. Tests: `a_dropped_connection_loses_and_doubles_nothing`,
  `a_restarted_control_box_rejoins_without_doubling` in `tests/worker_link.rs`.

## The E2B backend

`E2bBackend::new(api_key, api)` (`src/hosting/e2b.rs`, `api` is `https://api.e2b.app`) implements
`SandboxBackend` over E2B's REST API and each sandbox's envd. Endpoints as in E2B's `spec/openapi.yml` and
`spec/envd/envd.yaml`:

| Call | Request | Answer |
|---|---|---|
| `create` | `POST {api}/sandboxes`, `X-API-KEY`, body below | 201 `{sandboxID, envdAccessToken, domain}` |
| `write_file` | `POST https://49983-{sandboxID}.{domain}/files?path=<path>&username=root`, `X-Access-Token`, multipart `file` | 200 |
| `kill` | `DELETE {api}/sandboxes/{id}`, `X-API-KEY` | 204; a 404 counts as killed |
| `state` | `GET {api}/sandboxes/{id}`, `X-API-KEY` | 200 `state: running` is `Running`; 404 or any other state is `Gone` |

The create body:

```json
{"templateID": "<spec.template>", "timeout": <spec.timeout in s>, "secure": true,
 "metadata": {"...": "..."},
 "network": {"allowOut": ["<spec.allow_out>"], "denyOut": ["0.0.0.0/0"], "allowPublicTraffic": false}}
```

- **Egress.** Everything is denied but `allow_out`: the control box's link host and the hosts the agent setup
  allows. With `allowPublicTraffic: false`, E2B's spec says sandbox URLs take no traffic without the
  sandbox's traffic token (not tested here; envd still answers with its access token, checked live).
- **Keys.** The E2B key goes only to E2B's API, never to envd or into a sandbox
  (`write_file_sends_the_access_token` checks the envd request carries no `X-API-KEY`). The envd access token
  of each sandbox is kept in memory only, by sandbox id; a control box that restarts cannot write into the
  sandboxes it made before (Task 8 keeps it sealed with the vault key).
- **Domain.** When E2B names no `domain`, envd is on `e2b.app`.
- **Files** are written as root, so `TICKET_PATH` under `/run` is writable; envd makes the folders.
- **Errors** name the call, E2B's status and up to 500 characters of its answer.
- `with_envd(base)` sends every envd call to one base instead of the sandbox's host (the unit tests' mock).
- `read_file` reads a file back (the live check).

## The control box's end

`src/worker/link.rs`:

- **Hello.** It must come within `link_hello_timeout` (10 s). A ticket is taken only while its branch's link
  waits for a worker (`take_ticket`). Of two tickets for one branch only one makes a token (`start_for`). When
  the token cannot be minted, the claim is let go and the ticket given back.
- **Rejoin.** A worker token resolves through `run_tokens::worker_caller(WorkerLink)`. The branch must still be
  `running`, and `resume_from` must not pass the last event appended.
- **Rebuild after a restart.** When this control box has no `Link` for the branch, `rebuilt` asks
  `layer.authenticate` who the token acts for, then, under the branch lock, reads the last `link_n` and what
  the branch is held to (`on_record`). A spend limit recorded reached whose hold was lost is put back on
  `budget_exceeded`. A stopping branch has its end grace armed again.
- **Keepalive.** The control box pings every third of `link_idle_timeout` (60 s) and closes a link silent for
  all of it.
- **Ended.** Under the branch lock: read the hold. If the branch is stopping, the hold's status wins over the
  reported one. Then `holds::end_locked`: status set, tokens revoked, link forgotten.

## Holds

```mermaid
sequenceDiagram
    participant UI as API (api/flow.rs) or gateway settle
    participant HD as worker/holds.rs
    participant DB as branches.hold
    participant LK as Links
    participant WK as worker
    UI->>HD: put(caller, branch, hold, why)
    HD->>HD: keep: take the branch lock
    HD->>DB: set_hold (resume also extends tokens by the pause)
    HD->>LK: set_hold
    HD->>HD: tell
    alt a worker is connected
        HD->>WK: pause, resume, stop or over_budget
        HD->>HD: a stop arms the end after link_end_grace (30 s)
    else no worker connected
        HD->>HD: a stop ends the branch at once (end)
    end
```

- A hold is kept in the store first, so a restart keeps it, then on the link, both under the branch lock
  (`keep`).
- The gateway enforces the hold on every model call whether a worker is connected or not
  (`Linked::held_back`): paused is 409 with `x-shakedown-held: paused`, stopping is 409, stopping for the
  limit is 402.
- The worker's gateway answers a 409 `held: paused` by polling every 200 ms and sending the call again, so a
  pause it has not heard of yet holds the call rather than failing it (`src/gateway/forward.rs`).
- A resume extends the worker token by how long the pause lasted. A token's life counts only unpaused time
  (`token_live!`).

## Budgets on a linked branch

The control box keeps the run's spend estimate in `Link.budget`, from the rules it planned the branch with.

| Step | Where |
|---|---|
| Rules not on record (a branch from before 0005): every call is 503 `NO_LIMITS`, never unlimited. | `src/gateway/linked.rs`, `open` |
| A model with no provider cost and no price in the table is refused 400. | `src/gateway/linked.rs`, `unpriced` |
| A call that answered with no cost, or a cut stream, is charged a ceiling from the highest prices the table lists for its provider. When there is none, a run with a limit is forced over it. | `src/gateway/charge.rs`, `linked` |
| What was charged is kept on the event (`charged_usd`, `estimated`), so a restarted control box rebuilds the estimate. | `src/gateway/book.rs`, `bill` |
| As soon as a charge puts the run over, `settle` keeps the hold `budget_exceeded`, records `budget/limit_reached` at the branch's last world clock, and tells the worker. | `src/gateway/linked.rs`, `settle` |
| The worker's own estimate comes first for a well-behaved worker, never instead. A control-box 402 forces the worker over. | `src/gateway/forward.rs`, `over_budget` |
| At most 4 model calls in flight per agent. A call waits up to 10 s for a permit, then 429. | `src/worker/links.rs`, `INFLIGHT_PER_AGENT`. `src/gateway/linked.rs`, `in_flight` |

## The sweep

`holds::sweep_every` runs at start and every `link_sweep_every` (60 s), spawned by `serve::run`. For every
running branch that ever had a worker token (`jobs::linked_branches`):

- paused: left alone (its resume extends the token)
- stopping: the end grace is armed again (a restart lost the timer)
- otherwise, worker token dead: ended `infra_error` with `TOKEN_DIED`, only if it is still unpaused and no
  worker token is live when it ends (`EndIf::TokenDied`, checked in the same statement).

At start, `jobs::interrupt_at_start` leaves a branch with a live worker token `running`, so its worker can
reconnect.

## Invariants

| Invariant | Enforced in |
|---|---|
| The sandbox holds no key but its worker token. | `src/layer.rs`, `WorkerLayer`. `src/worker/run.rs`, `worker_app`. `src/gateway/forward.rs`, `onward`. `tests/worker_run.rs`, `a_workers_model_calls_carry_no_model_key_and_are_recorded_once` |
| A worker token opens only its own branch, and a worker's own gateway refuses one (401). | `src/auth/can.rs`, `in_run`. `src/gateway/target.rs`, `resolve`. `tests/gateway_passthrough.rs`, `a_worker_token_reaches_only_its_branch`, `a_worker_token_at_a_workers_own_gateway_is_refused` |
| `seq` is assigned only on the control box, under the same lock as its own events. | `src/store/linked.rs`, `append_linked`. `src/gateway/target.rs`, `CallFor::append` |
| An event is appended once per `n`, also across reconnects and restarts. | `core/crates/shakedown-schema/migrations/shakedown/0004_event_link_n.sql`. `src/worker/links.rs`, `Appended::resent` |
| An ended branch takes no more events. | `src/store/linked.rs` (`HAVING EXISTS … status = 'running'`) |
| The rules, the hold and the spend estimate are the control box's, and survive a restart. | `src/store/control.rs`. `src/worker/link.rs`, `on_record`. `tests/gateway_passthrough.rs`, `a_restarted_control_box_keeps_the_limit_and_the_spend`, `a_worker_written_limit_counts_for_nothing` |
| A stop pressed on the control box ends the branch with the hold's status, whatever the worker reports. | `src/worker/link.rs`, `ended`. `src/worker/holds.rs`, `tell`, `arm` |
| Every end holds the branch lock over the status and the revoke, so no worker event lands after it. | `src/worker/holds.rs`, `end`, `end_locked` |
| One `model_call` per call, recorded on the control box. The worker records none. | `src/gateway/forward.rs`. `src/agents/claude_session.rs` |

## Extension points

- **A sandbox backend.** Implement `SandboxBackend` (`src/hosting/mod.rs`) as `E2bBackend` does. The planned
  `Hosting` (M2a Task 7) creates the sandbox, calls `links.expect(plan)`, issues a ticket and writes it to
  `TICKET_PATH`. Nothing in the link changes for it.
- **A new control frame.** Add it to `ToWorker`, handle it in `worker::run::obey` and pin its JSON in
  `src/worker/frames.rs` tests.

## Tests

| Test | What it covers |
|---|---|
| `tests/worker_link.rs` | Tickets, order, dropped connections, control-box restarts, holds before start, silent links, ended branches. |
| `tests/worker_run.rs` | A real `shaked worker` end to end on the toy world, rejoin, cut-down events, give-up, stop, model calls without keys. |
| `tests/gateway_passthrough.rs` | Linked calls: worker tokens, limits, holds, restarts, unpriced models. |
| `tests/run_tokens.rs` | Worker and ticket tokens, a worker token needing its sandbox, pauses extending tokens. |
| `src/worker/link_records.rs`, `src/worker/frames.rs` unit tests | The window, resends in order, the end after every ack, frame cutting, frame JSON. |
| `tests/support/link.rs` | Shared link plumbing: `ticketed`, `started`, `restarted`, `hear`. |
| `src/hosting/e2b.rs` unit tests | The E2B calls against a local mock of E2B's API and envd: the create body, 404s on kill and state, the envd token and no E2B key on a write. |
| `tests/hosting.rs` | `live_e2b_create_write_kill` (`#[ignore]`, needs `E2B_API_KEY`): a real sandbox from E2B's `base` template is running, takes a ticket at `TICKET_PATH` and reads it back, is killed, then is `Gone`. Its timeout is 2 minutes in case the kill fails. |
