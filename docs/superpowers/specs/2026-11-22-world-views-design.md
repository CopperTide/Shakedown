# World views: a declarative view language and one generic renderer

Design, 2026-11-22, branch `airline-world` (project 3). Owner's rules: a world's maps and dashboards come only
from view files shipped with the world (YAML in its folder), drawn by one renderer in core and the UI; never
world-specific UI code. The demo is an airline operations centre over United's network day (2,495 flights, 797
tails, 128 stations).

Today a benchmark draws one fixed `TwinView` (`docs/protocol.md`, "The world's view"). That stays for the
benchmarks that use it. A world with view files gets the screens its files declare instead.

## 1. Where views live

`world.yaml` lists its view files, relative to the world's folder:

```yaml
format: shakedown-world/3
views: [views/occ.yaml]
```

A view's id is its file stem (`occ`). A view file that doesn't parse is a problem of its world, like any other
format error, and is left out.

## 2. The file: `format: shakedown-view/1`

```yaml
format: shakedown-view/1
label: Operations control          # the view's name
part: driver                       # the part bindings read when they name none
styles: {<name>: [StyleRule]}      # tones from field values, first match wins
panels: {<id>: Panel}              # what is drawn
screens: [Screen]                  # how panels are laid out; the first is the default
```

### Bindings: the expect grammar's finders

The finders of `kits/python/shakedown_part/expect.py`, without the check part:

- **one doc:** `{part?, key, field?}`, e.g. `{key: kpis, field: otp_a15_pct}`.
- **many docs:** `{part?, find: "<glob>", where?}`, e.g. `{find: "flight/*", where: {status: airborne}}`. `*`
  matches any run of characters and `?` one character. `where` is expect's: `{field: value}` (equal) or
  `{field: {op: value}}` with the ops `= != < <= > >= in contains exists absent`; numbers are equal within 0.005.

A **field** is a dotted path into a doc (`program.arrival_rate`, list items by index). Where a panel takes a
field, a list of fields means the first one present and not null (`[on, eta]`: the landing, else the estimate).
A field used as a time (map, timeline, `format: time`) may hold an ISO time or epoch ms; core sends epoch ms.

### Styles: tones from field values

```yaml
styles:
  flight:
    - {where: {status: cancelled}, tone: critical, label: CNX}
    - {where: {delay_min: {">=": 60}}, tone: critical}
    - {where: {delay_min: {">=": 15}}, tone: warn}
    - {where: {status: airborne}, tone: active}
    - {tone: neutral}
```

A rule is `{where?, tone, label?}`; the first rule whose `where` holds gives the row its tone (and a chip label
in place of the value, when given). No rule matching is `neutral`. Tones are the renderer's fixed set:
`neutral`, `active`, `done`, `warn`, `critical`, `accent`. The renderer maps tones to its palette; a view file
never names a colour.

### Panels

Every panel has `kind` and an optional `title`.

| kind | Fields | Draws |
|---|---|---|
| `kpis` | `items: [{label, value: <one doc> \| <many docs> with count: true, format?, decimals?, style?}]` | a strip of figures; `style` rules see `{value}` |
| `map` | `center: [lon, lat]`, `zoom`, `layers: [Layer]` | a MapLibre map on OpenFreeMap tiles |
| `board` | `data: <many docs>`, `columns: [{field, label, format?}]`, `sort?: {field, desc?}`, `style?`, `filters?: [{id, label, where}]`, `top?` | a dense table with status chips, quick filters and search |
| `timeline` | `data`, `lane`, `start`, `end`, `label?`, `style?`, `lane_label?` | one lane per distinct `lane` value, a bar per doc, a now line; virtualized |
| `bars` | `data`, `label`, `value: <field> \| {since: <time field>}`, `limit: <field>`, `unit?`, `style?`, `top?` | a value against its limit, worst ratio first |
| `feed` | `kinds: [<event kind>]`, `parts?: [<part>]`, `style?`, `top?` | the world's events, newest first |

Map layers (`id` and `kind` on each):

- `points`: `data`, `lat`, `lon`, `key` (the field others join on), `label?`, `style?`.
- `arcs` and `moving`: `data`, `origin: {layer, field}` and `dest: {layer, field}` (the doc's field holds a
  `key` of that points layer), `label?`, `style?`; `moving` also takes `depart` and `arrive` (time fields).
  The renderer places a `moving` doc on the great circle from origin to dest at
  `(now − depart) / (arrive − depart)` of the way, by the world clock, so aircraft move between updates.

