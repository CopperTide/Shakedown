//! The world clock of a live branch: milliseconds that run with real time × speed while agents
//! work, and stand still while the world is paused (saving or restoring). A stepped clock runs on
//! no wall time at all: it moves only by what agents' tool calls cost (`charge`) and when the
//! runner moves it to the world's next wake point (`step_to`).

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use parts::proto::Effect;

/// The speed of new branch clocks: env `SHAKEDOWN_CLOCK_SPEED` (a positive number), default 1.
pub fn speed_from_env() -> f64 {
    parts::settings::var("SHAKEDOWN_CLOCK_SPEED")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .unwrap_or(1.0)
}

/// What agents' tool calls cost a stepped clock, in world ms: by the tool's effect, or the
/// tool's own cost when it has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepCost {
    pub read_ms: i64,
    pub write_ms: i64,
    pub tools: HashMap<String, i64>,
}

impl StepCost {
    /// What a stepped clock's calls cost, as a run's scenario gives it.
    pub fn of_clock(c: &parts::proto::SteppedClock) -> StepCost {
        StepCost {
            read_ms: c.read_ms,
            write_ms: c.write_ms,
            tools: c.tools.clone().into_iter().collect(),
        }
    }

    /// What a call of `tool`, which has `effect`, costs.
    pub fn of(&self, effect: Effect, tool: &str) -> i64 {
        let by_effect = match effect {
            Effect::Read => self.read_ms,
            Effect::Write => self.write_ms,
        };
        self.tools.get(tool).copied().unwrap_or(by_effect).max(0)
    }
}

struct State {
    /// Clock ms counted before `running_since` (all of them on a stepped clock).
    counted: f64,
    /// When the clock last started running; None while paused.
    running_since: Option<Instant>,
    /// Open pauses; the clock runs when there are none.
    pauses: u32,
    /// Set by `freeze`: the clock never runs again.
    frozen: bool,
    speed: f64,
    /// Set on a stepped clock: it never runs on wall time.
    stepped: Option<StepCost>,
}

impl State {
    fn now_ms(&self) -> f64 {
        let running = self.running_since.map_or(Duration::ZERO, |t| t.elapsed());
        self.counted + running.as_secs_f64() * 1000.0 * self.speed
    }

    /// Folds the running time into `counted` and restarts from now (when running).
    fn settle(&mut self) {
        self.counted = self.now_ms();
        if self.running_since.is_some() {
            self.running_since = Some(Instant::now());
        }
    }
}

pub struct BranchClock {
    state: Mutex<State>,
}

impl BranchClock {
    /// A running clock that starts at `base_ms` and runs at `speed` × real time.
    pub fn new(base_ms: i64, speed: f64) -> BranchClock {
        BranchClock {
            state: Mutex::new(State {
                counted: base_ms as f64,
                running_since: Some(Instant::now()),
                pauses: 0,
                frozen: false,
                speed,
                stepped: None,
            }),
        }
    }

    /// A stepped clock at `base_ms`: it stands still on wall time and moves only by `charge` and
    /// `step_to`.
    pub fn stepped(base_ms: i64, cost: StepCost) -> BranchClock {
        BranchClock {
            state: Mutex::new(State {
                counted: base_ms as f64,
                running_since: None,
                pauses: 0,
                frozen: false,
                speed: 1.0,
                stepped: Some(cost),
            }),
        }
    }

    /// True for a stepped clock.
    pub fn is_stepped(&self) -> bool {
        self.state.lock().unwrap().stepped.is_some()
    }

    /// Moves a stepped clock on by what a call of `tool` (with `effect`) costs; the ms it moved.
    /// A wall clock, or a frozen one, does not move.
    pub fn charge(&self, effect: Effect, tool: &str) -> i64 {
        let mut s = self.state.lock().unwrap();
        let Some(cost) = s.stepped.as_ref().filter(|_| !s.frozen) else {
            return 0;
        };
        let ms = cost.of(effect, tool);
        s.counted += ms as f64;
        ms
    }

    /// Moves a stepped clock to `ms`, never backwards. A wall clock, or a frozen one, does not
    /// move.
    pub fn step_to(&self, ms: i64) {
        let mut s = self.state.lock().unwrap();
        if s.stepped.is_some() && !s.frozen {
            s.counted = s.counted.max(ms as f64);
        }
    }

    /// The clock a branch under `rules` gets at `base_ms`: stepped when the rules say so, else
    /// running at the speed `SHAKEDOWN_CLOCK_SPEED` sets.
    pub fn for_rules(rules: &crate::model::Rules, base_ms: i64) -> BranchClock {
        match &rules.stepped {
            Some(c) => BranchClock::stepped(base_ms, StepCost::of_clock(c)),
            None => BranchClock::from_env(base_ms),
        }
    }

    /// A running clock from `base_ms` at the speed `SHAKEDOWN_CLOCK_SPEED` sets.
    pub fn from_env(base_ms: i64) -> BranchClock {
        BranchClock::new(base_ms, speed_from_env())
    }

    pub fn now_ms(&self) -> i64 {
        self.state.lock().unwrap().now_ms() as i64
    }

