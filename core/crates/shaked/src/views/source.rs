//! The docs a view is evaluated against, at a point of a branch: its live instances now, its
//! final state, or just after a step (`crate::docs_at`, which moves a clock part on to the step's
//! clock). Only the parts the view reads are restored; a step where one of them can't be (it was
//! written without a save) is shown as at the latest step before that write, and the answer says
//! so. A recorded point never changes, so the docs of the last few are kept in memory, and so is
//! the lean record (no model-call bodies) of the last few branches read, while it stays the same.

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

use super::{eval::FeedEvent, query::WorldDocs};
use crate::{
    app::AppState,
    auth::Caller,
    docs_at::{docs_at, restorable_before, save_at, NotThere, Point},
    model::{Event, EventKind},
    recorded::{clock_at, world_epoch_ms, world_utc_offset_min},
    saves::{part_named, world_of},
};

/// Where a view is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    /// The live branch's instances now.
    Live,
    /// A recorded point.
    Recorded(Point),
}

impl At {
    /// `live`, `final` or a step's seq.
    pub fn parse(text: &str) -> Option<At> {
        match text {
            "live" => Some(At::Live),
            other => Point::parse(other).map(At::Recorded),
        }
    }

    /// Live while the branch runs, else its end once scored, else its last step.
    pub fn default_for(live: bool, events: &[Event]) -> At {
        let scored = events.iter().any(|e| e.kind == EventKind::Check);
        match (live, scored, events.last()) {
            (true, _, _) => At::Live,
            (false, true, _) | (false, false, None) => At::Recorded(Point::Final),
            (false, false, Some(e)) => At::Recorded(Point::Step(e.seq)),
        }
    }
}

/// The point docs were read at, as the answer tells it.
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub point: &'static str,
    pub seq: Option<i64>,
    pub clock_ms: i64,
    pub world_ms: i64,
    /// World ms per real ms while the clock runs; 0 when it stands still or is stepped.
    pub speed: f64,
    pub paused: bool,
    /// The world's UTC offset in minutes (its world times are local with it); None: UTC.
    pub utc_offset_min: Option<i32>,
    /// Set when the step asked for couldn't be restored and an earlier one is shown.
    pub fallback: Option<Fallback>,
}

/// A step shown in place of the one asked for, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Fallback {
    pub asked: i64,
    pub why: String,
}

impl Moment {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"point": self.point, "seq": self.seq, "clock_ms": self.clock_ms,
               "world_ms": self.world_ms, "epoch_ms": self.world_ms - self.clock_ms,
               "speed": self.speed, "paused": self.paused, "utc_offset_min": self.utc_offset_min});
        if let Some(f) = &self.fallback {
            v["fallback"] = json!({"asked": f.asked, "why": f.why});
        }
        v
    }
}

/// Docs read at a moment, and the world events up to it.
pub struct Read {
    pub docs: WorldDocs,
    pub moment: Moment,
    pub events: Vec<FeedEvent>,
}

const KEPT: usize = 8;
const KEPT_RECORDS: usize = 3;

/// A branch's lean record and the `(count, last seq)` it was read at.
type Kept = (String, (i64, i64), Arc<Vec<Event>>);

static RECORDS: LazyLock<Mutex<VecDeque<Kept>>> = LazyLock::new(Default::default);

/// The branch's events without model-call bodies (`Scoped::events_lean`): from memory while the
/// branch has the same events as when they were read, else read again. The caller must be one
/// who may read the branch (the store checks it on every read).
pub async fn record(state: &AppState, caller: &Caller, branch: &str) -> Result<Arc<Vec<Event>>> {
    let store = state.store.at(caller);
    let version = store.events_version(branch).await?;
    {
        let kept = RECORDS.lock().unwrap_or_else(|e| e.into_inner());
        let hit = kept.iter().find(|(b, v, _)| b == branch && *v == version);
        if let Some((_, _, events)) = hit {
            return Ok(events.clone());
        }
    }
    let events = Arc::new(store.events_lean(branch).await?);
    let mut kept = RECORDS.lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|(b, _, _)| b != branch);
    kept.push_back((branch.to_string(), version, events.clone()));
    while kept.len() > KEPT_RECORDS {
        kept.pop_front();
    }
    Ok(events)
}

