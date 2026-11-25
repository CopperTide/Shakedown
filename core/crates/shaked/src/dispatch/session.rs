//! One agent's long-lived session in a dispatch run, served on its own task: a turn for each
//! input the runner sends, each turn's end passed back, and the session's totals at its end.

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use super::board::Input;
use crate::{
    agents::{session::Session, AgentResult},
    model::TokenUsage,
};

/// One agent's session, running on its own task.
pub(super) struct Slot {
    pub(super) inputs: mpsc::UnboundedSender<String>,
    pub(super) task: JoinHandle<()>,
}

impl Slot {
    /// `agent`'s `session` served on its own task: each turn's end goes to `turns`, the session's
    /// totals to `ends` once `stop` says so (a turn still going is cut off).
    pub(super) fn spawn(
        session: Box<dyn Session>,
        agent: &str,
        turns: mpsc::UnboundedSender<Input>,
        ends: mpsc::UnboundedSender<(String, AgentResult)>,
        stop: watch::Receiver<bool>,
    ) -> Slot {
        let (inputs, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(serve(session, agent.to_string(), rx, turns, ends, stop));
        Slot { inputs, task }
    }
}

/// Runs `agent`'s session: a turn for each input, each turn's end passed back, until the run
/// tells it to stop (a turn still going is cut off); then ends the session and passes back its
/// totals.
async fn serve(
    mut session: Box<dyn Session>,
    agent: String,
    mut inputs: mpsc::UnboundedReceiver<String>,
    turns: mpsc::UnboundedSender<Input>,
    ends: mpsc::UnboundedSender<(String, AgentResult)>,
    mut stop: watch::Receiver<bool>,
) {
    let mut cut = false;
    loop {
        let input = tokio::select! {
            i = inputs.recv() => i,
            _ = stop.wait_for(|s| *s) => None,
        };
        let Some(input) = input else { break };
        let turn = tokio::select! {
            t = session.turn(input) => Some(t),
            _ = stop.wait_for(|s| *s) => None,
        };
        let Some(t) = turn else {
            cut = true;
            break;
        };
        let ended = Input::TurnEnded {
            agent: agent.clone(),
            ok: t.ok,
            error: t.error,
            failure: t.failure,
            text: t.text,
            note: t.note,
            trimmed: t.trimmed,
        };
        let _ = turns.send(ended);
    }
    let result = session.end(cut).await;
    let _ = ends.send((agent, result));
}

/// An agent's end when its session never said how it ended.
pub(super) fn lost(agent: &str) -> AgentResult {
    AgentResult {
        ok: false,
        error: Some(format!(
            "{agent}'s session did not end in time when the run ended"
        )),
        tokens: TokenUsage::default(),
        cost_usd: None,
        cost_source: None,
        seconds: 0.0,
        failure: None,
    }
}
