//! A part's docs at a point of a branch's record, read from saves outside any running world: just
//! after a step, or at the end (the check's `final_saves`).
//!
//! A step reads the part's last save at or before it. A clock part is saved with the steps that
//! write it, not on every tick, so between saves its state has moved on with the clock: when the
//! step's world clock is later than its save's, the save is started in a scratch instance,
//! advanced to the step's clock, inspected and stopped. That is what a rerun from the step would
//! see, and the protocol's clock contract (how advances are split never changes the state) makes
//! it the state the run had. Runs pay nothing for it; only the steps someone reads do. A part that
//! restores by replay has no exact save to start from, and is shown at its last save.

use anyhow::Result;
use parts::{
    proto::{ExactKind, FromSave, Mode, StartParams, WorldPart},
    Hub,
};
use serde_json::{Map, Value};

use crate::{
    model::{Event, EventKind},
    recorded::{clock_at, final_saves, part_save_at, run_info, saves_of, world_time_of},
    save_calls,
};

/// Where in a branch's record docs are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    /// Just after the step with this seq.
    Step(i64),
    /// The branch's final state, saved for its check.
    Final,
}

impl Point {
    /// `final`, or a step's seq.
    pub fn parse(text: &str) -> Option<Point> {
        match text {
            "final" => Some(Point::Final),
            n => n.parse().ok().map(Point::Step),
        }
    }
}

/// Why a point has no state to read: said to the asker as it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotThere {
    #[error("there is no step with that number on this branch")]
    NoStep,
    #[error(
        "the world after that step can't be restored: the write at step {seq} to the part {part} has no save"
    )]
    Unsaved { seq: i64, part: String },
    #[error("there is no save of the part {0}")]
    NoSave(String),
    #[error("this branch has no final state yet")]
    NoFinal,
}

/// The save holding a part's state at a point, and the world clock it was made at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAt {
    pub save: String,
    pub saved_ms: i64,
    /// The world clock of the point (the save's own at the end).
    pub clock_ms: i64,
}

/// The seq of the branch's check, when it was scored.
fn check_seq(events: &[Event]) -> Option<i64> {
    let check = events.iter().rev().find(|e| e.kind == EventKind::Check);
    check.map(|e| e.seq)
}

/// The world clock of the last event at or before `seq` that carried a save of `part` (0 for
/// the run's start save).
fn saved_clock(events: &[Event], seq: i64, part: &str) -> i64 {
    let carries = |e: &&Event| {
        e.seq <= seq
            && matches!(e.kind, EventKind::ToolCall | EventKind::SavePoint)
            && saves_of(&e.payload).is_some_and(|s| s.contains_key(part))
    };
    let carrier = events.iter().rev().find(carries);
    carrier.map_or(0, |e| clock_at(events, e.seq + 1))
}

/// The save of `part` holding its state at `point` of the branch `branch`'s `events`. A step at
/// or after the check (its own, or the branch's end) reads the final state.
pub fn save_at(
    branch: &str,
    events: &[Event],
    part: &str,
    point: Point,
) -> Result<SaveAt, NotThere> {
    if let Point::Step(seq) = point {
        if !events.iter().any(|e| e.seq == seq) {
            return Err(NotThere::NoStep);
        }
    }
    let point = match (point, check_seq(events)) {
        (Point::Step(seq), Some(check)) if seq >= check => Point::Final,
        (p, _) => p,
    };
    match point {
        Point::Final => {
            let saves = final_saves(branch, events).ok_or(NotThere::NoFinal)?;
            let save = saves.get(part).ok_or(NotThere::NoSave(part.into()))?;
            Ok(SaveAt {
                save: save.clone(),
                saved_ms: 0,
                clock_ms: 0,
            })
        }
        Point::Step(seq) => step_save(events, part, seq),
    }
}

/// The part's own save at the step: what other parts left unsaved doesn't matter to it.
fn step_save(events: &[Event], part: &str, seq: i64) -> Result<SaveAt, NotThere> {
    let base = run_info(events).base;
    let save = part_save_at(events, seq, &base, part).map_err(|n| NotThere::Unsaved {
        seq: n.seq,
        part: n.part,
    })?;
    let save = save.ok_or(NotThere::NoSave(part.into()))?;
    Ok(SaveAt {
        save,
        saved_ms: saved_clock(events, seq, part),
        clock_ms: clock_at(events, seq + 1),
    })
}

/// The latest step a part left unsaved by `why` can be read at: the one just before its first
/// unsaved write, where its last save still holds. None when `why` is not an unsaved write.
pub fn restorable_before(events: &[Event], why: &NotThere) -> Option<Point> {
    let NotThere::Unsaved { seq, .. } = why else {
        return None;
    };
    let before = events.iter().rev().find(|e| e.seq < *seq);
    before.map(|e| Point::Step(e.seq))
}

