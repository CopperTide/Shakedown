//! World time for what a part emits (metrics, logs, traces): the world's epoch plus its clock, so
//! telemetry follows the world clock and stands still while the branch is paused. arenad gives a
//! part `world_time {epoch_ms, speed}` at `start` (docs/protocol.md, "World time").

use std::time::Instant;

pub use crate::proto::WorldTimeParams;

/// How far past the last clock it saw a part's world time runs on (world ms): a branch that is
/// paused or saving sends no clock, and its telemetry must not run away from it.
pub const MAX_LEAD_MS: i64 = 1_000;

/// A part's world time: stamps from the clock it is given, running on at `speed` between clocks
/// (at most `MAX_LEAD_MS` past the last one) and never going back.
#[derive(Debug, Clone)]
pub struct WorldTime {
    params: WorldTimeParams,
    /// The last clock seen, and when.
    seen: Option<(i64, Instant)>,
    /// The last time `now_ms` gave.
    given: i64,
}

impl WorldTime {
    pub fn new(params: WorldTimeParams) -> WorldTime {
        WorldTime {
            params,
            seen: None,
            given: i64::MIN,
        }
    }

    /// The unix ms of branch clock `clock_ms`.
    pub fn at(&self, clock_ms: i64) -> i64 {
        self.params.epoch_ms + clock_ms
    }

    /// Notes the branch clock `clock_ms` (from `advance` or `call_tool`) at `now`.
    pub fn observe(&mut self, clock_ms: i64, now: Instant) {
        self.seen = Some((clock_ms, now));
    }

    /// The world's unix ms at `now`: the last clock seen plus the time since at `speed`, at most
    /// `MAX_LEAD_MS` ahead of it, and never less than what it gave before.
    pub fn now_ms(&mut self, now: Instant) -> i64 {
        let clock = match self.seen {
            None => 0,
            Some((clock, at)) => {
                let since = now.saturating_duration_since(at).as_secs_f64() * 1000.0;
                clock + ((since * self.params.speed) as i64).min(MAX_LEAD_MS)
            }
        };
        self.given = self.given.max(self.at(clock));
        self.given
    }
}

/// `ms` (unix) as nanoseconds in decimal: Loki push and OTLP `timeUnixNano`.
pub fn unix_nanos(ms: i64) -> String {
    format!("{}", i128::from(ms) * 1_000_000)
}

/// `ms` (unix) as RFC 3339 in UTC with milliseconds (`2026-03-10T06:02:30.000Z`).
pub fn rfc3339(ms: i64) -> String {
    let (days, ms_of_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    let (h, min) = (ms_of_day / 3_600_000, ms_of_day / 60_000 % 60);
    let (sec, milli) = (ms_of_day / 1000 % 60, ms_of_day % 1000);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{sec:02}.{milli:03}Z")
}

/// The proleptic Gregorian date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn stamps_are_the_epoch_plus_the_clock() {
        let w = WorldTime::new(WorldTimeParams {
            epoch_ms: 1_773_122_400_000,
            speed: 3.0,
        });
        assert_eq!(w.at(150_000), 1_773_122_550_000);
        assert_eq!(unix_nanos(1_773_122_550_000), "1773122550000000000");
        assert_eq!(rfc3339(1_773_122_550_000), "2026-03-10T06:02:30.000Z");
        assert_eq!(rfc3339(1_773_122_550_042), "2026-03-10T06:02:30.042Z");
    }

    #[test]
    fn between_advances_time_runs_on_but_stands_still_past_the_lead_and_never_goes_back() {
        let params = WorldTimeParams {
            epoch_ms: 0,
            speed: 2.0,
        };
        let (mut w, t0) = (WorldTime::new(params), Instant::now());
        w.observe(10_000, t0);
        assert_eq!(w.now_ms(t0 + Duration::from_millis(200)), 10_400);
        assert_eq!(w.now_ms(t0 + Duration::from_secs(5)), 11_000); // a pause: capped at 1 s
        w.observe(10_600, t0 + Duration::from_secs(6)); // the clock resumed behind the cap
        assert_eq!(w.now_ms(t0 + Duration::from_secs(6)), 11_000); // never backwards
        assert_eq!(w.now_ms(t0 + Duration::from_millis(6_400)), 11_400);
    }

    #[test]
    fn before_any_clock_is_seen_time_is_the_epoch() {
        let mut w = WorldTime::new(WorldTimeParams {
            epoch_ms: 5_000,
            speed: 1.0,
        });
        assert_eq!(w.now_ms(Instant::now()), 5_000);
    }
}