`format` is one of `text` (default), `int`, `number`, `percent` (a value already in percent), `ratio_pct` (a
ratio shown as a percent: `0.93` → `93%`), `usd`, `minutes` (`75` → `1:15`), `time` (`14:05Z`), `chip` (the
style's tone and label). `bars` style rules also see the computed `value`, `limit` and `ratio` (value ÷ limit).
`feed` style rules see the event: `{part, kind, text, data}`.

### Screens: a grid of named areas

```yaml
screens:
  - id: overview
    label: Overview
    areas: ["kpis kpis", "network alerts"]   # CSS grid-template-areas; each name is a panel id
    columns: "3fr 1fr"                      # optional grid-template-columns
    rows: "auto 1fr"                        # optional grid-template-rows
```

## 3. Evaluation in core

Core evaluates a screen's panels against the parts' docs at one point of a branch and sends compact rows; the
UI never sees whole docs.

- **The point** (`at`): `live` (the live branch's instances now), `final` (the check's `final_saves`), or a step
  `seq`. Default: `live` while the branch runs, else `final`, else its last step.
- **Docs at a step** (shared with `GET /api/branches/{id}/rows`): each part's last save at or before the step.
  A clock part whose save is older than the step's world clock is started from that save in a scratch
  instance, advanced to the step's clock, inspected and stopped: what a rerun from that step would see. This
  is the cheapest correct choice: a run pays nothing more (no extra saves on ticks, which at 2,880× would be a
  multi-megabyte save every 200 ms), and only the steps someone looks at cost a start and an advance. It needs
  what reruns already need: a clock part that keeps exact saves and advances deterministically. A part
  restored by replay is shown at its last save.
- **Payload:** only the fields the view names, as `columns` plus `rows` of arrays; times as epoch ms; one
  `tone` (and `chip` label) per row instead of styling data; board filters as a bitmask per row. For the
  network day the overview's map is a few hundred airborne flights and 128 stations; the board and timeline are
  2.5k rows of about ten values.
- **Cache:** the docs of the last few points per branch stay in memory (a recorded step never changes).

### API

`GET /api/branches/{id}/world-views` lists the branch's views: `{world, views: [{id, label, panels, screens}]}`
(the files as parsed, the panel definitions included so the renderer knows labels and formats), and `problems`
for files that don't parse. The branch's world is the project world whose `package` is the run's benchmark
(a run doesn't record its project experiment's world). Empty `views` when it has none.

`GET /api/branches/{id}/world-views/{view}?screen=<id>&at=live|final|<seq>` evaluates one screen:

```json
{"view": "occ", "screen": "overview",
 "at": {"seq": 812, "point": "step", "clock_ms": 51000000, "world_ms": 1786626600000,
        "epoch_ms": 1786575600000, "speed": 0, "paused": false},
 "panels": {
   "kpis":    {"kind": "kpis", "items": [{"value": 81.2, "tone": "warn"}]},
   "network": {"kind": "map", "layers": {
     "stations": {"columns": ["key", "label", "lat", "lon", "tone", "chip"], "rows": [["IAD", "IAD", 38.95, -77.46, "critical", "Ground delay"]]},
     "aircraft": {"columns": ["key", "label", "olat", "olon", "dlat", "dlon", "depart", "arrive", "tone", "chip"], "rows": []}}},
   "flights": {"kind": "board", "columns": ["key", "number", "origin", "tone", "chip", "filters"], "rows": [], "total": 2495},
   "tails":   {"kind": "timeline", "lanes": ["N101UA"], "columns": ["lane", "key", "label", "start", "end", "tone", "chip"], "rows": [], "range": [0, 0]},
   "duty":    {"kind": "bars", "columns": ["key", "label", "value", "limit", "ratio", "tone", "chip"], "rows": []},
   "alerts":  {"kind": "feed", "columns": ["seq", "clock_ms", "world_ms", "part", "kind", "text", "tone", "chip"], "rows": []}}}
```

`kpis.items` follow the file's order. A board column appears under its field's dotted path (the first path of a
list), and a row carries the board's one tone and chip;
a timeline row's `lane` is an index into `lanes`. `world_ms` is the world's epoch plus `clock_ms`; `speed` is
world ms per real ms while live and running (0 on a stepped or frozen clock), so the renderer can move aircraft
between polls. A panel that can't be evaluated (a part the world lacks, a bad field) comes back as
`{"kind": …, "error": "…"}` alone; the rest of the screen still draws.

This sits beside the `world_view` event, it doesn't replace it: the twin's recorded views stay for the
benchmarks that draw them, and the declarative views are evaluated on request, never recorded.

## 4. The renderer (UI)

One React renderer in `ui/src/features/views/`, reached from the run's World page: a world with views shows its
screens as tabs; a world without falls back to the twin.

- **Live** runs poll `at=live` about once a second while the branch runs (and on new events), and interpolate
  moving layers by `world_ms + speed × elapsed`. **Recorded** runs scrub: the track's stops are the steps where
  world time moved; a stop fetches `at=<seq>` (debounced), the end fetches `at=final`.
- **Identity:** CopperTide's: warm grays (no pure white or black), copper the only accent, serif titles,
  Instrument Sans and IBM Plex Mono (never Inter), tabular numerals, dense like an operations tool. No
  gradients, emoji or rounded-card grids: panels are ruled, square-cornered regions. Tones map to a restrained
  palette (critical is a muted brick red, warn an ochre, active a slate blue, done a gray-green, accent copper).
- **Map:** MapLibre GL with the free OpenFreeMap `positron` style (no keys), desaturated to the warm grays;
  stations as small marks sized by role, a station with a tone above `neutral` ringed; aircraft as small
  oriented glyphs on a canvas layer, recomputed each frame from the clock.
- **Board:** sticky header, sortable columns, quick filters from `filters`, a search box over text columns,
  virtualized rows. **Timeline:** lanes virtualized (about 800), a time axis in UTC, a now line, bars toned.
  **Bars:** a bar per row to its limit, the limit marked, the ratio in numerals. **Feed:** newest first, time
  in UTC, tone as a left rule.

## 5. Not in this version

Click-through from a map mark to its doc; joins other than a map layer's origin and dest; per-screen
refresh rates; editing views in the app (the YAML is the editor until the views editor lands).
