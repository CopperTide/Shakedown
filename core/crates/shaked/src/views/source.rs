//! The docs a view is evaluated against, at a point of a branch: its live instances now, its
//! final state, or just after a step (`crate::docs_at`, which moves a clock part on to the step's
//! clock). A recorded point never changes, so the docs of the last few are kept in memory.

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

use super::{eval::FeedEvent, query::WorldDocs};
use crate::{
    app::AppState,
    docs_at::{docs_at, Point},
    model::{Event, EventKind},
    recorded::{clock_at, world_epoch_ms},
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
}

impl Moment {
    pub fn to_json(&self) -> Value {
        json!({"point": self.point, "seq": self.seq, "clock_ms": self.clock_ms,
               "world_ms": self.world_ms, "epoch_ms": self.world_ms - self.clock_ms,
               "speed": self.speed, "paused": self.paused})
    }
}

/// Docs read at a moment, and the world events up to it.
pub struct Read {
    pub docs: WorldDocs,
    pub moment: Moment,
    pub events: Vec<FeedEvent>,
}

const KEPT: usize = 8;

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
    point: Point,
) -> Result<Read> {
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
    };
    Ok(Read {
        docs,
        moment,
        events: feed_events(events, seq),
    })
}