/// The docs of `keys` (every doc for none) of `part` at `point` of the branch `branch`'s
/// `events`: its save there, moved on to the point's clock when it is a clock part that keeps
/// exact saves and was saved earlier. A `NotThere` is the error when the point has no state.
pub async fn docs_at(
    hub: &Hub,
    branch: &str,
    events: &[Event],
    part: &WorldPart,
    point: Point,
    keys: &[String],
) -> Result<Map<String, Value>> {
    let at = save_at(branch, events, &part.part, point)?;
    if at.clock_ms > at.saved_ms && moves_on(hub, part).await? {
        let start = scratch_start(events, part, &at);
        return save_calls::inspect_advanced(hub, &part.integration, &start, at.clock_ms, keys)
            .await;
    }
    save_calls::inspect_all(hub, &part.integration, &at.save, keys).await
}

/// True for a part whose state runs on with the clock and starts exactly from a save.
async fn moves_on(hub: &Hub, part: &WorldPart) -> Result<bool> {
    let d = hub.describe(&part.integration).await?;
    Ok(d.uses_clock && d.restore.exact == ExactKind::Save)
}

/// The start of a scratch instance of `part` from the save `at`, as a rerun would start it.
fn scratch_start(events: &[Event], part: &WorldPart, at: &SaveAt) -> StartParams {
    StartParams {
        instance: crate::world::scratch_instance_id(&part.part),
        config: part.config.clone(),
        seed: run_info(events).seed,
        clock_ms: at.saved_ms,
        from: Some(FromSave {
            save: at.save.clone(),
            mode: Mode::Exact,
        }),
        world_time: Some(world_time_of(events)),
        models: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(seq: i64, kind: EventKind, payload: Value) -> Event {
        crate::testkit::event(seq, None, kind, payload)
    }

    fn record() -> Vec<Event> {
        vec![
            ev(
                1,
                EventKind::RunStart,
                json!({"benchmark": "b", "seed": 7, "saves": {"box": "base"}}),
            ),
            ev(
                2,
                EventKind::ToolCall,
                json!({"part": "box", "writes": ["box/n"], "saves": {"box": "s2"}, "clock_ms": 100}),
            ),
            ev(
                3,
                EventKind::WorldEvent,
                json!({"part": "box", "kind": "alarm", "clock_ms": 450}),
            ),
            ev(4, EventKind::Check, json!({"final_saves": {"box": "end"}})),
        ]
    }

    #[test]
    fn a_step_reads_its_save_at_the_steps_clock_and_the_end_reads_the_final_save() {
        let evs = record();
        let at = |p| save_at("b", &evs, "box", p);
        let saved = |save: &str, saved_ms, clock_ms| SaveAt {
            save: save.into(),
            saved_ms,
            clock_ms,
        };
        assert_eq!(at(Point::Step(2)), Ok(saved("s2", 100, 100)));
        assert_eq!(at(Point::Step(3)), Ok(saved("s2", 100, 450)));
        assert_eq!(at(Point::Step(4)), Ok(saved("end", 0, 0)));
        assert_eq!(at(Point::Final), Ok(saved("end", 0, 0)));
        assert_eq!(at(Point::Step(9)), Err(NotThere::NoStep));
    }

    #[test]
    fn a_branch_not_scored_has_no_final_state() {
        let evs = &record()[..3];
        assert_eq!(
            save_at("b", evs, "box", Point::Final),
            Err(NotThere::NoFinal)
        );
        assert_eq!(Point::parse("final"), Some(Point::Final));
        assert_eq!(Point::parse("12"), Some(Point::Step(12)));
        assert_eq!(Point::parse("x"), None);
    }

    /// `record` with a second part `other` written at step 3 without a save, and `box` written
    /// at step 5 without one.
    fn unsaved_record() -> Vec<Event> {
        let mut evs = record()[..3].to_vec();
        evs[0].payload["saves"] = json!({"box": "base", "other": "o1"});
        evs.push(ev(
            4,
            EventKind::ToolCall,
            json!({"part": "other", "writes": ["other/x"], "clock_ms": 500}),
        ));
        evs.push(ev(
            5,
            EventKind::ToolCall,
            json!({"part": "box", "writes": ["box/n"], "clock_ms": 600}),
        ));
        evs.push(ev(
            6,
            EventKind::WorldEvent,
            json!({"part": "box", "kind": "alarm", "clock_ms": 700}),
        ));
        evs
    }

    #[test]
    fn a_part_reads_at_a_step_whatever_another_part_left_unsaved() {
        let evs = unsaved_record();
        let at = save_at("b", &evs, "box", Point::Step(4)).unwrap();
        assert_eq!(
            (at.save.as_str(), at.saved_ms, at.clock_ms),
            ("s2", 100, 500)
        );
        assert_eq!(
            save_at("b", &evs, "other", Point::Step(4)),
            Err(NotThere::Unsaved {
                seq: 4,
                part: "other".into()
            })
        );
    }

    #[test]
    fn a_part_left_unsaved_reads_at_the_step_before_its_first_unsaved_write() {
        let evs = unsaved_record();
        let err = save_at("b", &evs, "box", Point::Step(6)).unwrap_err();
        assert_eq!(
            err,
            NotThere::Unsaved {
                seq: 5,
                part: "box".into()
            }
        );
        assert_eq!(restorable_before(&evs, &err), Some(Point::Step(4)));
        assert_eq!(restorable_before(&evs, &NotThere::NoFinal), None);
        let at = save_at("b", &evs, "box", Point::Step(4)).unwrap();
        assert_eq!(at.save, "s2");
    }
}
