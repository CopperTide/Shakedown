//! A test's end of the worker link: a branch made by `record_start` with a ticket for its
//! worker, the client speaking the frames a `shaked worker` would, and a control box restarted on
//! the same store.
use std::{sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use shaked::{
    app::{AppState, Config},
    auth::Caller,
    launch::LaunchSpec,
    model::{Rules, Team},
    runner::{self, BranchPlan},
    scenario::Scenario,
    worker::{
        frames::{ToControl, ToWorker},
        Ticket,
    },
};
use tokio_tungstenite::tungstenite::Message;

use super::{app, AppFx};

pub type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The worker link of the control box at `base`.
pub fn ws_url(base: &str) -> String {
    format!("{}/worker", base.replace("http://", "ws://"))
}

/// A link to the control box at `base`, nothing said yet.
pub async fn connect(base: &str) -> Ws {
    tokio_tungstenite::connect_async(ws_url(base))
        .await
        .unwrap()
        .0
}

pub async fn say(ws: &mut Ws, f: &ToControl) {
    let text = serde_json::to_string(f).unwrap();
    ws.send(Message::text(text)).await.unwrap();
}

pub fn hello(ticket: &str, resume_from: u64) -> ToControl {
    ToControl::Hello {
        ticket: ticket.into(),
        resume_from,
    }
}

/// A link to the control box at `base` that said hello with `ticket` (a ticket or a worker
/// token), its last event `n`.
pub async fn joined(base: &str, ticket: &str, n: u64) -> Ws {
    let mut ws = connect(base).await;
    say(&mut ws, &hello(ticket, n)).await;
    ws
}

/// The next frame the control box sends, pings aside; None once it has closed the link.
pub async fn heard(ws: &mut Ws) -> Option<ToWorker> {
    loop {
        let next = tokio::time::timeout(Duration::from_secs(10), ws.next()).await;
        match next.expect("the control box answers within 10 s") {
            Some(Ok(Message::Text(t))) => return Some(serde_json::from_str(&t).unwrap()),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return None,
        }
    }
}

pub async fn hear(ws: &mut Ws) -> ToWorker {
    heard(ws).await.expect("a frame, not a closed link")
}

/// A ticket's claims for `plan`'s worker, as the person.
pub fn ticket_for(plan: &BranchPlan) -> Ticket {
    Ticket {
        branch_id: plan.branch_id.clone(),
        sandbox: uuid::Uuid::new_v4(),
        caller: Arc::new(Caller::person()),
    }
}

/// A branch of `task` made by `record_start` for `team` under `rules`, expected on the link, and
/// a ticket for its worker.
pub async fn ticketed(fx: &AppFx, task: &str, team: Team, rules: Rules) -> (BranchPlan, String) {
    let scenario = Scenario::default();
    let spec = LaunchSpec {
        scope: Arc::new(Caller::person()),
        task_id: task,
        team,
        scenario: &scenario,
        rules,
        driver: None,
        benchmark: None,
    };
    let plan = runner::record_start(&fx.state, &spec).await.unwrap();
    fx.state.links.expect(plan.clone());
    let ticket = fx.state.links.tickets.issue(ticket_for(&plan));
    (plan, ticket)
}

/// Says hello with `ticket`: the link and the worker token of the `Start` it was answered with.
pub async fn started(fx: &AppFx, ticket: &str) -> (Ws, String) {
    let mut ws = joined(&fx.base, ticket, 0).await;
    match hear(&mut ws).await {
        ToWorker::Start { token, .. } => (ws, token),
        other => panic!("start, not {other:?}"),
    }
}

/// The control box restarted: a new one on `fx`'s store with `config` (nothing in memory, no
/// timers), served at its base.
pub async fn restarted(fx: &AppFx, mut config: Config) -> (Arc<AppState>, String) {
    let (listener, base) = app::bind().await;
    config.self_base = base.clone();
    let state = AppState::new(fx.state.store.clone(), fx.state.hub.clone(), config);
    let state = Arc::new(state);
    app::serve(listener, &state);
    (state, base)
}
