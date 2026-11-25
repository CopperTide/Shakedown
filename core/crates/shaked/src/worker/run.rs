//! `shaked worker --link <ws(s)-url> --ticket-file <path>`: the engine in a sandbox. It waits for
//! the ticket file (the sandbox starts the worker before it is handed out), reads the ticket and
//! deletes the file, then connects out to the control box's `/worker`.
//!
//! **The ticket file.** Whoever hands the ticket over writes it to a temporary file in the same
//! folder and renames it to `--ticket-file`, the ticket ending with a newline. The worker reads
//! the file only once it ends with a newline (a file still being written is left alone), trims it
//! and deletes it. `Start` brings the branch's
//! plan and worker token; the worker stands the world up and drives the agents, recording every
//! event through the link ([`LinkRecords`](super::LinkRecords)). It holds no database.
//!
//! The agents reach the worker's MCP endpoint, gateway and proxy on `127.0.0.1`. The gateway
//! sends every model call on to `Start.gateway` with the worker token
//! ([`crate::gateway::ForwardTo`]): the control box adds the run's key and records the call. CLI
//! agents get that gateway as their API's base URL, their agent token as the key. In a sandbox
//! (`deploy/sandbox`) the CLI agents run in containers ([`Jail`]): the MCP endpoint and gateway
//! are served on the jail's address too, the only port their network reaches.
//!
//! The link itself, and how it rejoins after a drop, is [`client`](super::client).
//!
//! The worker's shaked runs on [`WorkerLayer`]: no vault, no keys from the environment.

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use tokio::{sync::mpsc::UnboundedReceiver, task::JoinHandle};

pub use super::client::{open, Opened, Patience};
use super::frames::ToWorker;
use crate::{
    agents::jail::Jail,
    app::{AppState, Config},
    exec::BranchCtx,
    gateway::ForwardTo,
    layer::WorkerLayer,
    providers::{NamedKey, ProviderConfig, Secret},
    runner::{self, StartPoint},
    store::Store,
};

/// How often the ticket file is looked for.
const TICKET_POLL: Duration = Duration::from_millis(200);

/// `shaked worker`'s arguments.
#[derive(Debug, PartialEq, Eq)]
struct WorkerArgs {
    link: String,
    ticket_file: PathBuf,
}

const USAGE: &str = "usage: shaked worker --link <ws(s)://host/worker> --ticket-file <path>";

impl WorkerArgs {
    fn parse(args: &[String]) -> Result<WorkerArgs> {
        let (mut link, mut ticket_file) = (None, None);
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let value = it
                .next()
                .with_context(|| format!("{flag} needs a value; {USAGE}"))?;
            match flag.as_str() {
                "--link" => link = Some(value.clone()),
                "--ticket-file" => ticket_file = Some(PathBuf::from(value)),
                other => bail!("unknown option {other}; {USAGE}"),
            }
        }
        Ok(WorkerArgs {
            link: link.context(USAGE)?,
            ticket_file: ticket_file.context(USAGE)?,
        })
    }
}

/// Runs `shaked worker` with `args` (after `worker`) to the branch's end: an error (exit 1) when
/// the branch could not start or its link was lost for good.
pub async fn command(args: &[String]) -> Result<()> {
    let args = WorkerArgs::parse(args)?;
    let ticket = take_ticket(&args.ticket_file).await?;
    let opened = open(&args.link, &ticket, Patience::from_env()).await?;
    drive(opened).await
}

