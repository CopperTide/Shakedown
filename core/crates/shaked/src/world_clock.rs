//! The world's own date and time, for agents. A task whose world keeps a calendar other than
//! today's (a plant dated March 2026) gives its clock 0 as `clock_start`; shaked tells every agent
//! the world's current date and time (clock 0 plus the branch clock) in its input, the same way for
//! every team, so an agent reads the world's dates against the world's clock and not against the
//! date its own environment reports.
//!
//! `clock_start` is the world's local time. It may end in its UTC offset (`2026-08-13T00:00-04:00`,
//! or `Z`): that offset is the world's one statement of its time zone. The epoch (unix ms of clock
//! 0) is then exact, and what shows world times (the brief, the views) shows them local with the
//! offset. Without one the local time is read as UTC, as before. Scenario times (`HH:MM`) are
//! world local times either way: they are placed against the local start, never the offset.
use chrono::{DateTime, Duration, NaiveDateTime, Utc};

/// The local formats of a `clock_start`, before any offset.
const START_FORMATS: [&str; 3] = [
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M",
];

/// A task's `clock_start`: the world's local date and time at clock 0, and its UTC offset in
/// minutes when it gives one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockStart {
    pub local: NaiveDateTime,
    pub utc_offset_min: Option<i32>,
}

impl ClockStart {
    /// `YYYY-MM-DDTHH:MM[:SS[.fff]]`, then optionally `Z` or `±HH:MM` (`±HHMM`, `±HH`); None when
    /// it doesn't parse.
    pub fn parse(text: &str) -> Option<ClockStart> {
        let t = text.trim();
        let (local, utc_offset_min) = split_offset(t)?;
        let local = START_FORMATS
            .iter()
            .find_map(|f| NaiveDateTime::parse_from_str(local, f).ok())?;
        Some(ClockStart {
            local,
            utc_offset_min,
        })
    }

    /// The unix ms of clock 0: the local time less its offset (none: read as UTC).
    pub fn epoch_ms(&self) -> i64 {
        let offset = Duration::minutes(self.utc_offset_min.unwrap_or(0).into());
        (self.local - offset).and_utc().timestamp_millis()
    }
}

/// `text` split into its local part and its offset in minutes (None without one). An offset is a
/// trailing `Z`, or a sign after the time (the date's own dashes come before the `T`).
fn split_offset(text: &str) -> Option<(&str, Option<i32>)> {
    if let Some(local) = text.strip_suffix('Z').or_else(|| text.strip_suffix('z')) {
        return Some((local, Some(0)));
    }
    let time_at = text.find('T')?;
    let Some(sign_at) = text[time_at..].rfind(['+', '-']).map(|i| time_at + i) else {
        return Some((text, None));
    };
    let sign = if text.as_bytes()[sign_at] == b'-' {
        -1
    } else {
        1
    };
    let digits: String = text[sign_at + 1..].chars().filter(|c| *c != ':').collect();
    let ok = matches!(digits.len(), 2 | 4) && digits.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits.get(2..).map_or(Ok(0), str::parse).ok()?;
    (hours <= 23 && minutes <= 59)
        .then_some((&text[..sign_at], Some(sign * (hours * 60 + minutes))))
}

/// `-240` as "UTC-04:00" (ASCII, for agents' input).
pub fn offset_label(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let m = minutes.abs();
    format!("UTC{sign}{:02}:{:02}", m / 60, m % 60)
}

/// A world's clock: its date and time at branch clock 0 (local, with its UTC offset when the task
/// gives one), and how fast the branch clock runs (or that it is stepped: it moves when the team
/// acts or waits).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldClock {
    pub start: NaiveDateTime,
    pub utc_offset_min: Option<i32>,
    pub speed: f64,
    pub stepped: bool,
}

impl WorldClock {
    /// The clock of a task whose `clock_start` is `start` (see [`ClockStart::parse`]); None when
    /// it gives none or one that doesn't parse.
    pub fn of(start: Option<&str>, speed: f64) -> Option<WorldClock> {
        let start = ClockStart::parse(start?)?;
        Some(WorldClock {
            start: start.local,
            utc_offset_min: start.utc_offset_min,
            speed,
            stepped: false,
        })
    }

    /// The same clock, stepped when `stepped`.
    pub fn stepped_if(mut self, stepped: bool) -> WorldClock {
        self.stepped = stepped;
        self
    }

    /// The unix ms of a world's clock 0: the task's `clock_start` (its local time less its
    /// offset; without one, read as UTC), else the run's start (`run_start`).
    pub fn epoch_ms(start: Option<&str>, run_start: DateTime<Utc>) -> i64 {
        match start.and_then(ClockStart::parse) {
            Some(t) => t.epoch_ms(),
            None => run_start.timestamp_millis(),
        }
    }

