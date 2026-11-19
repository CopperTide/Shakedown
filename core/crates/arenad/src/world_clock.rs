//! The world's own date and time, for agents. A task whose world keeps a calendar other than
//! today's (a plant dated March 2026) gives its clock 0 as `clock_start`; arenad tells every agent
//! the world's current date and time (clock 0 plus the branch clock) in its input, the same way for
//! every team, so an agent reads the world's dates against the world's clock and not against the
//! date its own environment reports.
use chrono::{DateTime, Duration, NaiveDateTime, Utc};

/// The format of a task's `clock_start`.
const START_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// A world's clock: its date and time at branch clock 0, and how fast the branch clock runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldClock {
    pub start: NaiveDateTime,
    pub speed: f64,
}

impl WorldClock {
    /// The clock of a task whose `clock_start` is `start` (`YYYY-MM-DDTHH:MM:SS`, fractions of a
    /// second allowed); None when it gives none or one that doesn't parse.
    pub fn of(start: Option<&str>, speed: f64) -> Option<WorldClock> {
        let start = parse_start(start?)?;
        Some(WorldClock { start, speed })
    }

    /// The unix ms of a world's clock 0: the task's `clock_start` read as UTC, else the run's
    /// start (`run_start`).
    pub fn epoch_ms(start: Option<&str>, run_start: DateTime<Utc>) -> i64 {
        match start.and_then(parse_start) {
            Some(t) => t.and_utc().timestamp_millis(),
            None => run_start.timestamp_millis(),
        }
    }

    /// The world's date and time at branch clock `now_ms`.
    pub fn at(&self, now_ms: i64) -> NaiveDateTime {
        self.start + Duration::milliseconds(now_ms.max(0))
    }

    /// The section that opens an agent's first input: the world's date and time now, and that it
    /// is not today's.
    pub fn brief(&self, now_ms: i64) -> String {
        let pace = match self.speed {
            s if (s - 1.0).abs() < 1e-9 => "with real time".to_string(),
            s => format!("at about {} times real time", trim_number(s)),
        };
        format!(
            "# World clock\n\nThis world keeps its own clock, which is not today's date. In this world \
             it is now {} (world time). The clock started at {} and runs while you work, {pace}. Read \
             every date and time in the world's systems against this clock, not against the date your \
             own environment reports: \"today\" and \"now\" mean the world's.\n\n",
            stamp(self.at(now_ms)),
            stamp(self.start),
        )
    }

    /// The line that ends each later input: the world's date and time now.
    pub fn now_line(&self, now_ms: i64) -> String {
        format!("\n(World time now: {}.)\n", stamp(self.at(now_ms)))
    }
}

/// A `clock_start` (`YYYY-MM-DDTHH:MM:SS`, fractions of a second allowed).
fn parse_start(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    let start = NaiveDateTime::parse_from_str(s, START_FORMAT)
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"));
    start.ok()
}

/// "Tuesday, 2026-03-10 06:02".
fn stamp(t: NaiveDateTime) -> String {
    t.format("%A, %Y-%m-%d %H:%M").to_string()
}

fn trim_number(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(speed: f64) -> WorldClock {
        WorldClock::of(Some("2026-03-10T06:00:00"), speed).unwrap()
    }

    #[test]
    fn the_epoch_is_clock_start_as_utc_else_the_runs_start() {
        let run_start = chrono::DateTime::parse_from_rfc3339("2026-11-19T10:00:00Z").unwrap();
        let run_start = run_start.with_timezone(&chrono::Utc);
        let epoch = WorldClock::epoch_ms(Some("2026-03-10T06:00:00"), run_start);
        assert_eq!(epoch, 1_773_122_400_000);
        assert_eq!(WorldClock::epoch_ms(None, run_start), 1_795_082_400_000);
        assert_eq!(
            WorldClock::epoch_ms(Some("soon"), run_start),
            1_795_082_400_000
        );
    }

    #[test]
    fn a_task_without_a_clock_start_has_no_world_clock() {
        assert_eq!(WorldClock::of(None, 1.0), None);
        assert_eq!(WorldClock::of(Some("next Tuesday"), 1.0), None);
        assert!(WorldClock::of(Some("2026-03-10T06:00:00.250"), 1.0).is_some());
    }

    #[test]
    fn the_world_time_is_clock_start_plus_the_branch_clock() {
        let c = clock(3.0);
        assert_eq!(c.at(0).to_string(), "2026-03-10 06:00:00");
        assert_eq!(c.at(150_000).to_string(), "2026-03-10 06:02:30");
        assert_eq!(c.at(86_400_000).to_string(), "2026-03-11 06:00:00");
        assert_eq!(c.at(-5).to_string(), "2026-03-10 06:00:00");
    }

    #[test]
    fn the_brief_gives_the_world_date_and_says_it_is_not_today() {
        let b = clock(3.0).brief(125_000);
        assert!(b.starts_with("# World clock\n"));
        assert!(
            b.contains("it is now Tuesday, 2026-03-10 06:02 (world time)"),
            "{b}"
        );
        assert!(b.contains("not today's date"));
        assert!(b.contains("about 3 times real time"));
        assert!(b.contains("not against the date your own environment reports"));
        assert!(clock(1.0)
            .brief(0)
            .contains("runs while you work, with real time"));
        assert!(clock(0.25).brief(0).contains("about 0.25 times"));
    }

    #[test]
    fn later_inputs_end_with_the_world_time_now() {
        assert_eq!(
            clock(3.0).now_line(3_600_000),
            "\n(World time now: Tuesday, 2026-03-10 07:00.)\n"
        );
    }
}
