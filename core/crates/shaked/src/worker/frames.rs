//! The frames of the worker link: one JSON object per WebSocket text message, tagged by `type`.
//!
//! The control box assigns `seq`; `n` is the worker's own counter, used for the window, acks and
//! resends.

use crate::{
    model::{BranchStatus, EventKind},
    overhead::Overhead,
    runner::BranchPlan,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// How many events the worker may have sent without an `ack`.
pub const WINDOW: u64 = 64;
/// How long a worker keeps trying to reconnect before it gives up and stops.
pub const GIVE_UP: Duration = Duration::from_secs(300);

/// The `Refused` a worker token gets once it no longer works: the branch's end revoked it (or it
/// was never one, or expired).
pub const REFUSED_TOKEN: &str = "unknown, revoked or expired worker token";
/// What `Refused` starts with when a worker rejoins a branch that has ended (its status follows).
pub const REFUSED_ENDED: &str = "the branch ended";

/// Worker to control box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToControl {
    /// The first frame; `resume_from` is the last `n` the worker knows was acked.
    Hello { ticket: String, resume_from: u64 },
    Event {
        n: u64,
        agent: Option<String>,
        kind: EventKind,
        payload: Value,
        overhead: Option<Overhead>,
    },
    Ended {
        status: BranchStatus,
        error: Option<String>,
    },
}

/// Control box to worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToWorker {
    /// `gateway` is the control box's model gateway base URL.
    Start {
        plan: Box<BranchPlan>,
        token: String,
        gateway: String,
    },
    /// Sent in `n` order; `seq` is what the control box assigned.
    Ack {
        n: u64,
        seq: i64,
    },
    Pause,
    Resume,
    Stop,
    /// The control box closes the link after this.
    Refused {
        why: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentSpec, Rules, Runner, Team};
    use serde_json::json;

    fn plan() -> BranchPlan {
        let agent: AgentSpec = serde_json::from_value(
            json!({"id": "a", "name": "A", "kind": "open", "model": null, "prompt": "p"}),
        )
        .unwrap();
        BranchPlan {
            run_id: "r1".into(),
            branch_id: "b1".into(),
            task_id: "t".into(),
            benchmark: "bench".into(),
            team: Team {
                id: "tm".into(),
                name: "T".into(),
                agents: vec![agent],
            },
            rules: Rules::new(Runner::Chain),
            scenario: Default::default(),
            clock_from_ms: 0,
        }
    }

    fn round_trip<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
        frame: T,
        type_tag: &str,
    ) {
        let text = serde_json::to_string(&frame).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["type"], type_tag, "{text}");
        assert_eq!(serde_json::from_str::<T>(&text).unwrap(), frame);
    }

    #[test]
    fn control_frames_round_trip_as_tagged_json() {
        round_trip(
            ToControl::Hello {
                ticket: "t".into(),
                resume_from: 3,
            },
            "hello",
        );
        round_trip(
            ToControl::Event {
                n: 4,
                agent: Some("a".into()),
                kind: EventKind::Message,
                payload: json!({"text": "hi"}),
                overhead: Some(Overhead {
                    gateway: 12.0,
                    ..Default::default()
                }),
            },
            "event",
        );
        round_trip(
            ToControl::Ended {
                status: BranchStatus::Passed,
                error: None,
            },
            "ended",
        );
    }

    #[test]
    fn worker_frames_round_trip_as_tagged_json() {
        let start = ToWorker::Start {
            plan: Box::new(plan()),
            token: "k".into(),
            gateway: "http://g".into(),
        };
        round_trip(start, "start");
        round_trip(ToWorker::Ack { n: 4, seq: 90 }, "ack");
        round_trip(ToWorker::Pause, "pause");
        round_trip(ToWorker::Resume, "resume");
        round_trip(ToWorker::Stop, "stop");
        round_trip(ToWorker::Refused { why: "no".into() }, "refused");
    }

    #[test]
    fn an_unknown_type_fails_to_parse() {
        assert!(serde_json::from_str::<ToControl>(r#"{"type":"nope"}"#).is_err());
        assert!(serde_json::from_str::<ToWorker>(r#"{"type":"nope"}"#).is_err());
    }
}
