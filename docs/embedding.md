# Embedding shaked

shaked is a library as well as a program: `shaked::serve::run(store, opts)` starts the same server the
`shaked` binary starts, over a `Store` that carries a **layer**. The layer is where the engine asks its
deployment; the open-source binary passes `shaked::layer::LocalLayer`, which answers every hook for one person
at one machine. A hosting service implements the same trait (`shaked::layer::Layer`) and starts shaked with
its own.

```rust
let opts = shaked::serve::ServeOptions::from_env(my_layer.policy()).await?;
let store = shaked::store::Store::open(&shaked::pg::Admin::lazy(&opts.pg_url)).await?;
shaked::serve::run(store.with_layer(std::sync::Arc::new(my_layer)), opts).await
```

A service that runs each branch in a sandbox of its own sets `opts.hosting` to a
`shaked::hosting::Hosting` over a `SandboxBackend` (E2B's: `shaked::hosting::e2b::E2bBackend`) before
`run`; `from_env` never sets it. Such a control box refuses runs an external system or a Code connection
drives, runs one sandbox per `object_prefix` scope at a time and sweeps its sandboxes every minute
([sandbox runs](internals/sandbox-runs.md)).

## The hooks

| Hook | When the engine calls it | `LocalLayer` |
|---|---|---|
| `policy()` | at start and wherever a rule depends on the deployment: `serves_others` (a bind beyond loopback is allowed), `private_model_urls` (model servers on private addresses may be used), `runs_packages_here` (worlds and code connections run on this machine) | `false`, `true`, `true` |
| `authenticate(req, found)` | every `/api` request but `GET /api/health`, after the engine resolved any Bearer token (`sd_`, `sdr_`, `sdw_`) to `found`; for a layer that serves others, `req`'s extensions hold the body's `BodySha256` (bodies over 2 MiB are refused 413 first) | `found`, else the person at this machine |
| `begin(tx, for)` | right after `BEGIN` of every transaction, with its caller or system job | nothing |
| `authorize(caller, action, target)` | before every route's handler, after the engine's own checks (token scope, run-token reach) | allow |
| `admit(caller, costly)` | before a run (however it starts: `POST /api/runs`, a suite, an experiment, a suite's claim), a fork, a model call or a stored save | go |
| `meter(caller, usage)` | after each run created and each recorded model call (idempotent on its key) | nothing |
| `fallback_provider(caller, want)` | when an agent's provider has no key of the caller's own nor in the environment; a lent provider's `cap` (`CallCap`) clamps each call's `max_tokens` and, on OpenRouter, sets `provider.max_price` | none |
| `audit(tx, caller, event)` | inside the transaction of each token or model-key change (never with a secret) | nothing |
| `routes()` | merged under `/api`, behind `authenticate` | none |
| `me(caller)` | `GET /api/me`: `mode`, `features` and an optional `account` menu for the UI | `local`, none, none |
| `object_prefix(caller)` | the first segment of every object key the caller's records name | `local` |

`shaked::harness` (feature `harness`) has what an embedder's tests need: the route table (`ROUTES`, each route's
action and target), `SpyLayer` (records every hook call, answers set per test), `scratch_db` and `serve_app`.

Open-source shaked is for personal use, one person at one machine. A hosting layer keeps what it adds in its
own schema streams on top of the base `shakedown` stream, which it may extend but never change.
