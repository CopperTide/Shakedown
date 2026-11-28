//! `project.example`: one small, real example added to the project when someone asks for it: a
//! world on the built-in retail benchmark, a scenario, an agent setup on the caller's own model key
//! and a one-setup experiment ready to deploy. Its content is data shipped with shaked
//! (`example/`, read in at build time), so it works wherever retail is built in, locally and
//! hosted. Without `confirm` it answers what it would write; it never overwrites an item: one
//! whose id is taken already makes it 409.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use axum::http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::{Kind, SetupFile, Team};
use shakedown_project::{item_file, kind_name, Id, ItemBody, Project, ProjectError};

use super::{flag, input, schema, unprocessable, Access, OpCtx, OpError, OpFuture, Operation};
use crate::app::AppState;
use crate::auth::Caller;
use crate::store::KeyRow;

#[cfg(test)]
mod tests;

const MANIFEST: &str = include_str!("../../example/example.yaml");
const WORLD: &str = include_str!("../../example/world.yaml");
const SCENARIO: &str = include_str!("../../example/scenario.yaml");
const SETUP: &str = include_str!("../../example/setup.yaml");
const EXPERIMENT: &str = include_str!("../../example/experiment.yaml");

/// What the project's history calls the change that adds the example.
const CHANGE: &str = "start from an example";

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "project.example",
        area: "Project",
        summary: "Adds a small, real example (a world, scenario, agent setup on your model key and an experiment) ready to deploy; shows what it would write first",
        access: Access::Edit,
        method: "POST",
        path: "/api/project/example",
        target: None,
        input_schema: schema::<ExampleInput>,
        run: example,
    }]
}

/// What `project.example` takes.
#[derive(Deserialize, JsonSchema)]
struct ExampleInput {
    /// Write now; without it, nothing is written and what would be is answered.
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    confirm: bool,
    /// The key-store name of the model key the agent runs on. Default: your newest key that has
    /// not failed its test (none when no key is stored: the default provider).
    key: Option<String>,
    /// The model the agent runs on. Default: the key's model, else the example's for its provider.
    model: Option<String>,
}

/// `example/example.yaml`: what the example is, its items' ids, and its models by provider.
#[derive(Deserialize)]
struct Manifest {
    summary: String,
    shows: String,
    items: Ids,
    models: BTreeMap<String, String>,
}

/// The id of each of the example's items.
#[derive(Deserialize)]
struct Ids {
    world: String,
    scenario: String,
    setup: String,
    experiment: String,
}

static EXAMPLE: LazyLock<Manifest> =
    LazyLock::new(|| serde_yaml_ng::from_str(MANIFEST).expect("example/example.yaml is valid"));

/// One item to write.
type NewItem = (Kind, Id, ItemBody);

/// The model key (empty: the default provider) and model the example's agent runs on.
#[derive(Debug, Clone, PartialEq)]
struct AgentModel {
    key: String,
    model: String,
}

fn example<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let ExampleInput {
            confirm,
            key,
            model,
        } = input(value)?;
        let agent = agent_model(ctx, key, model).await?;
        let items = items(&agent)?;
        let listed = listed(&items);
        let taken = taken(&ctx.project.current().await?, &items);
        if !taken.is_empty() {
            return Err(already_there(taken));
        }
        if !confirm {
            return Ok(answer(json!({ "proposed": true, "items": listed }), &agent));
        }
        let taken = ctx.project.change(|p| write_all(p, items)).await??;
        if !taken.is_empty() {
            return Err(already_there(taken));
        }
        Ok(answer(json!({ "written": listed }), &agent))
    })
}

/// `out` with what the example is, what it shows, the experiment to deploy and the agent's model.
fn answer(mut out: Value, agent: &AgentModel) -> Value {
    out["summary"] = json!(EXAMPLE.summary);
    out["shows"] = json!(EXAMPLE.shows);
    out["experiment"] = json!(EXAMPLE.items.experiment);
    out["agent"] = json!({ "key": agent.key, "model": agent.model });
    out
}

/// The example's items, its setup's agents on `agent`.
fn items(agent: &AgentModel) -> Result<Vec<NewItem>, OpError> {
    let ids = &EXAMPLE.items;
    let files = [
        (Kind::World, &ids.world, WORLD),
        (Kind::Scenario, &ids.scenario, SCENARIO),
        (Kind::Setup, &ids.setup, SETUP),
        (Kind::Experiment, &ids.experiment, EXPERIMENT),
    ];
    let item = |(kind, id, text): (Kind, &String, &str)| -> Result<NewItem, OpError> {
        let id = Id::parse(id)?;
        let mut body = ItemBody::parse(kind, &item_file(kind, &id), text)?;
        if let ItemBody::Setup(setup) = &mut body {
            run_on(setup, agent);
        }
        Ok((kind, id, body))
    };
    files.into_iter().map(item).collect()
}