    /// How many clock ms pass per real ms while the clock runs.
    pub fn speed(&self) -> f64 {
        self.state.lock().unwrap().speed
    }

    /// Stops the clock until the matching `resume` (pauses nest).
    pub fn pause(&self) {
        let mut s = self.state.lock().unwrap();
        s.settle();
        s.pauses += 1;
        s.running_since = None;
    }

    /// Ends one `pause`; the clock runs again once every pause has ended.
    pub fn resume(&self) {
        let mut s = self.state.lock().unwrap();
        s.pauses = s.pauses.saturating_sub(1);
        if s.pauses == 0 && !s.frozen && s.stepped.is_none() && s.running_since.is_none() {
            s.running_since = Some(Instant::now());
        }
    }

    /// Stops the clock for good (a later `freeze` changes nothing; it never resumes).
    pub fn freeze(&self) {
        let mut s = self.state.lock().unwrap();
        s.settle();
        s.frozen = true;
        s.running_since = None;
    }

    /// True while the clock stands still (paused, or frozen).
    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().running_since.is_none()
    }

    /// Pauses the clock until the returned guard drops; the guard tells how long it was held.
    pub fn paused(&self) -> Paused<'_> {
        self.pause();
        Paused {
            clock: self,
            since: Instant::now(),
        }
    }
}

/// A pause of a `BranchClock`, ended on drop.
pub struct Paused<'a> {
    clock: &'a BranchClock,
    since: Instant,
}

impl Paused<'_> {
    /// Real milliseconds since the pause began.
    pub fn elapsed_ms(&self) -> i64 {
        self.since.elapsed().as_millis() as i64
    }

    /// Real milliseconds since the pause began, with the fraction kept.
    pub fn elapsed_ms_f64(&self) -> f64 {
        crate::overhead::ms_since(self.since)
    }
}

impl Drop for Paused<'_> {
    fn drop(&mut self) {
        self.clock.resume();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleep(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    #[test]
    fn runs_with_real_time_from_its_base() {
        let c = BranchClock::new(5000, 1.0);
        sleep(100);
        let t = c.now_ms();
        assert!((5090..5400).contains(&t), "{t}");
    }

    #[test]
    fn pauses_nest_and_stop_the_clock() {
        let c = BranchClock::new(0, 1.0);
        c.pause();
        let t = c.now_ms();
        {
            let p = c.paused();
            sleep(80);
            assert!(p.elapsed_ms() >= 80);
        }
        // Still paused: the outer pause has not ended.
        assert_eq!(c.now_ms(), t);
        c.resume();
        sleep(50);
        assert!(c.now_ms() >= t + 45);
        // A resume without a pause does nothing.
        c.resume();
        c.pause();
        let t = c.now_ms();
        sleep(30);
        assert_eq!(c.now_ms(), t);
    }

    #[test]
    fn a_frozen_clock_stays_frozen() {
        let c = BranchClock::new(0, 1.0);
        c.freeze();
        let t = c.now_ms();
        drop(c.paused());
        c.freeze();
        sleep(30);
        assert_eq!(c.now_ms(), t);
    }

    fn cost() -> StepCost {
        StepCost {
            read_ms: 0,
            write_ms: 120_000,
            tools: [("plan_recovery".to_string(), 300_000)].into(),
        }
    }

    #[test]
    fn stepped_clock_ignores_wall_time() {
        let c = BranchClock::stepped(1000, cost());
        assert!(c.is_stepped() && !BranchClock::new(0, 1.0).is_stepped());
        sleep(50);
        assert_eq!(c.now_ms(), 1000);
        drop(c.paused());
        sleep(20);
        assert_eq!(c.now_ms(), 1000, "a pause's end does not start it running");
    }

    #[test]
    fn charge_by_effect_and_tool_override() {
        let c = BranchClock::stepped(0, cost());
        assert_eq!(c.charge(Effect::Read, "flight"), 0);
        assert_eq!(c.now_ms(), 0);
        assert_eq!(c.charge(Effect::Write, "apply_schedule_change"), 120_000);
        assert_eq!(c.now_ms(), 120_000);
        assert_eq!(c.charge(Effect::Read, "plan_recovery"), 300_000);
        assert_eq!(c.now_ms(), 420_000);
        let wall = BranchClock::new(0, 0.0);
        assert_eq!(
            wall.charge(Effect::Write, "x"),
            0,
            "a wall clock is never charged"
        );
        c.freeze();
        c.charge(Effect::Write, "x");
        assert_eq!(c.now_ms(), 420_000, "a frozen clock stays where it is");
    }

    #[test]
    fn step_to_never_goes_back() {
        let c = BranchClock::stepped(0, cost());
        c.step_to(5_400_000);
        assert_eq!(c.now_ms(), 5_400_000);
        c.step_to(60_000);
        assert_eq!(c.now_ms(), 5_400_000);
        let wall = BranchClock::new(0, 0.0);
        wall.step_to(1_000);
        assert_eq!(wall.now_ms(), 0, "a wall clock moves with wall time only");
    }

    #[test]
    fn speed_scales_real_time() {
        let c = BranchClock::new(0, 4.0);
        sleep(50);
        assert!(c.now_ms() >= 190, "{}", c.now_ms());
    }
}
