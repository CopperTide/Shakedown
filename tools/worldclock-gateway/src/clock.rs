//! The world clock as the platform's `clock.json` says it, and wall stamps mapped to world time.
//!
//! Each clock file `{epoch_ms, clock_ms, speed, wall_ms}` is a point of a timeline: from the
//! wall time it was written, the world runs on at `speed` for at most `MAX_LEAD_MS` of wall
//! time (a paused branch writes no clock, so its stamps stand still), and a stamp is mapped by
//! the point in effect at its own wall time, so the spans of one batch keep their order and
//! durations. A point that comes behind where the one before had run to (a resume behind the
//! lead) starts from there instead: world time never goes back. A file of another epoch (a new
//! world) starts the timeline again. Without a clock file, world time is the wall time.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use shakedown_part::helpers::world_time::MAX_LEAD_MS;

const NS_PER_MS: i128 = 1_000_000;
const MAX_LEAD_NS: i128 = MAX_LEAD_MS as i128 * NS_PER_MS;
/// How many points are kept: telemetry older than the oldest is mapped by the oldest.
const KEPT: usize = 4096;
/// How often the file is checked for a change.
pub const POLL: Duration = Duration::from_millis(100);

/// What `clock.json` says (its `world_ms` is `epoch_ms + clock_ms`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ClockFile {
    pub epoch_ms: i64,
    pub clock_ms: i64,
    pub speed: f64,
    pub wall_ms: i64,
    /// False while the world has no world time of its own (a base or gold save being made); a
    /// file without the field is a world clock.
    #[serde(default = "is_world")]
    pub world: bool,
}

fn is_world() -> bool {
    true
}

/// One point of the timeline: world `base_ns` at wall `wall_ns`, never below `floor_ns`.
#[derive(Debug, Clone, Copy)]
struct Point {
    epoch_ms: i64,
    world: bool,
    base_ns: i128,
    wall_ns: i128,
    speed: f64,
    floor_ns: i128,
}

impl Point {
    fn world_ns(&self, wall_ns: i128) -> i128 {
        let lead = (wall_ns - self.wall_ns).min(MAX_LEAD_NS);
        let world = self.base_ns + (lead as f64 * self.speed) as i128;
        world.max(self.floor_ns)
    }
}

/// The clock points seen, oldest first.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    points: VecDeque<Point>,
}

impl Timeline {
    /// Whether the latest point is a world clock's (no point yet: it is).
    pub fn is_world(&self) -> bool {
        self.points.back().is_none_or(|p| p.world)
    }

    /// Adds the point a clock file gives.
    pub fn add(&mut self, f: ClockFile) {
        let wall_ns = i128::from(f.wall_ms) * NS_PER_MS;
        let last = self.points.back().copied();
        let follows = last.filter(|l| l.epoch_ms == f.epoch_ms && l.wall_ns <= wall_ns);
        if follows.is_none() {
            self.points.clear();
        }
        self.points.push_back(Point {
            epoch_ms: f.epoch_ms,
            world: f.world,
            base_ns: i128::from(f.epoch_ms + f.clock_ms) * NS_PER_MS,
            wall_ns,
            speed: f.speed,
            floor_ns: follows.map_or(i128::MIN, |l| l.world_ns(wall_ns)),
        });
        if self.points.len() > KEPT {
            self.points.pop_front();
        }
    }

    /// The world unix ns of the wall unix ns `wall_ns`; 0 stays 0 (a time not set).
    pub fn world_ns(&self, wall_ns: u64) -> u64 {
        if wall_ns == 0 {
            return 0;
        }
        let at = i128::from(wall_ns);
        let after = self.points.partition_point(|p| p.wall_ns <= at);
        let point = self.points.get(after.saturating_sub(1));
        let world = point.map_or(at, |p| p.world_ns(at));
        world.clamp(1, i128::from(u64::MAX)) as u64
    }
}

/// The unix ns now.
pub fn wall_now_ns() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    since.map_or(0, |d| d.as_nanos() as u64)
}

/// The world clock of a clock file, re-read whenever its modification time changes.
#[derive(Debug, Default)]
pub struct Clock {
    file: Option<PathBuf>,
    state: Mutex<(Timeline, Option<SystemTime>)>,
}

impl Clock {
    /// The clock `file` says (read now; `poll` keeps it current).
    pub fn of_file(file: PathBuf) -> Clock {
        let clock = Clock {
            file: Some(file),
            ..Clock::default()
        };
        clock.reload();
        clock
    }

    /// A clock of one file's content, never re-read.
    pub fn of(f: ClockFile) -> Clock {
        let clock = Clock::default();
        clock.add(f);
        clock
    }

    pub fn add(&self, f: ClockFile) {
        self.state.lock().expect("clock lock").0.add(f);
    }

    /// The timeline now, for mapping many stamps under one look.
    pub fn timeline(&self) -> Timeline {
        self.state.lock().expect("clock lock").0.clone()
    }

