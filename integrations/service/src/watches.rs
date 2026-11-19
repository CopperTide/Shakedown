//! Watches: a probe polled every `every_ms` of world time whose list of items becomes world
//! events. The items are at pointer `items` of the probe's doc ("" for the doc itself), each
//! named by pointer `id`; an id not seen before gives an `on` event, a seen id that is gone an
//! `off` event, each with `id` and the item's `fields` (pointers, as last seen). A probe that
//! fails changes nothing. What each watch has seen is kept in a save (`watches.json`), so a fork
//! does not report again what was firing when it was saved.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use arena_part::proto::WorldEvent;
use serde::Deserialize;
use serde_json::{Map, Value};

const FILE: &str = "watches.json";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WatchSpec {
    /// The key of the probe it polls.
    pub probe: String,
    pub every_ms: i64,
    pub items: String,
    pub id: String,
    pub on: String,
    pub off: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

/// The items a watch saw, by id: their fields.
pub type Seen = BTreeMap<String, Map<String, Value>>;

/// A watch's state: when it polls next (clock ms) and what it saw.
#[derive(Debug, Clone, Default)]
pub struct Watching {
    pub next_ms: i64,
    pub seen: Seen,
}

impl WatchSpec {
    /// The items of `doc` by id, with their fields; None when the doc has no list there.
    fn items(&self, doc: &Value) -> Option<Seen> {
        let list = doc.pointer(&self.items)?.as_array()?;
        let mut out = Seen::new();
        for item in list {
            let id = match item.pointer(&self.id) {
                Some(Value::String(s)) => s.clone(),
                Some(v @ Value::Number(_)) => v.to_string(),
                _ => continue,
            };
            let fields = (self.fields.iter())
                .map(|(k, p)| (k.clone(), item.pointer(p).cloned().unwrap_or(Value::Null)))
                .collect();
            out.insert(id, fields);
        }
        Some(out)
    }

    /// The events `doc` (the probe's doc now) gives against what `w` saw, which it updates.
    pub fn step(&self, w: &mut Watching, doc: &Value) -> Vec<WorldEvent> {
        let Some(now) = self.items(doc) else {
            return vec![];
        };
        let gone = (w.seen.iter()).filter(|(id, _)| !now.contains_key(*id));
        let mut events: Vec<WorldEvent> = gone.map(|(id, f)| event(&self.off, id, f)).collect();
        let new = (now.iter()).filter(|(id, _)| !w.seen.contains_key(*id));
        events.extend(new.map(|(id, f)| event(&self.on, id, f)));
        w.seen = now;
        events
    }
}

fn event(kind: &str, id: &str, fields: &Map<String, Value>) -> WorldEvent {
    let mut data = fields.clone();
    data.insert("id".into(), Value::String(id.into()));
    WorldEvent {
        kind: kind.into(),
        text: format!("{kind}: {id}"),
        data: Value::Object(data),
    }
}

/// Keeps what each watch saw (in config order) in the save `dir`.
pub fn write(dir: &Path, seen: &[Seen]) -> Result<()> {
    std::fs::write(dir.join(FILE), serde_json::to_vec(seen)?)?;
    Ok(())
}

/// What each watch saw when the save `dir` was made (empty for a save without watches).
pub fn read(dir: &Path) -> Vec<Seen> {
    let text = std::fs::read(dir.join(FILE)).unwrap_or_default();
    serde_json::from_slice(&text).unwrap_or_default()
}
