//! The worker links this control box keeps, by branch id: the plan each branch's worker is
//! started from, who the branch records for, the connection open now (if one is) and the last
//! events appended, so a resent event is acknowledged again without a query. The store keeps each
//! event's `n` too (`events.link_n`): a link lost with a restart is rebuilt from it.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::{mpsc, Semaphore};

use super::{
    frames::{ToWorker, REFUSED_ENDED, WINDOW},
    tickets::Tickets,
};

use crate::{
    auth::Caller,
    budget::{Budget, OVER_LIMIT},
    model::{BranchStatus, Hold},
    runner::BranchPlan,
    store::Controlled,
};

/// A linked branch's pool of model calls in flight at once is this many per agent of its team
/// (Claude Code makes some in parallel: subagents, a background call to a small model).
pub const INFLIGHT_PER_AGENT: usize = 4;

/// Who the spend a rebuilt link's branch had before the control box restarted is charged to.
const SPENT_BEFORE: &str = "(before the restart)";

/// What the control box holds a link's branch to, whatever its worker does: the run's spend
/// estimate and limit, the hold, whether its limits are on record at all, and how many agents its
/// team has (its pool of calls in flight is [`INFLIGHT_PER_AGENT`] times that).
pub struct HoldState {
    pub budget: Budget,
    pub hold: Hold,
    pub limits_known: bool,
    pub agents: usize,
}

impl HoldState {
    /// What `plan`'s branch starts held to: the control box's own rules.
    pub fn of_plan(plan: &BranchPlan) -> HoldState {
        HoldState {
            budget: Budget::new(plan.rules.max_cost_usd),
            hold: Hold::Running,
            limits_known: true,
            agents: plan.team.agents.len(),
        }
    }

    /// What a branch is held to after a restart, from what the store kept (never from the
    /// worker's own records).
    pub fn of_record(c: &Controlled) -> HoldState {
        let budget = Budget::new(c.limit.flatten());
        budget.add(SPENT_BEFORE, c.spent);
        if c.refused {
            budget.first_refusal();
        }
        HoldState {
            budget,
            hold: c.hold,
            limits_known: c.limit.is_some(),
            agents: c.agents,
        }
    }
}

/// The frame that tells a worker its branch is on `hold` (`why`: the reason of a stop for the
/// spend limit); None for going on unpaused.
pub fn frame_of(hold: Hold, why: &str) -> Option<ToWorker> {
    match hold {
        Hold::Running => None,
        Hold::Paused => Some(ToWorker::Pause),
        Hold::Stopping(BranchStatus::BudgetExceeded) => Some(ToWorker::OverBudget {
            why: why.to_string(),
        }),
        Hold::Stopping(_) => Some(ToWorker::Stop),
    }
}

/// Every branch a worker runs (or is about to) for this control box, and the tickets issued for
/// them.
#[derive(Default)]
pub struct Links {
    pub tickets: Tickets,
    branches: Mutex<HashMap<String, Arc<Link>>>,
    /// The branches whose end after the grace is armed (see `holds`).
    armed: Mutex<HashSet<String>>,
}

/// One branch's link.
pub struct Link {
    pub run_id: String,
    pub branch_id: String,
    /// What `Start` carries; None for a link rebuilt when its worker reconnected (a reconnect is
    /// never started).
    pub plan: Option<BranchPlan>,
    /// The run's spend estimate and limit as the control box keeps them: the gateway charges
    /// every model call of the branch here and refuses calls once it is over (402).
    pub budget: Budget,
    hold: Mutex<Hold>,
    /// False when the branch's rules are not on record: its calls are refused (503).
    pub limits_known: bool,
    /// [`INFLIGHT_PER_AGENT`] permits per agent: the model calls of the branch in flight at once.
    pub inflight: Arc<Semaphore>,
    wire: Mutex<Wire>,
    pub(super) appended: tokio::sync::Mutex<Appended>,
}

/// Who the branch records for (set when its ticket is redeemed) and the open connection's
/// generation and relay.
#[derive(Default)]
struct Wire {
    caller: Option<Arc<Caller>>,
    generation: u64,
    out: Option<mpsc::UnboundedSender<ToWorker>>,
}