type CacheKey = (String, String, String);

/// One part's docs at a recorded point.
type PartDocs = Arc<Map<String, Value>>;

/// The docs of recently read recorded points: `(branch, point, part)`.
static CACHE: LazyLock<Mutex<VecDeque<(CacheKey, PartDocs)>>> = LazyLock::new(Default::default);

fn cached(key: &CacheKey) -> Option<PartDocs> {
    let kept = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    kept.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn keep(key: CacheKey, docs: PartDocs) {
    let mut kept = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|(k, _)| *k != key);
    kept.push_back((key, docs));
    while kept.len() > KEPT {
        kept.pop_front();
    }
}

/// The world events of `events` up to `seq` (all of them for none).
fn feed_events(events: &[Event], seq: Option<i64>) -> Vec<FeedEvent> {
    let epoch = world_epoch_ms(events);
    let upto = |e: &&Event| seq.is_none_or(|s| e.seq <= s) && e.kind == EventKind::WorldEvent;
    let of = |e: &Event| {
        let p = &e.payload;
        let clock_ms = p["clock_ms"].as_i64().unwrap_or(0);
        let text = |k: &str| p[k].as_str().unwrap_or("").to_string();
        FeedEvent {
            seq: e.seq,
            clock_ms,
            world_ms: epoch + clock_ms,
            part: text("part"),
            kind: text("kind"),
            text: text("text"),
            data: p["data"].clone(),
        }
    };
    events.iter().filter(upto).map(of).collect()
}

/// The docs of `parts` (those the world has) at `at` of branch `branch`, whose record is
/// `events`. Live when the branch isn't is an error.
pub async fn read(
    state: &AppState,
    branch: &str,
    events: &[Event],
    parts: &BTreeSet<String>,
    at: At,
) -> Result<Read> {
    match at {
        At::Live => read_live(state, branch, events, parts).await,
        At::Recorded(point) => read_recorded(state, branch, events, parts, point).await,
    }
}

async fn read_live(
    state: &AppState,
    branch: &str,
    events: &[Event],
    parts: &BTreeSet<String>,
) -> Result<Read> {
    let ctx = (state.live.get(branch)).ok_or_else(|| anyhow!(NotLive))?;
    let mut docs = WorldDocs::new();
    for part in parts.iter().filter(|p| ctx.world.part_names().contains(p)) {
        docs.insert(part.clone(), ctx.world.inspect_live(part, &[]).await?);
    }
    let clock_ms = ctx.clock.now_ms();
    let still = ctx.clock.is_stepped() || ctx.clock.is_paused();
    let moment = Moment {
        point: "live",
        seq: events.last().map(|e| e.seq),
        clock_ms,
        world_ms: world_epoch_ms(events) + clock_ms,
        speed: if still { 0.0 } else { ctx.clock.speed() },
        paused: ctx.clock.is_paused(),
        utc_offset_min: world_utc_offset_min(events),
        fallback: None,
    };
    Ok(Read {
        docs,
        moment,
        events: feed_events(events, None),
    })
}

/// Asked for the live view of a branch that isn't running.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("the branch is not running")]
pub struct NotLive;

/// The point `parts` can all be read at: `point`, else (a step where one was written without a
/// save) the latest step before the write, and why. A point with no state at all is the error.
fn readable_point(
    branch: &str,
    events: &[Event],
    parts: &BTreeSet<String>,
    point: Point,
) -> Result<(Point, Option<NotThere>), NotThere> {
    let mut at = (point, None);
    'again: loop {
        for part in parts {
            let why = match save_at(branch, events, part, at.0) {
                Ok(_) | Err(NotThere::NoSave(_)) => continue,
                Err(why) => why,
            };
            let before = restorable_before(events, &why).ok_or_else(|| why.clone())?;
            at = (before, Some(why));
            continue 'again;
        }
        return Ok(at);
    }
}

