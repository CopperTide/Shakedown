# Isaac Sim plant mirror (`isaac-sim-mirror`)

Follows a Ridgeline factory plant that is already running, and puts its events and state in a Shakedown world.
It makes no plant, gives agents no tools, and only ever sends GET requests, so it never changes the plant. It is
for watching a live plant from a world. Ridgeline is a fictional manufacturer, and the plant is the one in the
Ridgeline factory example. This package is a manifest only: it runs the [`isaac-sim`](../isaac-sim/README.md)
program with `--mode mirror`, whose code is `integrations/isaac-sim/isaac_part/mirror.py`.

| | |
|---|---|
| Program | `python -m isaac_part --mode mirror` (Python, run with `uv` from `../isaac-sim`) |
| Restore | none. Exact `none`, approximate no: a live plant can't be rewound. |
| World clock | used: each `advance` reads the plant's new events |
| Agent tools | none |

## What it needs

- `uv` and the `isaac-sim` package folder beside this one (the manifest runs `uv run --directory ../isaac-sim`).
- A running plant that serves the factory example's HTTP API (`GET /state`, `GET /events`), by default on
  `http://127.0.0.1:8090`. That plant is the example's own app (`isaac/run.py`, on Isaac Sim). The example is not
  public, so without it there is nothing to follow.
- No GPU of its own, and no environment variables of its own.

## Use it

A world names it as a part, for example `{part: "plant", integration: "isaac-sim-mirror", config: {plant_url: ...}}`.

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `plant_url` | string | `http://127.0.0.1:8090` | The plant's HTTP API. Must be `http` or `https`. |

- `start` reads `GET /state` and follows from its `log_seq`. Events from before the start are the plant's history,
  not this world's.
- `advance` returns `GET /events?since=<last seq>&limit=500` as world events, `{kind, text, data: {source, seq,
  t, detail}}`, as the `isaac-sim` plant gives them. A reply that isn't a list of events, such as `{"error": ...}`,
  is `-32603`. So is a plant that doesn't answer within 10 s.
- `inspect` returns pieces of `GET /state` under the `isaac-sim` keys (`machine/<id>`, `line/<id>`,
  `plant/clock`, `plant/sim` and the rest).
- `save` writes the current `/state` as documents for display (`exact: false`). Nothing can start from it: a
  start `from` a save is `-32001`. `shaked` refuses to fork or rerun a world that contains the mirror, in either
  mode (`core/crates/shaked/src/restore.rs`).
- Any tool call is `-32602`.

## How it's tested

```bash
cd integrations/isaac-sim
uv run --with pytest pytest -q tests/test_part.py -k mirror
```

`test_mirror_mode_has_no_tools_and_relays_events` runs the mirror against a stub HTTP server on a free port: no
tools, no restore, events before the start left out, events relayed with their data, `inspect` of the live plant
and of a save, a refused start from a save, GET requests only, and an error reply. The test is in the `isaac-sim`
suite, which is skipped without the private factory example. The manifest has no `[[example]]`, so
`shakedown-part-check` can't run on it.

## Limits

- It has never followed a real plant, on Isaac Sim or otherwise. Only the stub server has been tested.
- No restore of any kind, so a world that contains it can't be forked or rerun.
- It shows what the plant reports when `advance` runs. It does not keep the plant in step with the world clock.
