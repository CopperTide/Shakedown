use super::*;
use serde_json::json;

const VIEW: &str = r#"
format: shakedown-view/1
label: Ops
part: sim
styles:
  move:
    - {where: {delay: {">=": 15}}, tone: warn, label: Late}
    - {where: {state: up}, tone: active}
    - {tone: neutral}
  site:
    - {where: {closed: {exists: true}}, tone: critical, label: Closed}
  load:
    - {where: {ratio: {">=": 1}}, tone: critical}
    - {tone: active}
  big:
    - {where: {value: {">": 1}}, tone: warn}
panels:
  figures:
    kind: kpis
    items:
      - {label: Up, value: {find: "trip/*", where: {state: up}, count: true}, style: big}
      - {label: Score, value: {key: score, field: pct}, format: percent}
      - {label: Gone, value: {key: nope, field: pct}}
  map:
    kind: map
    layers:
      - {id: sites, kind: points, data: {find: "site/*"}, lat: lat, lon: lon, key: code, label: code, style: site}
      - id: trips
        kind: moving
        data: {find: "trip/*", where: {state: up}}
        origin: {layer: sites, field: from}
        dest: {layer: sites, field: to}
        depart: off
        arrive: [on, eta]
        style: move
  board:
    kind: board
    data: {find: "trip/*"}
    columns:
      - {field: from, label: From}
      - {field: [off, std], label: Off, format: time}
    sort: {field: delay, desc: true}
    style: move
    filters:
      - {id: late, label: Late, where: {delay: {">=": 15}}}
      - {id: up, label: Up, where: {state: up}}
    top: 2
  lanes:
    kind: timeline
    data: {find: "trip/*"}
    lane: unit
    start: [off, std]
    end: [on, eta]
    label: from
  duty:
    kind: bars
    data: {find: "crew/*"}
    label: id
    value: {since: report}
    limit: max
    style: load
  feed:
    kind: feed
    kinds: [alert]
  broken:
    kind: board
    data: {part: other, find: "x/*"}
    columns: [{field: a, label: A}]
screens:
  - {id: all, label: All, areas: ["figures map", "board lanes", "duty feed"]}
"#;

fn view() -> ViewFile {
    shakedown_project::format::parse("v.yaml", VIEW).unwrap()
}

fn docs() -> WorldDocs {
    let sim = json!({
        "site/A": {"code": "A", "lat": 1.0, "lon": 2.0},
        "site/B": {"code": "B", "lat": 3.0, "lon": 4.0, "closed": {"until": "x"}},
        "trip/1": {"from": "A", "to": "B", "state": "up", "unit": "u1", "delay": 20,
                   "off": "1970-01-01T00:10:00Z", "eta": "1970-01-01T01:10:00Z", "on": null},
        "trip/2": {"from": "B", "to": "C", "state": "up", "unit": "u2", "delay": 0,
                   "std": "1970-01-01T00:05:00Z", "eta": "1970-01-01T00:50:00Z"},
        "trip/3": {"from": "A", "to": "B", "state": "down", "unit": "u1", "delay": 5,
                   "std": "1970-01-01T02:00:00Z", "eta": "1970-01-01T03:00:00Z"},
        "crew/c1": {"id": "c1", "report": "1970-01-01T00:00:00Z", "max": 120},
        "crew/c2": {"id": "c2", "report": "1970-01-01T01:00:00Z", "max": 120},
        "score": {"pct": 91.5},
    });
    WorldDocs::from([("sim".to_string(), sim.as_object().unwrap().clone())])
}

fn feed_events() -> Vec<FeedEvent> {
    let e = |seq, kind: &str| FeedEvent {
        seq,
        clock_ms: seq * 10,
        world_ms: 1000 + seq * 10,
        part: "sim".into(),
        kind: kind.into(),
        text: format!("e{seq}"),
        data: json!({}),
    };
    vec![e(1, "alert"), e(2, "movement"), e(3, "alert")]
}

fn evaluated() -> Map<String, Value> {
    let (view, docs, events) = (view(), docs(), feed_events());
    let ctx = Ctx {
        view: &view,
        docs: &docs,
        world_ms: 7_200_000,
        events: &events,
    };
    screen(&ctx, &view.screens[0])
}

#[test]
fn figures_count_and_read_a_field_and_are_toned() {
    let out = &evaluated()["figures"];
    assert_eq!(out["kind"], "kpis");
    let items = out["items"].as_array().unwrap();
    assert_eq!(items[0], json!({"value": 2, "tone": "warn", "chip": null}));
    assert_eq!(items[1]["value"], 91.5);
    assert_eq!(items[2]["value"], Value::Null);
}

#[test]
fn a_moving_layer_joins_its_ends_on_a_points_layer() {
    let out = &evaluated()["map"]["layers"];
    let sites = &out["sites"]["rows"];
    assert_eq!(sites[1], json!(["B", "B", 3.0, 4.0, "critical", "Closed"]));
    let trips = out["trips"]["rows"].as_array().unwrap();
    assert_eq!(trips.len(), 1, "trip/2 goes to a site the map lacks");
    assert_eq!(
        trips[0],
        json!(["trip/1", null, 1.0, 2.0, 3.0, 4.0, 600_000, 4_200_000, "warn", "Late"])
    );
}

#[test]
fn a_board_sends_only_its_columns_sorted_filtered_and_cut() {
    let out = &evaluated()["board"];
    assert_eq!(
        out["columns"],
        json!(["key", "from", "off", "tone", "chip", "filters"])
    );
    assert_eq!(out["total"], 3);
    let rows = out["rows"].as_array().unwrap();
    assert_eq!(rows[0], json!(["trip/1", "A", 600_000, "warn", "Late", 3]));
    assert_eq!(
        rows[1],
        json!(["trip/3", "A", 7_200_000, "neutral", null, 0])
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn a_timeline_has_a_lane_per_value_and_bars_by_time() {
    let out = &evaluated()["lanes"];
    assert_eq!(out["lanes"], json!(["u1", "u2"]));
    assert_eq!(out["range"], json!([300_000, 10_800_000]));
    let first = &out["rows"][0];
    assert_eq!(
        first,
        &json!([0, "trip/1", "A", 600_000, 4_200_000, "neutral", null])
    );
}

#[test]
fn bars_measure_since_a_time_by_the_world_clock_worst_first() {
    let out = &evaluated()["duty"];
    let rows = out["rows"].as_array().unwrap();
    assert_eq!(
        rows[0],
        json!(["crew/c1", "c1", 120.0, 120.0, 1.0, "critical", null])
    );
    assert_eq!(
        rows[1],
        json!(["crew/c2", "c2", 60.0, 120.0, 0.5, "active", null])
    );
}

#[test]
fn a_feed_shows_its_kinds_newest_first() {
    let rows = evaluated()["feed"]["rows"].clone();
    let seqs: Vec<i64> = (rows.as_array().unwrap().iter())
        .map(|r| r[0].as_i64().unwrap())
        .collect();
    assert_eq!(seqs, [3, 1]);
    assert_eq!(rows[0][2], 1030);
}

#[test]
fn a_panel_that_cant_be_evaluated_says_why_alone() {
    let (view, docs) = (view(), docs());
    let ctx = Ctx {
        view: &view,
        docs: &docs,
        world_ms: 0,
        events: &[],
    };
    let out = panel(&ctx, &view.panels["broken"]);
    assert_eq!(
        out,
        json!({"kind": "board", "error": "no part other in this world"})
    );
    let parts = parts_of(&view, &view.panels["broken"]);
    assert_eq!(parts, BTreeSet::from(["other".to_string()]));
}