    pub fn world_ns(&self, wall_ns: u64) -> u64 {
        self.state.lock().expect("clock lock").0.world_ns(wall_ns)
    }

    /// Whether the latest clock file is a world clock (no file yet: it is).
    pub fn is_world(&self) -> bool {
        self.state.lock().expect("clock lock").0.is_world()
    }

    /// World now minus wall now, in ms.
    pub fn offset_ms(&self) -> i64 {
        let now = wall_now_ns();
        let diff = i128::from(self.world_ns(now)) - i128::from(now);
        (diff / NS_PER_MS) as i64
    }

    /// Reads the file again when it changed; whether it did.
    pub fn reload(&self) -> bool {
        let Some(file) = &self.file else { return false };
        let mtime = std::fs::metadata(file).and_then(|m| m.modified()).ok();
        let mut state = self.state.lock().expect("clock lock");
        if mtime.is_none() || mtime == state.1 {
            return false;
        }
        let read = std::fs::read(file).ok();
        let Some(f) = read.and_then(|t| serde_json::from_slice::<ClockFile>(&t).ok()) else {
            return false;
        };
        state.0.add(f);
        state.1 = mtime;
        true
    }

    /// Re-reads the file every `POLL` for as long as the program runs.
    pub fn poll(self: Arc<Clock>) {
        tokio::spawn(async move {
            let mut every = tokio::time::interval(POLL);
            loop {
                every.tick().await;
                self.reload();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const W: i64 = 1_773_122_000_000;

    pub fn ns(ms: i64) -> u64 {
        ms as u64 * 1_000_000
    }

    pub fn clock_at(v: Value, _read_at: i64) -> Clock {
        Clock::of(serde_json::from_value(v).unwrap())
    }

    #[test]
    fn a_pause_stops_world_time_in_stamps() {
        // Review Focus 3: no clock update for 5 s
        let c = clock_at(
            json!({"epoch_ms": 0, "clock_ms": 10_000, "speed": 1.0, "wall_ms": W}),
            W,
        );
        assert_eq!(c.world_ns(ns(W + 5_000)), ns(11_000));
    }

    #[test]
    fn stamps_are_mapped_by_the_clock_in_effect_at_their_wall_time() {
        let c = clock_at(
            json!({"epoch_ms": 0, "clock_ms": 10_000, "speed": 2.0, "wall_ms": W}),
            W,
        );
        c.add(
            serde_json::from_value(
                json!({"epoch_ms": 0, "clock_ms": 20_000, "speed": 2.0, "wall_ms": W + 4_000}),
            )
            .unwrap(),
        );
        assert_eq!(
            c.world_ns(ns(W + 300)),
            ns(10_600),
            "the first clock, at speed 2"
        );
        assert_eq!(c.world_ns(ns(W + 4_100)), ns(20_200), "the second");
        assert_eq!(
            c.world_ns(ns(W - 1_000)),
            ns(8_000),
            "before the first: run back from it"
        );
        assert_eq!(c.world_ns(0), 0, "a time not set stays unset");
    }

    #[test]
    fn a_resume_behind_the_lead_never_goes_back() {
        let c = clock_at(
            json!({"epoch_ms": 0, "clock_ms": 10_000, "speed": 1.0, "wall_ms": W}),
            W,
        );
        // paused 5 s (stamps stood at 11 000), resumed at clock 10 200
        c.add(
            serde_json::from_value(
                json!({"epoch_ms": 0, "clock_ms": 10_200, "speed": 1.0, "wall_ms": W + 5_000}),
            )
            .unwrap(),
        );
        assert_eq!(c.world_ns(ns(W + 5_100)), ns(11_000));
        assert_eq!(c.world_ns(ns(W + 6_000)), ns(11_200));
    }

    #[test]
    fn a_new_epoch_starts_again_and_no_file_is_wall_time() {
        let c = clock_at(
            json!({"epoch_ms": 0, "clock_ms": 10_000, "speed": 1.0, "wall_ms": W}),
            W,
        );
        c.add(
            serde_json::from_value(
                json!({"epoch_ms": 5_000, "clock_ms": 0, "speed": 1.0, "wall_ms": W + 10}),
            )
            .unwrap(),
        );
        assert_eq!(c.world_ns(ns(W + 10)), ns(5_000));
        assert_eq!(Clock::default().world_ns(ns(W)), ns(W));
    }

    #[test]
    fn the_file_is_read_again_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("wcg_clock_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("clock.json");
        let write = |clock_ms: i64| {
            let v = json!({"epoch_ms": 0, "clock_ms": clock_ms, "world_ms": clock_ms, "speed": 1.0, "wall_ms": W});
            std::fs::write(&file, v.to_string()).unwrap();
        };
        write(1_000);
        let c = Clock::of_file(file.clone());
        assert_eq!(c.world_ns(ns(W)), ns(1_000));
        std::thread::sleep(Duration::from_millis(20));
        write(2_000);
        assert!(c.reload());
        assert!(!c.reload(), "unchanged");
        assert_eq!(c.world_ns(ns(W)), ns(2_000));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