/// The last `n` appended and the seqs of the last [`WINDOW`] appended here (a worker never has
/// more unacknowledged).
#[derive(Default)]
pub(super) struct Appended {
    pub(super) last_n: u64,
    recent: VecDeque<(u64, i64)>,
}

impl Appended {
    /// The seq event `n` got, while it is one of the last [`WINDOW`].
    pub(super) fn seq_of(&self, n: u64) -> Option<i64> {
        self.recent.iter().find(|(m, _)| *m == n).map(|(_, s)| *s)
    }

    /// The seq of event `n` when it is known here to be appended already; None when the store
    /// is to append it (or find it, appended before this link was rebuilt); why not when it
    /// would leave a gap.
    pub(super) fn resent(&self, n: u64) -> Result<Option<i64>, String> {
        match self.seq_of(n) {
            Some(seq) => Ok(Some(seq)),
            None if n == 0 => Err("events count from 1".into()),
            None if n <= self.last_n + 1 => Ok(None),
            None => Err(format!("event {n} came after {}", self.last_n)),
        }
    }

    /// Event `n` has `seq`.
    pub(super) fn push(&mut self, n: u64, seq: i64) {
        self.last_n = self.last_n.max(n);
        self.recent.push_back((n, seq));
        if self.recent.len() as u64 > WINDOW {
            self.recent.pop_front();
        }
    }
}

impl Links {
    /// A worker for `plan`'s branch will say hello: its ticket's `Start` carries `plan`.
    pub fn expect(&self, plan: BranchPlan) {
        let held = HoldState::of_plan(&plan);
        let mut link = Link::new((&plan.run_id, &plan.branch_id), Wire::default(), 0, held);
        link.plan = Some(plan);
        let id = link.branch_id.clone();
        self.branches.lock().unwrap().insert(id, Arc::new(link));
    }

    /// The link of `branch` (of `run`), rebuilt when this control box no longer has it (it
    /// restarted): started for `caller`, `last_n` its worker's last event in the store, `held`
    /// to what the store kept.
    pub(super) fn rejoined(
        &self,
        (run, branch): (&str, &str),
        caller: Arc<Caller>,
        last_n: u64,
        held: HoldState,
    ) -> Arc<Link> {
        let mut branches = self.branches.lock().unwrap();
        let wire = Wire {
            caller: Some(caller),
            ..Wire::default()
        };
        let made = || Arc::new(Link::new((run, branch), wire, last_n, held));
        branches
            .entry(branch.to_string())
            .or_insert_with(made)
            .clone()
    }

    /// Sends `f` to `branch`'s worker: false when no worker is connected.
    pub fn send(&self, branch: &str, f: ToWorker) -> bool {
        let Some(link) = self.get(branch) else {
            return false;
        };
        let wire = link.wire.lock().unwrap();
        wire.out.as_ref().is_some_and(|out| out.send(f).is_ok())
    }

    /// Puts `branch`'s link on `hold`, as the store has it already ([`Hold::then`]).
    pub fn set_hold(&self, branch: &str, hold: Hold) {
        if let Some(link) = self.get(branch) {
            let mut now = link.hold.lock().unwrap();
            *now = now.then(hold).unwrap_or(*now);
        }
    }

    /// Marks `branch`'s end after the grace armed: false when it was already.
    pub fn arm(&self, branch: &str) -> bool {
        self.armed.lock().unwrap().insert(branch.to_string())
    }

    /// `branch`'s end after the grace went off (or was not needed).
    pub fn disarm(&self, branch: &str) {
        self.armed.lock().unwrap().remove(branch);
    }

    /// Why `branch`'s run is over its limit: its link's reason ([`Link::limit_reason`]), else
    /// [`OVER_LIMIT`].
    pub fn limit_reason(&self, branch: &str) -> String {
        let reason = self.get(branch).map(|l| l.limit_reason());
        reason.unwrap_or_else(|| OVER_LIMIT.to_string())
    }

    /// True when this control box keeps a link for `branch` (its worker may not have started).
    pub fn has(&self, branch: &str) -> bool {
        self.get(branch).is_some()
    }