/// The ticket in `path`, once the file is there and ends with a newline; the file is deleted.
async fn take_ticket(path: &std::path::Path) -> Result<String> {
    loop {
        match tokio::fs::read_to_string(path).await {
            Ok(t) if t.ends_with('\n') && !t.trim().is_empty() => {
                tokio::fs::remove_file(path).await?;
                return Ok(t.trim().to_string());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        }
        tokio::time::sleep(TICKET_POLL).await;
    }
}

/// The worker's own shaked on `hub`, reached at `base`: no database (its records go over the
/// link), [`WorkerLayer`] (no vault, no keys from the environment), its model calls sent on
/// `to` the control box, priced knowing the `keys` its agents name, its CLI agents in the `jail`
/// when there is one.
fn worker_app(
    hub: parts::Hub,
    base: String,
    (to, keys): (ForwardTo, &[NamedKey]),
    jail: Option<Jail>,
) -> Result<Arc<AppState>> {
    let store = Arc::new(Store::detached(Arc::new(WorkerLayer))?);
    let providers = ProviderConfig::without_environment().pricing_named(keys);
    let mut config = Config::with_providers(providers);
    config.self_base = base;
    config.forward_to = Some(to);
    config.jail = jail;
    let state = Arc::new(AppState::new(store, Arc::new(hub), config));
    crate::app::install_default_drivers(&state);
    Ok(state)
}

/// The worker's shaked ([`worker_app`]) on the hub from the environment, serving its agents' MCP
/// endpoint, gateway (forwarding `to` the control box) and proxy on 127.0.0.1; the MCP endpoint
/// and gateway on the jail's address too, when the environment names a jail.
async fn worker_state(to: ForwardTo, keys: &[NamedKey]) -> Result<Arc<AppState>> {
    let jail = Jail::from_env().map_err(anyhow::Error::msg)?;
    let jailed = match &jail {
        Some(j) => Some(bind(j.listen).await?),
        None => None,
    };
    let listener = bind(([127, 0, 0, 1], 0).into()).await?;
    let hub = crate::serve::worker_hub().await?;
    let base = format!("http://{}", listener.local_addr()?);
    let state = worker_app(hub, base, (to, keys), jail)?;
    let agents = crate::mcp::router()
        .merge(crate::gateway::worker_router())
        .merge(crate::otel::receive::router());
    let agents = agents.with_state(state.clone());
    for l in jailed.into_iter().chain([listener]) {
        let agents = agents.clone();
        tokio::spawn(async move { axum::serve(l, agents).await });
    }
    crate::serve::start_proxy(&state).await?;
    Ok(state)
}

async fn bind(addr: std::net::SocketAddr) -> Result<tokio::net::TcpListener> {
    let l = tokio::net::TcpListener::bind(addr).await;
    l.with_context(|| format!("listening on {addr}"))
}

/// Pauses, resumes and stops the branch as the control box says; over the run's limit by the
/// control box's estimate, the branch stops and ends `budget_exceeded`.
async fn obey(
    state: Arc<AppState>,
    ctx: Arc<BranchCtx>,
    mut controls: UnboundedReceiver<ToWorker>,
) {
    while let Some(c) = controls.recv().await {
        let done = match &c {
            ToWorker::Pause => ctx.pause().await.map(|_| ()),
            ToWorker::Resume => ctx.resume().await.map(|_| ()),
            ToWorker::Stop => {
                runner::stop(&state, &ctx).await;
                Ok(())
            }
            ToWorker::OverBudget { why } => {
                ctx.budget.force_over(why.clone());
                runner::stop(&state, &ctx).await;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = done {
            eprintln!("worker: {c:?} failed: {e:#}");
        }
    }
}

/// Runs the stood-up branch to its end. The branch goes first when both are ready; a link that
/// ended Ok recorded the branch's end, so the branch is let finish. A link lost for good stops the
/// flow and the world and fails.
async fn until_end(
    state: &Arc<AppState>,
    ctx: &Arc<BranchCtx>,
    mut link: JoinHandle<Result<()>>,
) -> Result<()> {
    let branch = runner::run_branch(state.clone(), ctx.clone(), StartPoint::default());
    tokio::pin!(branch);
    tokio::select! {
        biased;
        () = &mut branch => link.await?.context(
            "the branch finished, but its end may not be recorded on the control box",
        ),
        lost = &mut link => match lost? {
            Ok(()) => {
                branch.await;
                Ok(())
            }
            Err(e) => {
                ctx.flow.stop();
                ctx.world.stop().await;
                Err(e)
            }
        },
    }
}

/// Stands the branch up and drives it to its end ([`until_end`]).
async fn drive(o: Opened) -> Result<()> {
    let Opened {
        plan,
        token,
        gateway,
        keys,
        records,
        controls,
        link,
    } = o;
    let token = Secret::new(token);
    let state = worker_state(ForwardTo { gateway, token }, &keys).await?;
    let ctx = match runner::stand_up(&state, &plan, records).await {
        Ok(ctx) => Arc::new(ctx),
        // Its end (`infra_error`) went over the link.
        Err(e) => bail!("the branch could not start: {e}"),
    };
    state.live.insert(ctx.clone());
    tokio::spawn(obey(state.clone(), ctx.clone(), controls));
    until_end(&state, &ctx, link).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn the_workers_shaked_has_no_vault_and_no_environment_keys() {
        let dir = tempfile::tempdir().unwrap();
        let registry = parts::Registry::scan(&[]).unwrap();
        let hub = parts::Hub::new(registry, dir.path().into(), dir.path().into());
        let to = ForwardTo {
            gateway: "http://127.0.0.1:9/gateway".into(),
            token: Secret::new("sdw_t"),
        };
        let state = worker_app(hub, "http://127.0.0.1:9".into(), (to, &[]), None).unwrap();
        let Err(refused) = state.vault() else {
            panic!("no vault in a worker");
        };
        assert!(refused.contains("worker"), "{refused}");
        assert!(!state.store.layer().keeps_secrets());
        assert!(!state.store.layer().policy().environment_keys);
        assert!(!state.config.providers.machine_credentials);
        let to = state
            .config
            .forward_to
            .as_ref()
            .expect("model calls go to the control box");
        assert_eq!(to.gateway, "http://127.0.0.1:9/gateway");
    }

    #[test]
    fn worker_reads_its_flags() {
        let a = WorkerArgs::parse(&args(&["--link", "wss://x/worker", "--ticket-file", "/t"]));
        let want = WorkerArgs {
            link: "wss://x/worker".into(),
            ticket_file: "/t".into(),
        };
        assert_eq!(a.unwrap(), want);
        assert!(WorkerArgs::parse(&args(&["--link", "wss://x/worker"])).is_err());
        assert!(WorkerArgs::parse(&args(&["--link"])).is_err());
        assert!(WorkerArgs::parse(&args(&["--nope", "1"])).is_err());
    }

    #[tokio::test]
    async fn the_ticket_is_waited_for_read_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ticket");
        let (p, write) = (path.clone(), async {
            tokio::time::sleep(TICKET_POLL * 2).await;
            std::fs::write(&path, "sdt_ab").unwrap();
            tokio::time::sleep(TICKET_POLL * 3).await;
            assert!(
                path.exists(),
                "a ticket without its newline is still being written"
            );
            std::fs::write(&path, "sdt_abc\n").unwrap();
        });
        let (got, ()) = tokio::join!(take_ticket(&p), write);
        assert_eq!(got.unwrap(), "sdt_abc");
        assert!(!p.exists());
    }
}