    /// The world's local date and time at branch clock `now_ms`.
    pub fn at(&self, now_ms: i64) -> NaiveDateTime {
        self.start + Duration::milliseconds(now_ms.max(0))
    }

    /// "Thursday, 2026-08-13 06:02", with " (UTC-04:00)" when the world gives its offset.
    fn stamp(&self, t: NaiveDateTime) -> String {
        let day = t.format("%A, %Y-%m-%d %H:%M").to_string();
        match self.utc_offset_min {
            Some(m) => format!("{day} ({})", offset_label(m)),
            None => day,
        }
    }

    /// The section that opens an agent's first input: the world's date and time now, and that it
    /// is not today's.
    pub fn brief(&self, now_ms: i64) -> String {
        let pace = match self.speed {
            _ if self.stepped => "moves when the team acts or waits: each action takes world time, \
                                  thinking takes none, and when the whole team waits the world moves on to \
                                  the next thing that happens"
                .to_string(),
            s if (s - 1.0).abs() < 1e-9 => "runs while you work, with real time".to_string(),
            s => format!("runs while you work, at about {} times real time", trim_number(s)),
        };
        format!(
            "# World clock\n\nThis world keeps its own clock, which is not today's date. In this world \
             it is now {} (world time). The clock started at {} and {pace}. Read \
             every date and time in the world's systems against this clock, not against the date your \
             own environment reports: \"today\" and \"now\" mean the world's.\n\n",
            self.stamp(self.at(now_ms)),
            self.stamp(self.start),
        )
    }

    /// The line that ends each later input: the world's date and time now.
    pub fn now_line(&self, now_ms: i64) -> String {
        format!("\n(World time now: {}.)\n", self.stamp(self.at(now_ms)))
    }
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
    fn a_stepped_brief_says_the_clock_moves_when_the_team_acts_or_waits() {
        let b = clock(1.0).stepped_if(true).brief(0);
        assert!(b.contains("moves when the team acts or waits"), "{b}");
        assert!(!b.contains("runs while you work"), "{b}");
    }

    #[test]
    fn later_inputs_end_with_the_world_time_now() {
        assert_eq!(
            clock(3.0).now_line(3_600_000),
            "\n(World time now: Tuesday, 2026-03-10 07:00.)\n"
        );
    }

    #[test]
    fn a_clock_start_with_an_offset_gives_the_exact_epoch() {
        let run_start = chrono::Utc::now();
        // 00:00 at UTC-4 is 04:00 UTC.
        let epoch = WorldClock::epoch_ms(Some("2026-08-13T00:00:00-04:00"), run_start);
        assert_eq!(epoch, 1_786_593_600_000);
        assert_eq!(
            WorldClock::epoch_ms(Some("2026-08-13T00:00-04:00"), run_start),
            epoch
        );
        assert_eq!(
            WorldClock::epoch_ms(Some("2026-08-13T04:00Z"), run_start),
            epoch
        );
        assert_eq!(
            WorldClock::epoch_ms(Some("2026-08-13T09:30:00+0530"), run_start),
            epoch
        );
        // No offset: read as UTC, as before.
        let naive = WorldClock::epoch_ms(Some("2026-08-13T00:00:00"), run_start);
        assert_eq!(naive, 1_786_579_200_000);
        assert_eq!(
            WorldClock::epoch_ms(Some("2026-08-13T00:00"), run_start),
            naive
        );
        assert_eq!(ClockStart::parse("2026-08-13T00:00:00+25:00"), None);
        assert_eq!(ClockStart::parse("2026-08-13T00:00:00-4"), None);
    }

    #[test]
    fn the_local_start_stays_the_local_time_whatever_the_offset() {
        let c = ClockStart::parse("2026-08-13T06:00:00-04:00").unwrap();
        assert_eq!(c.local.to_string(), "2026-08-13 06:00:00");
        assert_eq!(c.utc_offset_min, Some(-240));
        let w = WorldClock::of(Some("2026-08-13T06:00:00-04:00"), 1.0).unwrap();
        assert_eq!(w.at(3_600_000).to_string(), "2026-08-13 07:00:00");
    }

    #[test]
    fn the_brief_shows_the_local_time_with_its_offset() {
        let w = WorldClock::of(Some("2026-08-13T06:00:00-04:00"), 1.0).unwrap();
        let b = w.brief(120_000);
        assert!(
            b.contains("it is now Thursday, 2026-08-13 06:02 (UTC-04:00) (world time)"),
            "{b}"
        );
        assert!(
            b.contains("started at Thursday, 2026-08-13 06:00 (UTC-04:00)"),
            "{b}"
        );
        assert_eq!(
            w.now_line(0),
            "\n(World time now: Thursday, 2026-08-13 06:00 (UTC-04:00).)\n"
        );
        assert_eq!(offset_label(330), "UTC+05:30");
    }
}