    /// True when `branch`'s worker is connected now.
    pub fn connected(&self, branch: &str) -> bool {
        self.get(branch)
            .is_some_and(|l| l.wire.lock().unwrap().out.is_some())
    }

    /// The link of `branch`, once its worker has started.
    pub fn started(&self, branch: &str) -> Option<Arc<Link>> {
        self.get(branch).filter(|l| l.caller().is_some())
    }

    pub(super) fn get(&self, branch: &str) -> Option<Arc<Link>> {
        self.branches.lock().unwrap().get(branch).cloned()
    }

    /// The branch ended (`status`): its link is no longer kept, and a connection still open is
    /// told so (`Refused`) and closed.
    pub fn forget(&self, branch: &str, status: BranchStatus) {
        let Some(link) = self.branches.lock().unwrap().remove(branch) else {
            return;
        };
        let out = link.wire.lock().unwrap().out.take();
        if let Some(out) = out {
            let why = format!("{REFUSED_ENDED} {}", status.as_str());
            let _ = out.send(ToWorker::Refused { why });
        }
    }
}

impl Link {
    fn new((run, branch): (&str, &str), wire: Wire, last_n: u64, held: HoldState) -> Link {
        let appended = Appended {
            last_n,
            recent: VecDeque::new(),
        };
        Link {
            run_id: run.to_string(),
            branch_id: branch.to_string(),
            plan: None,
            budget: held.budget,
            hold: Mutex::new(held.hold),
            limits_known: held.limits_known,
            inflight: Arc::new(Semaphore::new(INFLIGHT_PER_AGENT * held.agents.max(1))),
            wire: Mutex::new(wire),
            appended: tokio::sync::Mutex::new(appended),
        }
    }

    /// Why the run is over its limit: its estimate's reason, else [`OVER_LIMIT`].
    pub fn limit_reason(&self) -> String {
        self.budget.over().unwrap_or_else(|| OVER_LIMIT.to_string())
    }

    /// What the control box last told the worker.
    pub fn hold(&self) -> Hold {
        *self.hold.lock().unwrap()
    }

    /// Who the branch records for, once its worker has started.
    pub fn caller(&self) -> Option<Arc<Caller>> {
        self.wire.lock().unwrap().caller.clone()
    }

    /// Marks the branch started for `caller`: false when it had started already.
    pub(super) fn start_for(&self, caller: Arc<Caller>) -> bool {
        let mut wire = self.wire.lock().unwrap();
        if wire.caller.is_some() {
            return false;
        }
        wire.caller = Some(caller);
        true
    }

    /// Undoes [`start_for`](Self::start_for): the branch's worker could not be started.
    pub(super) fn release(&self) {
        self.wire.lock().unwrap().caller = None;
    }

    /// Makes a new connection the open one (a connection still open is dropped from the relay):
    /// its generation and the frames relayed to it.
    pub(super) fn attach(&self) -> (u64, mpsc::UnboundedReceiver<ToWorker>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut wire = self.wire.lock().unwrap();
        wire.generation += 1;
        wire.out = Some(tx);
        (wire.generation, rx)
    }

    /// True when the connection of `generation` is still the open one.
    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.wire.lock().unwrap().generation == generation
    }

    /// The connection of `generation` closed: nothing is relayed until the worker reconnects.
    pub(super) fn detach(&self, generation: u64) {
        let mut wire = self.wire.lock().unwrap();
        if wire.generation == generation {
            wire.out = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_keeps_the_last_window_of_seqs() {
        let mut a = Appended::default();
        for n in 1..=WINDOW + 2 {
            a.push(n, n as i64 + 10);
        }
        assert_eq!(a.last_n, WINDOW + 2);
        assert_eq!(a.seq_of(WINDOW + 2), Some(WINDOW as i64 + 12));
        assert_eq!(a.seq_of(3), Some(13));
        assert_eq!(a.seq_of(2), None);
        assert_eq!(a.resent(3), Ok(Some(13)));
        assert_eq!(a.resent(2), Ok(None), "older: the store finds it");
        assert_eq!(a.resent(WINDOW + 3), Ok(None));
        assert!(a.resent(WINDOW + 4).is_err());
        a.push(2, 12);
        assert_eq!(a.last_n, WINDOW + 2, "an older event leaves the last n");
    }
}
