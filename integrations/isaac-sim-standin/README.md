# Factory plant on the stand-in (`isaac-sim-standin`)

Puts the press plant of Ridgeline, a fictional manufacturer, in a Shakedown world without Isaac Sim and without a
GPU. It is the [`isaac-sim`](../isaac-sim/README.md) program on its `standin` backend: the Ridgeline factory
example's `Factory`, with the example's offline press model (`tests/fake_sim.py`, `tests/press_dynamics.py`) in
place of Isaac Sim's physics. The [`factory`](../../benchmarks/factory/README.md) benchmark uses it by default
(`shakedown-factory --backend standin`). This package is a manifest only. Its code, config, tools, keys and saves
are those of `isaac-sim`.

| | |
|---|---|
| Program | `python -m isaac_part --backend standin` (Python, run with `uv` from `../isaac-sim`) |
| Restore | exact by replay, from the seed and the inputs. Approximate from a save (a pickle of the plant). |
| World clock | used: physics runs in fixed steps of `1/physics_hz` s of world time |
| Agent tools | those of `isaac-sim`: `plant_state`, `machine_telemetry`, `vibration_spectrum`, `recent_events`, `wait_for_event`, `predict_move`, `dispatch_technician`, `set_line_status`, `safe_stop_cell`, `start_cell`, and the world tools `add_shipment`, `show_status`, `plant_scenario` |

## What it needs

- `uv` and the `isaac-sim` package folder beside this one (the manifest runs `uv run --directory ../isaac-sim`).
- **The Ridgeline factory example, which is not public.** By default `CopperTide-Examples/factory/isaac` in the
  folder that holds the repository folder. Without it the part can't start. See `isaac-sim`, "What it needs".
- No GPU, no Isaac Sim and no services.

## Use it

A world names it as a part with the same config as `isaac-sim` (see its "Config"), for example the `factory`
benchmark's `{part: "plant", integration: "isaac-sim-standin", config: <shift.json's plant>}`. `record` is
refused (`-32602`): the stand-in can't render. `scene` is accepted and changes nothing without a recording.

The stand-in integrates the same press bodies and numbers as the Isaac Sim scene, in plain Python. It is a model
of the physics for tests and runs without a GPU, not an Isaac Sim run, and its readings are not equal to Isaac
Sim's.

## How it's tested

```bash
cd integrations/isaac-sim
uv run --with pytest --with asyncssh pytest -q
```

The `isaac-sim` suite runs every plant test on this backend, including a `shakedown-part-check` conformance run on
a copy of the manifest with `--backend standin` (see `isaac-sim`, "How it's tested"). Every test is skipped
without the private factory example.

## Limits

- It can't run without the private Ridgeline factory example.
- No recording. The plant's numbers are the offline model's, not Isaac Sim's.
- Everything in `isaac-sim`'s "Limits" that is not about Isaac Sim applies here too.
