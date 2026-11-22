//! A map panel's layers: points where their docs' `lat` and `lon` say, and paths from one point
//! to another (a doc's origin and destination keys joined on a points layer), with the times a
//! moving path departs and arrives so the renderer places it by the world clock.

use std::collections::HashMap;

use serde_json::{json, Map, Value};
use shakedown_project::format::{Field, Find, Layer, MapPanel, PathLayer, PointsLayer};

use super::eval::{toned, Ctx};
use super::query::{first_of, time_ms};

/// The docs a layer draws.
pub(super) fn data(layer: &Layer) -> &Find {
    match layer {
        Layer::Points(p) => &p.data,
        Layer::Arcs(p) | Layer::Moving(p) => &p.data,
    }
}

/// A point's position, `(lat, lon)`.
type Position = (f64, f64);

/// The positions of a points layer's keys.
type Positions = HashMap<String, Position>;

/// A field as a join key: text as it is, anything else as JSON.
fn key_of(doc: &Value, field: &Field) -> Option<String> {
    match first_of(doc, field)? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn label(doc: &Value, field: &Option<Field>) -> Value {
    field
        .as_ref()
        .and_then(|f| first_of(doc, f))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A points layer's `{columns, rows}`, and its keys' positions.
fn points(ctx: &Ctx, l: &PointsLayer) -> Result<(Value, Positions), String> {
    let mut rows = vec![];
    let mut at = Positions::new();
    for (_, doc) in super::query::found(ctx.docs, &l.data, ctx_part(ctx))? {
        let found = (
            key_of(doc, &l.key),
            first_of(doc, &l.lat),
            first_of(doc, &l.lon),
        );
        let (Some(key), Some(lat), Some(lon)) = found else {
            continue;
        };
        let (Some(lat), Some(lon)) = (lat.as_f64(), lon.as_f64()) else {
            continue;
        };
        let mut row = vec![json!(key), label(doc, &l.label), json!(lat), json!(lon)];
        row.extend(toned(ctx, l.style.as_deref(), doc)?);
        rows.push(Value::Array(row));
        at.insert(key, (lat, lon));
    }
    let columns = json!(["key", "label", "lat", "lon", "tone", "chip"]);
    Ok((json!({"columns": columns, "rows": rows}), at))
}

fn ctx_part<'a>(ctx: &'a Ctx) -> &'a str {
    ctx.view.part.as_deref().unwrap_or("")
}

fn time_of(doc: &Value, field: &Option<Field>) -> Value {
    let t = field
        .as_ref()
        .and_then(|f| first_of(doc, f))
        .and_then(time_ms);
    json!(t)
}

/// A path layer's `{columns, rows}`: docs whose ends are both on their points layers.
fn paths(ctx: &Ctx, l: &PathLayer, known: &HashMap<String, Positions>) -> Result<Value, String> {
    let end = |doc: &Value, join: &shakedown_project::format::Join| {
        let at = known.get(&join.layer)?;
        at.get(&key_of(doc, &join.field)?).copied()
    };
    let mut rows = vec![];
    for (key, doc) in super::query::found(ctx.docs, &l.data, ctx_part(ctx))? {
        let (Some(o), Some(d)) = (end(doc, &l.origin), end(doc, &l.dest)) else {
            continue;
        };
        let mut row = vec![json!(key), label(doc, &l.label), json!(o.0), json!(o.1)];
        row.extend([
            json!(d.0),
            json!(d.1),
            time_of(doc, &l.depart),
            time_of(doc, &l.arrive),
        ]);
        row.extend(toned(ctx, l.style.as_deref(), doc)?);
        rows.push(Value::Array(row));
    }
    let columns = [
        "key", "label", "olat", "olon", "dlat", "dlon", "depart", "arrive", "tone", "chip",
    ];
    Ok(json!({"columns": columns, "rows": rows}))
}

/// `{layers: {id: {columns, rows}}}`: points layers first, so paths can join on them.
pub(super) fn map(ctx: &Ctx, p: &MapPanel) -> Result<Value, String> {
    let mut layers = Map::new();
    let mut known = HashMap::new();
    for l in &p.layers {
        if let Layer::Points(pl) = l {
            let (drawn, at) = points(ctx, pl)?;
            layers.insert(pl.id.clone(), drawn);
            known.insert(pl.id.clone(), at);
        }
    }
    for l in &p.layers {
        if let Layer::Arcs(pl) | Layer::Moving(pl) = l {
            layers.insert(pl.id.clone(), paths(ctx, pl, &known)?);
        }
    }
    Ok(json!({"layers": layers}))
}