/// Every built-in agent of `setup` on `agent`'s key and model.
fn run_on(setup: &mut SetupFile, agent: &AgentModel) {
    if let Team::Builtin { agents } = &mut setup.team {
        for a in agents {
            a.key.clone_from(&agent.key);
            a.model.clone_from(&agent.model);
        }
    }
}

fn listed(items: &[NewItem]) -> Vec<Value> {
    let one = |(kind, id, body): &NewItem| json!({ "kind": kind_name(*kind), "id": id.as_str(), "label": body.label() });
    items.iter().map(one).collect()
}

/// The items whose ids `project` has already, as `world order-desk`.
fn taken(project: &Project, items: &[NewItem]) -> Vec<String> {
    let there = |(kind, id, _): &&NewItem| project.get(*kind, id).is_some();
    let name = |(kind, id, _): &NewItem| format!("{} {id}", kind_name(*kind));
    items.iter().filter(there).map(name).collect()
}

/// Writes `items` as one change, or writes nothing and answers the ids taken meanwhile.
fn write_all(project: &mut Project, items: Vec<NewItem>) -> Result<Vec<String>, ProjectError> {
    let taken = taken(project, &items);
    if !taken.is_empty() {
        return Ok(taken);
    }
    project.name_change(CHANGE);
    for (kind, id, body) in items {
        project.put(kind, &id, body)?;
    }
    Ok(vec![])
}

fn already_there(taken: Vec<String>) -> OpError {
    let message = format!(
        "the example is in this project already ({}): delete or rename those items to add it again",
        taken.join(", ")
    );
    OpError::new(StatusCode::CONFLICT, message).with("taken", json!(taken))
}

/// What the caller has connected: a model key's name, its model and its provider kind (with no
/// key, the default provider's kind).
#[derive(Debug, Default)]
struct Connected {
    key: Option<String>,
    model: Option<String>,
    kind: Option<String>,
}

/// The key and model asked for, else those of the key the caller connected, else the example's
/// model for the provider. 422 when no model can be worked out.
async fn agent_model(
    ctx: &OpCtx<'_>,
    key: Option<String>,
    model: Option<String>,
) -> Result<AgentModel, OpError> {
    let key = key.filter(|k| !k.trim().is_empty());
    let found = match ctx.app {
        Some(app) => connected(app, &ctx.caller, key.as_deref()).await?,
        None => Connected::default(),
    };
    pick(key, model, found)
}

fn pick(
    key: Option<String>,
    model: Option<String>,
    found: Connected,
) -> Result<AgentModel, OpError> {
    let by_kind = |kind: &String| EXAMPLE.models.get(kind).cloned();
    let model = (model.filter(|m| !m.trim().is_empty()))
        .or(found.model)
        .or_else(|| found.kind.as_ref().and_then(by_kind));
    let model = model.ok_or_else(|| {
        unprocessable(
            "name the model the example's agent runs on: pass model (the model key names none)"
                .into(),
        )
    })?;
    let key = key.or(found.key).unwrap_or_default();
    Ok(AgentModel { key, model })
}

/// The key named `asked`, else the caller's newest key of their own that has not failed its test;
/// with neither, the default provider.
async fn connected(
    app: &AppState,
    caller: &Caller,
    asked: Option<&str>,
) -> Result<Connected, OpError> {
    let rows = app.store.at(caller).list_keys().await;
    let rows = rows.map_err(OpError::internal)?;
    let row = match asked {
        Some(name) => Some(named(&rows, name)?),
        None => own_working(app, caller, &rows).await?,
    };
    Ok(match row {
        Some(r) => Connected {
            key: Some(r.name.clone()),
            model: r.model.clone().filter(|m| !m.trim().is_empty()),
            kind: Some(r.kind.as_str().to_string()),
        },
        None => Connected {
            kind: default_kind(app, caller).await,
            ..Connected::default()
        },
    })
}

fn named<'r>(rows: &'r [KeyRow], name: &str) -> Result<&'r KeyRow, OpError> {
    let found = rows.iter().find(|r| r.name == name);
    found.ok_or_else(|| unprocessable(format!("no model key is named {name:?}")))
}

/// The newest of `rows` that is the caller's own and has not failed its test (rows come newest
/// first).
async fn own_working<'r>(
    app: &AppState,
    caller: &Caller,
    rows: &'r [KeyRow],
) -> Result<Option<&'r KeyRow>, OpError> {
    let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
    let mine = app.store.layer().own_keys(caller, &ids).await;
    let mine = mine.map_err(OpError::internal)?;
    let usable = |r: &&KeyRow| mine.contains(&r.id) && r.checked_ok != Some(false);
    Ok(rows.iter().find(usable))
}

/// The kind of the caller's default provider, e.g. `openrouter`.
async fn default_kind(app: &AppState, caller: &Caller) -> Option<String> {
    let config = crate::providers::keys::config_for(app, caller).await;
    let provider = config.resolve(None).ok()?;
    serde_json::to_value(provider.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}
