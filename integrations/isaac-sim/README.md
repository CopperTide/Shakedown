# Isaac Sim factory plant (`isaac-sim`)

Puts the press plant of Ridgeline, a fictional manufacturer, in a Shakedown world, on the world clock. It wraps the
`Factory` class of the Ridgeline factory example (`isaac/factory.py`) without editing it, and gives agents the
plant floor's tools: telemetry, vibration spectra, events, technicians, line status and robot cells. The physics
runs on NVIDIA Isaac Sim 6.2 (the `isaac` backend) or on the example's offline stand-in, which needs no GPU (the
`standin` backend, package [`isaac-sim-standin`](../isaac-sim-standin/README.md)). The
[`factory`](../../benchmarks/factory/README.md) benchmark uses it as its part `plant`, to test agent teams that
handle a press bearing failure and a disrupted shift.

| | |
|---|---|
| Program | `python -m isaac_part --backend isaac` (Python, run with `uv`, or with Isaac Sim's own Python on a GPU machine) |
| Restore | exact by replay, from the seed and the inputs. Approximate from a save (a pickle of the plant). |
| World clock | used: physics runs in fixed steps of `1/physics_hz` s of world time |
| Agent tools | `plant_state`, `machine_telemetry`, `vibration_spectrum`, `recent_events`, `wait_for_event`, `predict_move`, `dispatch_technician`, `set_line_status`, `safe_stop_cell`, `start_cell`. Three tools are for a benchmark's setup or links: `add_shipment`, `show_status`, `plant_scenario`. |

## What it needs

- **The Ridgeline factory example, which is not public.** The part imports the example's plain-Python modules
  (`factory.py`, `press_model.py`, `signals.py`, `paths.py`, `schedule.py`, `layout.py`) and its data
  (`data/ridgeline.json`). By default it looks in `CopperTide-Examples/factory/isaac`, in the folder that holds the
  repository folder (`../CopperTide-Examples` from the repository root), branch `isaac-plant`. That repository is
  private. Without it the part can't start, every test is skipped, and the `factory` benchmark's world can't
  start. Outside contributors can read and change this code, but can't run it.
- `uv` and Python 3.10 or later. Dependencies: the Python kit (`kits/python`) and `numpy`.
- For the `standin` backend: nothing more. No GPU and no services.
- For the `isaac` backend:
  - an NVIDIA GPU (the manifest says `needs.gpu_vram_gb = 16`) and Isaac Sim 6.2, so that `isaacsim` is
    importable. The manifest's command runs under `uv`, where `isaacsim` is importable only if it is installed
    in that environment (not verified). Every recorded Isaac Sim run used Isaac Sim's own Python
    (`/isaac-sim/python.sh` in the `nvcr.io/nvidia/isaac-sim:6.2.0` image), started over ssh (see "Running on a GPU
    machine").
  - the example's `sim_io.py` and `ridgeline_scene.py` as well.
  - for a recording: the example's `record.py` and `visuals.py`, an `ffmpeg` (the Isaac Sim image has none:
    `imageio-ffmpeg` on `PYTHONPATH` gives the recorder one, else it writes PNG frames), and for
    `scene: showcase`, Isaac Sim's asset server (by default NVIDIA's public one).

| Variable | Default | Purpose |
|---|---|---|
| `SHAKEDOWN_SAVES` | set by `shaked` | Saves go to `$SHAKEDOWN_SAVES/isaac-sim/<save>/`, recordings under `$SHAKEDOWN_SAVES/isaac-sim/` |
| `SHAKEDOWN_RUN_ISAAC_SIM` | none | In `shaked`'s environment: a command line that replaces the package's run command, for a plant on another machine |
| `SHAKEDOWN_BIN_DIR` | none | Tests only: where to find `shakedown-part-check` (else `core/target/debug` or `release`) |

## Use it