/// The docs of `parts` (those the world has) at a recorded `point`.
async fn recorded_docs(
    state: &AppState,
    branch: &str,
    events: &[Event],
    parts: &BTreeSet<String>,
    point: Point,
) -> Result<WorldDocs> {
    let world = world_of(&state.hub, events).await?;
    let mut docs = WorldDocs::new();
    for name in parts {
        let Some(part) = part_named(&world, name) else {
            continue;
        };
        let key = (branch.to_string(), format!("{point:?}"), name.clone());
        let read = match cached(&key) {
            Some(d) => d,
            None => Arc::new(docs_at(&state.hub, branch, events, part, point, &[]).await?),
        };
        keep(key, read.clone());
        docs.insert(name.clone(), (*read).clone());
    }
    Ok(docs)
}

async fn read_recorded(
    state: &AppState,
    branch: &str,
    events: &[Event],
    parts: &BTreeSet<String>,
    asked: Point,
) -> Result<Read> {
    let world = world_of(&state.hub, events).await?;
    let kept: BTreeSet<String> = (parts.iter())
        .filter(|p| part_named(&world, p).is_some())
        .cloned()
        .collect();
    let (point, why) = readable_point(branch, events, &kept, asked)?;
    let fallback = match (asked, why) {
        (Point::Step(asked), Some(why)) => Some(Fallback {
            asked,
            why: why.to_string(),
        }),
        _ => None,
    };
    let docs = recorded_docs(state, branch, events, parts, point).await?;
    let (name, seq) = match point {
        Point::Step(seq) => ("step", Some(seq)),
        Point::Final => ("final", events.last().map(|e| e.seq)),
    };
    let clock_ms = clock_at(events, seq.map_or(i64::MAX, |s| s + 1));
    let moment = Moment {
        point: name,
        seq,
        clock_ms,
        world_ms: world_epoch_ms(events) + clock_ms,
        speed: 0.0,
        paused: false,
        utc_offset_min: world_utc_offset_min(events),
        fallback,
    };
    Ok(Read {
        docs,
        moment,
        events: feed_events(events, seq),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: i64, kind: EventKind, payload: Value) -> Event {
        crate::testkit::event(seq, None, kind, payload)
    }

    /// `box` saved at 2, `other` written at 3 without a save, `box` written at 4 without one.
    fn record() -> Vec<Event> {
        vec![
            ev(
                1,
                EventKind::RunStart,
                json!({"benchmark": "b", "seed": 7, "saves": {"box": "base", "other": "o"}}),
            ),
            ev(
                2,
                EventKind::ToolCall,
                json!({"part": "box", "writes": ["box/n"], "saves": {"box": "s2"}, "clock_ms": 100}),
            ),
            ev(
                3,
                EventKind::ToolCall,
                json!({"part": "other", "writes": ["other/x"], "clock_ms": 200}),
            ),
            ev(
                4,
                EventKind::ToolCall,
                json!({"part": "box", "writes": ["box/n"], "clock_ms": 300}),
            ),
            ev(
                5,
                EventKind::WorldEvent,
                json!({"part": "box", "kind": "alarm", "clock_ms": 400}),
            ),
        ]
    }

    #[test]
    fn a_view_reads_only_its_parts_and_falls_back_before_an_unsaved_write_of_one() {
        let evs = record();
        let only = |p: &str| BTreeSet::from([p.to_string()]);
        assert_eq!(
            readable_point("b", &evs, &only("box"), Point::Step(3)),
            Ok((Point::Step(3), None))
        );
        let (point, why) = readable_point("b", &evs, &only("box"), Point::Step(5)).unwrap();
        assert_eq!(point, Point::Step(3));
        assert_eq!(
            why,
            Some(NotThere::Unsaved {
                seq: 4,
                part: "box".into()
            })
        );
        let both = BTreeSet::from(["box".to_string(), "other".to_string()]);
        assert_eq!(
            readable_point("b", &evs, &both, Point::Step(5)).unwrap().0,
            Point::Step(2)
        );
        assert_eq!(
            readable_point("b", &evs, &only("box"), Point::Step(9)),
            Err(NotThere::NoStep)
        );
    }
}
