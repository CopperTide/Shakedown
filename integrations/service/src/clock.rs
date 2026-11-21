//! The world clock file of a live instance: `<clock folder>/clock.json`
//! `{epoch_ms, clock_ms, world_ms, speed, wall_ms}`, written at start and on each advance, read
//! by the containers that mount the folder (`SHAKEDOWN_CLOCK_DIR`) to stamp what they emit in world
//! time. It is replaced whole (written beside and renamed), so a reader never sees half of it.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shakedown_part::proto::WorldTimeParams;

pub const FILE: &str = "clock.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub epoch_ms: i64,
    pub clock_ms: i64,
    pub world_ms: i64,
    pub speed: f64,
    /// The wall time (unix ms) the clock was written at.
    pub wall_ms: i64,
    /// Whether the world gave its world time; a base or gold world runs on the wall clock, and
    /// the world-time gateway drops what it sends then (a file without the field is a world's).
    #[serde(default = "is_world")]
    pub world: bool,
}

fn is_world() -> bool {
    true
}

/// The unix ms now.
pub fn wall_now_ms() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis() as i64)
}

impl Clock {
    /// The world at `clock_ms` with `world_time`; a world that gives none (a base or gold world)
    /// runs at speed 1 from now, so stamps stay near the wall clock the stores accept.
    pub fn at_start(world_time: Option<WorldTimeParams>, clock_ms: i64) -> Clock {
        let wall = wall_now_ms();
        let world = world_time.is_some();
        let wt = world_time.unwrap_or(WorldTimeParams {
            epoch_ms: wall - clock_ms,
            speed: 1.0,
        });
        Clock {
            epoch_ms: wt.epoch_ms,
            clock_ms,
            world_ms: wt.epoch_ms + clock_ms,
            speed: wt.speed,
            wall_ms: wall,
            world,
        }
    }

    /// The same world moved to `clock_ms`, now.
    pub fn moved_to(self, clock_ms: i64) -> Clock {
        Clock {
            clock_ms,
            world_ms: self.epoch_ms + clock_ms,
            wall_ms: wall_now_ms(),
            ..self
        }
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        let tmp = dir.join(format!(".{FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(&tmp, dir.join(FILE)).context("replacing clock.json")
    }

    /// The clock last written in `dir`, if any.
    pub fn read(dir: &Path) -> Option<Clock> {
        let text = std::fs::read(dir.join(FILE)).ok()?;
        serde_json::from_slice(&text).ok()
    }
}

/// Where the clock folder of `instance` is under the package folder `root`.
pub fn dir_of(root: &Path, instance: &str) -> PathBuf {
    root.join(".clock").join(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_clock_keeps_its_epoch_and_speed() {
        let wt = WorldTimeParams {
            epoch_ms: 1_000_000,
            speed: 2.0,
        };
        let c = Clock::at_start(Some(wt), 500).moved_to(4_000);
        assert_eq!(
            (c.epoch_ms, c.clock_ms, c.world_ms, c.speed),
            (1_000_000, 4_000, 1_004_000, 2.0)
        );
    }

    #[test]
    fn without_world_time_the_world_starts_at_the_wall_clock() {
        let c = Clock::at_start(None, 0);
        assert!((c.world_ms - wall_now_ms()).abs() < 5_000);
    }

    #[test]
    fn the_file_says_whether_it_is_a_world_clock() {
        let wt = WorldTimeParams {
            epoch_ms: 1_000_000,
            speed: 1.0,
        };
        let json = |c: Clock| serde_json::to_value(c).unwrap()["world"].clone();
        assert_eq!(json(Clock::at_start(Some(wt), 0).moved_to(10)), true);
        assert_eq!(json(Clock::at_start(None, 0).moved_to(10)), false);
    }
}
