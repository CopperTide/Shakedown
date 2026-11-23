# RunPod pod for a self-hosted model

`gpupod` runs gpt-oss-120b (vLLM v0.32.0, with its gpt-oss tool and reasoning parsers) on one RunPod GPU, so
shaked can use it as the local model provider `local:runpod`. It was built for the
[software-factory designs](../../designs/software-factory) comparison. It keeps the cloud spend inside the budget
in `pod.toml` (`budget_usd = 50.0`).

## What it needs

- [uv](https://docs.astral.sh/uv/) and `runpodctl` 3.0.1, signed in to a RunPod account. `runpodctl` is the only
  way this tool talks to RunPod.
- A shaked at `SHAKEDOWN_URL` (default `http://127.0.0.1:4510`) for `smoke --through-shaked` and for the
  watchdog's "busy" check.

## Use

```bash
uv run --project deploy/runpod gpupod quote                       # the rate now and the spend so far
uv run --project deploy/runpod gpupod start --confirm-rate 2.69   # [--hours 8] [--gpu ID] [--low-memory]
uv run --project deploy/runpod gpupod smoke [--through-shaked http://127.0.0.1:4510]
uv run --project deploy/runpod gpupod status | stop | delete [--force] | reconcile
```

`start` writes the settings shaked needs to `~/.config/shakedown/runpod.env`. Put them in shaked's environment
before you start it, for example with `set -a; . ~/.config/shakedown/runpod.env; set +a`. The option
`--shaked <url>`, given before the command (default `SHAKEDOWN_URL`, else `http://127.0.0.1:4510`), names the
shaked the watchdog and `delete` ask whether a run is running.

- **Every start prints the GPU's hourly rate first** (from `runpodctl gpu list`). It refuses when the confirmed
  rate differs from it by more than $0.005, when the GPU has no stock, or when the ledger's spend plus the rate
  times the planned hours would pass the budget. Out of stock, it names the next GPU in `pod.toml`'s order with
  its rate and never starts that GPU by itself. It warns when the account balance is above what is left of the
  budget: with auto-reload off, that balance is the one stop that works if this machine dies with a pod up.
- **Start** reuses the stopped pod of the open session (its volume keeps the weights) or creates one with a new
  `VLLM_API_KEY`. It writes `~/.config/shakedown/runpod.env` (mode 0600: `SHAKEDOWN_LOCAL_MODELS`,
  `SHAKEDOWN_LOCAL_KEY_RUNPOD`, `SHAKEDOWN_LOCAL_PRICE_RUNPOD`), starts the watchdog detached (pid and log in
  `~/.local/state/shakedown/`), and waits up to 20 minutes for vLLM's `/health`. A pod that never gets healthy is
  stopped.
- **The watchdog** probes every minute. It stops the pod after `idle_minutes` idle, after `max_session_hours`,
  when the spend would pass the budget within ten minutes, or after ten unhealthy minutes in a row. Busy means
  vLLM is running or has finished a request since the last probe, or any run in shaked is running.
- **The ledger** (`ledger.jsonl`, committed) has one line per quote, start, ready, measure, stop, delete,
  reconcile and OpenRouter spend. It refuses any field named like a key. Spend per pod is its latest billed
  total (`reconcile`, from `runpodctl billing pods`) plus its estimated time since, else start-to-stop hours
  times the rate.
- **The smoke test** checks `/health`, that `/v1` needs the key, a `get_time` tool call with non-empty
  `message.reasoning`, an answer after the tool result, and the decode speed (recorded as `measure`). With
  `--through-shaked` it makes one gateway call in an external run and checks that the run recorded the
  reasoning with its model call's seq. That run is stopped unscored and deleted.

## Tests

```bash
cd deploy/runpod && uv run --with pytest pytest -q
```

The tests run against a fake `runpodctl` whose answers are hand-written in the shapes RunPod's docs give
([tests/fixtures/README.md](tests/fixtures/README.md) names each source), and against a fake vLLM and a fake
shaked. They passed on 2026-11-19 on Linux. The `pod create` flags were read from `runpodctl help pod create`
(3.0.1).

## Limits

Nothing here has run against a RunPod account: no pod was created, and no RunPod API was called.