A benchmark or world names it as a part. The `factory` benchmark names the plant
`{part: "plant", integration: "isaac-sim-standin" | "isaac-sim", config: {...}}`, picked by
`shakedown-factory --backend standin|isaac`, and gives it robot cell R1 and the load model from
`benchmarks/factory/data/shift.json`.

### Backends

The backend is a process argument, `--backend isaac|standin` (default `isaac`), because `describe` has no config
and the backend changes the fingerprint.

| Backend | Physics | Package |
|---|---|---|
| `isaac` | The example's `sim_io.IsaacIO` on a headless Isaac Sim, set up as the example's `run.py` does: build the scene, `setup_physics` on `cuda:0`, play, `start()`. PhysX steps run without rendering, and `Factory.physics_step` runs in the post-step callback. | `isaac-sim` |
| `standin` | The example's test double `tests/fake_sim.FakeIO`, backed by `tests/press_dynamics.py`. Each step is `io.step(dt)`, then `Factory.physics_step(dt)`. | [`isaac-sim-standin`](../isaac-sim-standin/README.md) |

- If `isaacsim` can't be imported, `describe` still answers and `start` fails with `-32603` ("Isaac Sim is not
  importable here ...").
- Isaac Sim has one stage per process, so `isaac` holds one live plant per process. A second `start` is refused
  with `-32603` until the first instance stops. One plant after another (`shaked`'s base, then a branch) works:
  stopping a plant points its physics callback at a no-op and stops the timeline, and the next plant builds a new
  stage and registers its own callback.

### What Isaac Sim simulates, and what it doesn't

- **On `isaac`**, PhysX simulates each press as an articulation: the frame on a spring-damper mount (two axes), the
  shaft and flywheel on a velocity drive with a torque limit, and the ram. An IMU on the frame gives the
  accelerations that become the vibration readings and spectra. Robots and people are placed and driven by the
  example's code.
- **The fault is modelled, not emergent.** The bearing defect and floor noise are forces the example's Python
  press model (`press_model.py`) applies to the frame each step, with a severity that the scenario makes grow.
  PhysX integrates their effect. Spindle temperature is modelled, not simulated.
- **On `standin`**, `press_dynamics.py` integrates the same bodies and numbers in plain Python (semi-implicit
  Euler). It is the example's own check of the tuning, not an Isaac Sim run. The readings are close in kind but
  not equal to Isaac Sim's.

### Config

| Config key | Type | Default | Meaning |
|---|---|---|---|
| `isaac_path` | string | `../CopperTide-Examples/factory/isaac` from the repository root | The example's `isaac` folder |
| `data_path` | string | `<isaac_path>/../data/ridgeline.json` | The plant data, passed to `press_model.load_data` |
| `plant_start` | string | `2026-03-10T06:00:00` | The plant clock at world time 0 |
| `physics_hz` | integer | `press_model.PHYSICS_HZ` (360) | Physics steps per second. The example's signal chain assumes 360, so another rate changes the readings. |
| `scenario` | boolean | `true` | `start` begins PR-A1's bearing wear at once. `false`: a plain reset. |
| `cells` | array | none | Robot cells: `[{id, name, rate_per_hour, running_kw, idle_kw, line?, zone?, robot?}]`, each running at the start |
| `power` | object | none | The load model: `{meter?, base_kw, lines: {<line>: {running_kw, idle_kw}}}` |
| `record` | string | none | An MP4 path, relative to `$SHAKEDOWN_SAVES/isaac-sim/`. `isaac` only. |
| `camera` | string | `auto` | `auto` (cuts to events), `free`, or a named shot |
| `record_fps` | integer | 30 | Frames per second of world time. The MP4 plays at this rate, so in world time. |
| `record_height` | integer | 720 | The MP4's height. The width follows at 16:9. |
| `scene` | string | `ridgeline` | `ridgeline`: the example's own scene. `showcase`: a dressed stamping plant. Visuals only. |
| `render` | object | `{}` | Renderer carb settings (`/rtx/...` paths to values), for example the path tracer |

**Loading the example.** The modules are imported with `isaac_path` first on `sys.path` and bytecode writing off,
so nothing is written into the example's folder. Modules are process-global, so one process serves one
`isaac_path`. A missing file makes `start` fail with `-32602`, naming it.

**Fingerprint.** `describe` declares:

- the sha256 of the example's files at the default paths: the common modules and `ridgeline.json`, plus
  `tests/fake_sim.py` and `tests/press_dynamics.py` for `standin`, or `sim_io.py` and `ridgeline_scene.py` for
  `isaac`.
- `adapter_sha256`, the sha256 of this package's own `isaac_part/*.py`.
- `backend`, `adapter` and the default `physics_hz`.
- for `isaac`, `{gpu, driver}`, read from `/proc/driver/nvidia/version` and `/proc/driver/nvidia/gpus/*/information`.
  Only when those are absent does it run `nvidia-smi`. If that fails or takes over 10 s, `describe` fails with
  `-32603` rather than record a null. Null means there is no NVIDIA driver at all.

`start` refuses configured files that differ from those hashes with `-32003`, naming the file. So `isaac_path` and
`data_path` may point at another copy only if it is byte for byte the same. The Isaac Sim version that ran is in
`inspect`, as `plant/sim` (`isaac_version`). The stand-in reports `"test double"`.

### Tools

The names and arguments are those of the example's `mcp/src/ridgeline_mcp/floor.py`. Each tool calls the
`Factory` method that the example's `api.py` route calls, directly, not over HTTP.

| Tool | Effect | Factory call | Keys |
|---|---|---|---|
| `plant_state` | read | `state()`, the keys `floor.py` keeps, plus `cells` and `power` | every document key |
| `machine_telemetry {machine, minutes=30}` | read | `telemetry`: 1 Hz readings | `machine/<id>` |
| `vibration_spectrum {machine, seconds=20, before_trip=true}` | read | `spectrum`: FFT of the frame's IMU | `machine/<id>` |
| `recent_events {since=0, limit=60}` | read | `events_since` | none (the log is not keyed) |
| `wait_for_event {kinds, since, machine, timeout_s}` | read | `events_since` | none |
| `predict_move {order, to_line}` | read | `whatif_move` | `order/<id>`, `line/<id>` |
| `dispatch_technician {proposal_id, technician, machine, task}` | write | `dispatch_technician` (`task`: `inspect` or `repair`) | `technician/<id>`, `machine/<id>` |
| `set_line_status {line, status, rate?}` | write | `set_line` (`rate` is the line's `boost`, 0.5 to 1.25) | `line/<id>` |
| `safe_stop_cell {cell, reason?}` | write | this package: the robot parks, its safety PLC holds | `cell/<id>` |
| `start_cell {cell}` | write | this package | `cell/<id>` |
| `add_shipment {part, origin, carrier, transit_hours, ref?}` | write | `add_shipment`: for a benchmark's link from the hub's booking | `shipment/<id>` |
| `show_status {key, value, number?}` | write | this package: what the showcase's boards show (`dr`, `ot`, `truck`), for a benchmark's links | `_signage/<key>` (internal) |
| `plant_scenario {name}` | write | `press-wear` starts PR-A1's wear now. `calm` resets with the run's seed, so no press wears. For a task's setup. | `plant/clock` |

- `wait_for_event` answers at once, with `timed_out` when nothing matches yet. Inside a call the world clock stands
  still, so a wait would never end. `timeout_s` is accepted and ignored.
- `proposal_id` is accepted and not checked here. A benchmark can gate the tool on an approvals part
  ([`docs/protocol.md`](../../docs/protocol.md), "Gates on tools").
- The plant runs without ERPNext, so it has no Work Orders: `orders` is empty and `predict_move` always refuses.
  Records the example would file in ERPNext (lockouts, statements, inspection notes) come out as events saying
  they were not filed. That is the example's `--no-erp` behaviour.
- Refusals are a tool-level `error` and change nothing: an unknown machine, technician, line, order or cell
  (checked before the `Factory` is called), a `task` other than `inspect` or `repair`, a bad argument type, and the
  `Factory`'s own `ValueError`s (a busy technician, an unknown line status). An unknown tool name is `-32602`.

### Keys, inspect and diff

- Keys come from `state()`: `machine/<id>`, `line/<id>`, `order/<id>`, `technician/<id>`, `operator/<id>`,
  `shipment/<id>`, `robot/<id>`, `incident/<id>` and `cell/<id>`. Also `plant/clock` (`t`, `clock`, `speed`,
  `scenario`, `log_seq`), `plant/sim` and, with `power`, `plant/power` (`{meter, kw, reported_kw}`).
- `diff` compares documents exactly, except floats inside `sensors` and `pose`. Those count as the same within
  0.05 plus 1% of the larger value, because physics readings drift after an approximate restore.

### The world clock

- The example is never edited. The part replaces two names on its imported `factory` module, `time` and
  `datetime`, with a clock whose `time()` and `now()` read `plant_start` plus world time. So the plant clock,
  promised dates and each event's time are the same in every run.
- The part does not connect ERPNext or start the example's threads. It calls `Factory` methods directly, under
  the factory's lock, then drains its inbox.
- `advance(clock_ms)` runs fixed steps up to `clock_ms`, counted from world time 0, so how the advances are split
  doesn't matter. It returns the factory's events since the last `advance` as `{kind, text, data}`, with `data`
  `{source, seq, t, detail}` (`t` is plant time, `detail` the factory event's own data).
- `call_tool` first steps to its `clock_ms`, then acts. Its events wait for the next `advance`.
- `start` calls `start_scenario(seed)` with the world seed, so the floor noise follows the seed. With
  `scenario: false` it calls `reset(seed)`.
- The example advances its plant clock by `timedelta(seconds=dt)`, which rounds 1/360 s to 2778 µs. So the plant
  clock runs 0.016% ahead of world time, as it does in the example's own app.
- If physics steps stop reaching the plant (its physics callback is not running), `advance` and `call_tool` fail
  with `-32603` rather than leave the plant standing still.

On the stand-in, with seed 7, PR-A1 entered vibration zone C at 33.7 s of world time, zone D at 46.4 s and tripped
at 61.9 s. When measured, the stand-in ran about 130 times faster than real time (120 s of physics in 0.91 s).

### Restore and saves

- **Exact by replay.** Physics state on the GPU can't be saved exactly, so the exact restore is a replay from the
  seed and the inputs. A start `from` a save in `exact` mode is refused with `-32001`.
- **Approximate saves.** A save is `plant.pkl.gz` (a gzipped pickle, protocol 5) with `docs.json` beside it for
  `inspect` and `diff`. The pickle holds every `Factory` attribute except `io`, `lock`, `cond`, `inbox`, `erp`,
  `triggers` and `log` (the list is in `isaac_part/state.py`, and a test fails if `Factory` gains an attribute
  that isn't classified), the cells, the load last reported, the seed, and the backend's state: FakeIO's presses,
  robot and people on `standin`. On `isaac`: the presses' joint positions and velocities, the robots' poses and
  joints, the people's poses and the last IMU readings.
- **Restoring** works in a fresh process: it loads the example's and the backend's modules, then unpickles. A save
  that can't be read is `-32001`, and so is one made with another backend, `physics_hz`, `plant_start`,
  `data_path`, `cells` or `power`, checked before anything is built. A missing save is `-32002`.
- On `isaac` a restore is approximate: PhysX's solver warm start and contact caches are lost, and the calls that
  set the state back (`set_dof_positions`, `set_world_poses` and the like) are marked unverified on 6.2 in the
  code.
- Each press's 120 s spectrum history (about 0.5 MB a press) is saved empty, because no tick reads it. After a
  restore, `vibration_spectrum` sees only the seconds since the restore. The pre-trip window that `before_trip`
  uses (60 s) is kept. On the stand-in everything else is in the save, so a restored plant continues as the
  original did.
- `save` always writes to `$SHAKEDOWN_SAVES/isaac-sim/<save>`, the protocol's convention, not to the `dir` it is
  given. That is the same folder when `shaked` runs the program itself, and the remote machine's folder when it
  runs elsewhere.
- **Size.** When measured, a save was 39 kB before the trip, 221 kB after it (mostly the frozen pre-trip window)
  and 250 kB at 10 minutes (the 1 Hz telemetry grows to a 2-hour cap). `shaked` saves at every step, so a run of a
  few hundred steps after the trip costs tens of MB.

### Recording and the showcase scene

- Only `isaac` renders. `record` on `standin` is `-32602`, and so is an unknown camera, checked before Isaac Sim
  starts.
- It reuses the example's `visuals.Visuals` and `record.Recorder` (a Replicator render product on the main camera,
  piped to ffmpeg), headless, with the RaytracedLighting renderer. A frame is rendered every
  `physics_hz / record_fps` physics steps, so frames fall on world time, not wall time.
- Each instance writes `<stem>-<instance>.mp4` (`shaked` runs a base, then a branch), with label textures in
  `<stem>-<instance>_labels/` beside it. The MP4 is finished on `stop` or when the process exits normally. Fetch it
  from the machine the program ran on.
- **`scene: showcase`** dresses the stage as a stamping plant before physics starts (`isaac_part/showcase_*.py`). It
  changes only what is seen: the example's press articulations, robots, people and physics stay as built, and a
  test runs the stand-in plant with and without it and compares events and documents.
  - It references assets at run time from Isaac Sim's asset server: an ArchVis industrial hall, FANUC R-2000iC/165F
    arms, conveyors, pallets, racks and a forklift, and with `cells` a UR10e welding cell and two Nova Carter AMRs
    with KLT bins. Nothing is copied into this repository. Their physics is removed, and the build fails if
    anything under `/World/Showcase` would be simulated.
  - Presses, washers, stillages, parts, stack lights and screens are generated meshes.
  - Everything moves kinematically from the plant's readings, once per frame: the slide follows the measured ram,
    the arms follow the crank (numeric inverse kinematics on the arm's own joints), parts flow on the belts and back
    up when the press stops, and the frame shakes with the measured vibration (none below 2.3 mm/s, 3 cm at the
    11.2 mm/s trip level, exaggerated on purpose).
  - `camera: auto` follows a plan: establishing shots, then cuts to PR-A1 on its zone C and zone D warnings and the
    trip, and with `cells`, to each front of the shift (the energy board, the gateway, the dock board, the cell).
- `remote/render_showcase.py` drives the part (start, advance, call_tool, stop) without `shaked`, for a video or
  stills. Run it with Isaac Sim's Python, `PYTHONPATH=<repo>/kits/python` and `SHAKEDOWN_SAVES` set:

```bash
/isaac-sim/python.sh remote/render_showcase.py --seconds 95 --out showcase.mp4
/isaac-sim/python.sh remote/render_showcase.py --seconds 3 --stills wide,press_a --out look.mp4
/isaac-sim/python.sh remote/render_showcase.py --shift --calm --seconds 40 \
  --at '10:safe_stop_cell:{"cell":"CELL-R1"}'
```

  `--shift` adds the factory benchmark's shift config, `--at "<seconds>:<tool>:<json>"` calls a plant tool at that
  world time, and `--path-traced` (with `--spp`) renders with the RTX path tracer. Its defaults are seed 7, 30 fps,
  720p and `scene: showcase`.

### Robot cells and the plant's load (`isaac_part/equipment.py`)

The example has presses only. This package adds, beside its `Factory`, what the `factory` benchmark's shift task
needs. A world that configures neither `cells` nor `power` sees the plant as before.

- A cell counts the parts it makes from the physics steps it ran (`rate_per_hour`) and gives a `cell` event when it
  stops or starts. Its document has `status` (`running` or `safe-stopped`), `safe_stopped`, `parts` and `kw`.
  `safe_stop_cell` is refused when it is safe-stopped already, `start_cell` when it runs.
- The load is `base_kw`, plus each metered line's `running_kw` times its `boost` while it runs (else its
  `idle_kw`), plus each cell's running or idle load. It is checked every 0.1 s of physics, by step count, and a
  change gives a `power` event (`detail: {kw, meter}`) that a benchmark can link to a utility's meter. The first
  check after the start reports it.
- Signage set by `show_status` is read only by the showcase's visuals. It is in no document and no tool answer, so
  agents learn nothing from the plant that they wouldn't from the systems it shows.

### Running on a GPU machine: `SHAKEDOWN_RUN_ISAAC_SIM`

`shaked` replaces a package's run command with `SHAKEDOWN_RUN_<PACKAGE>` from its environment, run by `/bin/sh -c`
in the package folder ([`docs/protocol.md`](../../docs/protocol.md), "Manifest"). Pointed at ssh, the part runs on
a GPU machine with the protocol on the ssh session's stdio:

```bash
export SHAKEDOWN_RUN_ISAAC_SIM="exec ssh -p <port> -i <key> -o BatchMode=yes isaac@<host> -- 'cd /isaac-sim/work/shakedown/integrations/isaac-sim && \
  SHAKEDOWN_SAVES=/isaac-sim/work/saves PYTHONPATH=/isaac-sim/work/shakedown/kits/python exec /isaac-sim/python.sh -m isaac_part --backend isaac'"
```

- The remote machine needs this folder, `kits/python` and the example's `isaac/` and `data/` in the same layout
  (the repository next to `CopperTide-Examples/`), since the default paths and the fingerprint are relative to it.
- Saves and recordings stay on the remote machine, under its `SHAKEDOWN_SAVES`. `shaked`'s count of save bytes
  doesn't see them.
- The `nvcr.io/nvidia/isaac-sim:6.2.0` image has no sshd and runs as uid 1234 without root. `remote/sshd.py` is a
  small SSH server to start inside the container: key login only (any user name, the keys in the authorized-keys
  file), one `bash -c` command per session, output copied with backpressure. When the session closes first, the
  command gets EOF and 60 s to exit (the part finishes its recording on EOF), then its process group is killed.
  It needs `asyncssh`.

```bash
python remote/sshd.py <port> <authorized_keys> <host_key>     # the host key is made on first start
```

- On RunPod, the image's entrypoint ignores a start command, so the pod was created with an entrypoint override
  (the REST API's `dockerEntrypoint`) to start `sshd.py`.

### The mirror and the stand-in

- [`isaac-sim-standin`](../isaac-sim-standin/README.md) is this program with `--backend standin`.
- [`isaac-sim-mirror`](../isaac-sim-mirror/README.md) is this program with `--mode mirror`: it follows a running
  example plant over its HTTP API and only ever sends GET requests (`isaac_part/mirror.py`).

## How it's tested

```bash
cd integrations/isaac-sim
uv run --with pytest --with asyncssh pytest -q
```

Every test reads the private factory example, so without it every test is skipped, with the reason. All the
plant's tests run on `standin`. They cover:

- the scenario on the world clock, advances split differently giving the same result, replay equal to the first
  run, approximate restore (in the same process and in a fresh one over stdio), unreadable or mismatched saves,
  and the list of saved `Factory` attributes.
- the fingerprint (the adapter's hashes, a changed example file refused, the GPU read from `/proc` and not
  `nvidia-smi`), bad configs, a clear error when `isaacsim` is missing, tools, keys and refusals.
- the diff tolerance, recording paths and frame spacing (with a fake renderer), saves by convention, physics steps
  that never reach the plant, and only the newest physics callback reaching a plant.
- the cells and the load (`tests/test_equipment.py`), the showcase's geometry, motion and camera plan
  (`tests/test_showcase.py`) and the shift's scene pieces (`tests/test_shift_scene.py`, one test needs Pillow).
- the mirror against a stub HTTP server.
- `remote/sshd.py` with OpenSSH's client (`tests/test_remote_sshd.py`, skipped without `asyncssh` or `ssh`), including
  25 MB of output that arrives whole.
- a `shakedown-part-check` conformance run on a copy of the manifest that uses `--backend standin`, as it is and with
  the shift's cells and load model. It is skipped unless the checker is built
  (`(cd core && cargo build -p parts --bin shakedown-part-check)`) and `uv` is on `PATH`.

No test is marked `live`, and no test runs Isaac Sim.

### What ran on Isaac Sim

All runs used the `nvcr.io/nvidia/isaac-sim:6.2.0` image on rented RunPod pods, with the part on the pod and
everything else on a Linux workstation.

- **2026-11-04, RTX 3090 (24 GB, driver 580.95.05).** `plant/sim` read Isaac Sim `6.2.0.rc.14.6`. `shaked` started
  the part over ssh (`remote/sshd.py`) as the plant of the `factory` benchmark's bearing task, at
  `SHAKEDOWN_CLOCK_SPEED=0.1`, recording at 10 fps. The team was a scripted, no-model team on the gold path, which
  Shakedown no longer has, so this run can't be repeated as it was. The branch passed all nine checks. It started
  from the base save approximately, so the restore calls ran on 6.2 without an error, but the continuity of the
  readings after them was not measured. The MP4 had 1158 frames (115.8 s of world time). Measured speed: a first
  start with empty caches took about 4.5 min, later starts 31 s (another plant in the same process 6 to 8 s),
  physics alone about 4 s of wall time per second of world time (about 11 ms a step), recording at 30 fps 9.4 s
  and at 10 fps 5.9 s per second of world time.
- **2026-11-05, RTX A5000 (24 GB).** `remote/render_showcase.py --seconds 100` (seed 7, 1280x720, 30 fps): 3000
  frames. PR-A1 entered zone C at 35 s, zone D at 48 s and tripped at 64 s (the stand-in: 33.7, 46.4 and 61.9 s).
  About 11 s of wall time per second of world time with the recording, about 6 s for physics alone, with or
  without the showcase scene.
- **2026-11-08, RTX 3090.** The shift's scene pieces at 1080p and 24 fps: 12.1 s of wall time per second of world
  time (208 ms to render a frame). At 4K: 22.8 s. A first start on a fresh pod took 16 min, later ones about 1 min.
  On an A5000 host with driver 550.144.03, DLSS was unavailable and frames were grainy and slow, so use a recent
  driver. The same day, the `factory` benchmark's shift and bearing tasks ran on this plant under `shaked` with
  Claude Code on Sonnet, including a rewind restored exactly by replay. The results are in
  [`benchmarks/factory/README.md`](../../benchmarks/factory/README.md).

Not run on Isaac Sim: `shakedown-part-check` (it keeps instances live side by side, which `isaac` refuses), and a
restore from a save with the showcase scene.

## Limits

- It can't run without the private Ridgeline factory example, and only with files identical to the default copy.
- On `isaac` the plant must step faster than the world clock runs, or every call waits longer than the last. At the
  speeds above, runs on Isaac Sim used a world clock of 0.06 to 0.1.
- One live plant per process on `isaac`, so two branches at once need two processes.
- An approximate restore on `isaac` loses PhysX's solver caches, and its continuity has not been measured. After
  any restore, spectra cover only the time since the restore.
- Right after a start on Isaac Sim, the readings differ from the stand-in's (PR-A1 read 16.4 mm/s, zone D, at 3 s,
  and labels showed PR-C1 at 888.8 mm/s), which looks like the presses spinning up from rest. Not investigated.
- `predict_move` always refuses: there is no ERPNext link inside the plant.
- Ridgeline, its plant and its data are fictional. The bearing fault and the temperatures are modelled, and the
  showcase's motion is animation driven by the readings, not simulation.
