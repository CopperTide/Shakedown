//! One part of a world: its live instance, starting it (from its save or empty), connecting it,
//! the hosts it serves, requests to its program, and stopping instances.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use parts::{
    proto::{
        CallToolParams, Describe, ExactKind, FromSave, HttpRoute, HttpRoutes, StartParams, Target,
        ToolOutput, WorldPart, WorldTimeParams,
    },
    request::{Connect, GetHttpRoutes, Stop},
    CallError, Client, Hub, Request,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{instance_id, PartStart, PlannedPart, StartSaves, WorldError};
use crate::restore::{in_shared_saves, keeps_saves};

pub(super) struct Part {
    pub(super) name: String,
    pub(super) integration: String,
    pub(super) instance: String,
    pub(super) client: Arc<Client>,
    pub(super) describe: Describe,
    /// True when the instance serves hosts through the proxy: its routes are asked again after
    /// each of its saves (a save that restarts a service may move it to a new port).
    pub(super) serves_http: bool,
}

impl Part {
    pub(super) fn label(&self) -> String {
        format!("the {} integration", self.name)
    }

    /// The part's live instance, as `inspect` and `diff` name it.
    pub(super) fn target(&self) -> Target {
        Target::Instance {
            instance: self.instance.clone(),
        }
    }

    /// True when the part keeps saves (a part with only replay is never saved).
    pub(super) fn keeps_saves(&self) -> bool {
        keeps_saves(&self.describe.restore)
    }

    /// True when the part is in the shared base and gold saves (so it can be checked).
    pub(super) fn in_shared_saves(&self) -> bool {
        in_shared_saves(&self.describe.restore)
    }

    /// `call_tool` on the part's instance.
    pub(super) async fn call_tool(
        &self,
        name: &str,
        args: Value,
        clock_ms: i64,
    ) -> Result<ToolOutput, WorldError> {
        let params = CallToolParams {
            instance: self.instance.clone(),
            name: name.to_string(),
            args,
            clock_ms,
        };
        request(&self.client, &self.label(), &params).await
    }
}

/// What starting one part came to: the part when it started (with its save and connection info),
/// and the error when starting or connecting it failed.
pub(super) struct StartedPart {
    pub(super) part: Option<Part>,
    pub(super) save: Option<String>,
    pub(super) connection: Option<Value>,
    /// The hosts the part's instance serves through the proxy.
    pub(super) routes: Vec<HttpRoute>,
    pub(super) error: Option<anyhow::Error>,
}

impl StartedPart {
    fn failed(part: Option<Part>, save: Option<String>, e: anyhow::Error) -> StartedPart {
        StartedPart {
            part,
            save,
            connection: None,
            routes: vec![],
            error: Some(e),
        }
    }
}

/// What a branch's parts are given at `start` besides their config: the world's time and where
/// they may call models (both None for base, gold and scratch worlds).
#[derive(Clone, Copy, Default)]
pub(super) struct Given<'a> {
    pub world_time: Option<WorldTimeParams>,
    pub models: Option<&'a super::models::WorldModels>,
}

/// Starts `p`'s instance for `branch_id` (from its save in `from`, if any) and connects it when a
/// benchmark may reach it.
pub(super) async fn start_part(
    hub: &Hub,
    p: PlannedPart,
    branch_id: &str,
    seed: u64,
    from: Option<&StartSaves>,
    given: Given<'_>,
) -> StartedPart {
    let (part, save) = match begin_part(hub, p, branch_id, seed, from, given).await {
        Ok(begun) => begun,
        Err(e) => return StartedPart::failed(None, None, e),
    };
    let connected = match connect(&part).await {
        Ok(connection) => connection,
        Err(e) => return StartedPart::failed(Some(part), save, e),
    };
    match http_routes(&part).await {
        Ok(routes) => StartedPart {
            part: Some(part),
            save,
            connection: connected,
            routes,
            error: None,
        },
        Err(e) => StartedPart::failed(Some(part), save, e),
    }
}

/// The part's instance started: the part, and the save it started from.
async fn begin_part(
    hub: &Hub,
    p: PlannedPart,
    branch_id: &str,
    seed: u64,
    from: Option<&StartSaves>,
    given: Given<'_>,
) -> Result<(Part, Option<String>)> {
    let instance = instance_id(branch_id, &p.part.part);
    let (mut params, save) = start_params(&p, &instance, seed, from)?;
    params.world_time = given.world_time;
    params.models = given.models.map(|m| m.of(&p.part.part));
    let part = Part {
        client: hub.client(&p.part.integration).await?,
        name: p.part.part,
        integration: p.part.integration,
        instance,
        describe: p.describe,
        serves_http: false,
    };
    let started: Result<Value, WorldError> = request(&part.client, &part.label(), &params).await;
    started.map_err(|e| anyhow::Error::from(e).context(super::StartingPart(part.name.clone())))?;
    Ok((part, save))
}

/// How `p` starts given `from` (None: every part empty). A part that is not in the shared saves
/// and isn't named starts fresh; any other part must be named.
fn part_start<'a>(p: &PlannedPart, from: Option<&'a StartSaves>) -> Result<Option<&'a FromSave>> {
    let Some(from) = from else {
        return Ok(None);
    };
    let name = &p.part.part;
    match from.get(name) {
        Some(PartStart::Save(f)) => Ok(Some(f)),
        Some(PartStart::Fresh) => Ok(None),
        None if !in_shared_saves(&p.describe.restore) => Ok(None),
        None => Err(anyhow!("there is no save to start the part {name} from")),
    }
}

/// `start` params of a part, and the save it starts from (None for an empty start).
fn start_params(
    p: &PlannedPart,
    instance: &str,
    seed: u64,
    from: Option<&StartSaves>,
) -> Result<(StartParams, Option<String>)> {
    let part: &WorldPart = &p.part;
    let mut params = StartParams {
        instance: instance.to_string(),
        config: part.config.clone(),
        seed,
        clock_ms: 0,
        from: None,
        world_time: None,
        models: None,
    };
    let Some(f) = part_start(p, from)? else {
        return Ok((params, None));
    };
    params.from = Some(f.clone());
    Ok((params, Some(f.save.clone())))
}

/// `connect` info of a part a benchmark may reach (exact saves only); None when it has none.
async fn connect(part: &Part) -> Result<Option<Value>> {
    if part.describe.restore.exact != ExactKind::Save {
        return Ok(None);
    }
    let doing = format!("connecting to the part {}", part.name);
    optional_request(part, &Connect::instance(&part.instance), &doing).await
}

/// The hosts a part's instance serves through the proxy; none when it doesn't answer
/// `http_routes`.
pub(super) async fn http_routes(part: &Part) -> Result<Vec<HttpRoute>> {
    let asked = GetHttpRoutes::instance(&part.instance);
    let doing = format!("asking the part {} which hosts it serves", part.name);
    let served: Option<HttpRoutes> = optional_request(part, &asked, &doing).await?;
    Ok(served.map(|s| s.routes).unwrap_or_default())
}

/// A request for an optional method of `part`: None when its program doesn't have the method; a
/// stopped program is the fatal message, any other failure says what it was `doing`.
async fn optional_request<T: DeserializeOwned>(
    part: &Part,
    r: &impl Request,
    doing: &str,
) -> Result<Option<T>> {
    match part.client.request::<T>(r).await {
        Ok(answer) => Ok(Some(answer)),
        Err(CallError::Part { code: -32601, .. }) => Ok(None),
        Err(CallError::Stopped(tail)) => bail!(stopped_message(&part.label(), &tail)),
        Err(e) => bail!("{doing}: {e}"),
    }
}

fn stopped_message(what: &str, tail: &str) -> String {
    match tail.trim() {
        "" => format!("{what} stopped"),
        tail => format!("{what} stopped: {tail}"),
    }
}

/// A request to a program; a stopped program is a fatal `WorldError` naming `what`.
pub(super) async fn request<T: DeserializeOwned, R: Request>(
    client: &Client,
    what: &str,
    r: &R,
) -> Result<T, WorldError> {
    let method = R::METHOD;
    client.request(r).await.map_err(|e| match e {
        CallError::Stopped(tail) => WorldError {
            message: stopped_message(what, &tail),
            fatal: true,
            code: None,
        },
        CallError::Part { code, .. } => WorldError {
            code: Some(code),
            ..WorldError::other(format!("{what}: {method} failed: {e}"))
        },
        other => WorldError::other(format!("{what}: {method} failed: {other}")),
    })
}

/// A running instance and the integration that runs it.
pub(super) struct Instance {
    pub(super) integration: String,
    pub(super) id: String,
}

pub(super) async fn stop_instances(hub: &Hub, instances: Vec<Instance>) {
    for Instance {
        integration,
        id: instance,
    } in instances
    {
        let r = async {
            hub.client(&integration)
                .await?
                .request::<Value>(&Stop::instance(&instance))
                .await
                .map_err(anyhow::Error::from)
        };
        if let Err(e) = r.await {
            eprintln!("world: could not stop {instance} ({integration}): {e:#}");
        }
    }
}
