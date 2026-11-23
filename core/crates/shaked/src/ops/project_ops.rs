//! The project itself: its status, and creating one.

use std::ops::ControlFlow;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::{parse, to_yaml, Kind, ProjectFile};
use shakedown_project::Project;

use super::{as_yaml, diff_first, flag, input, schema, view, Access, OpCtx, OpFuture, Operation};
use crate::project_state::NoProject;

pub(super) fn ops() -> Vec<Operation> {
    vec![
        project_op("project.status", Access::Read, "GET", schema::<NoInput>, status,
            "The project's name, settings, how many of each item it holds, and any problems in its files"),
        project_op("project.init", Access::Edit, "POST", schema::<InitInput>, init,
            "Creates a project in the configured folder"),
        project_op("project.update", Access::Edit, "PUT", schema::<UpdateSchema>, update,
            "Changes the project's settings: its name, the assistant's model and what agents may do without asking"),
    ]
}

/// An operation on the project itself, at `/api/project`.
fn project_op(
    name: &'static str,
    access: Access,
    method: &'static str,
    input_schema: fn() -> Value,
    run: super::OpFn,
    summary: &'static str,
) -> Operation {
    Operation {
        name,
        area: "Project",
        summary,
        access,
        method,
        path: "/api/project",
        target: None,
        input_schema,
        run,
    }
}

/// What `project.update` takes.
#[derive(Deserialize)]
struct UpdateInput {
    body: Value,
    #[serde(default, deserialize_with = "flag")]
    confirm: bool,
}

/// The schema of `project.update`'s input, with the body typed as the project file.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct UpdateSchema {
    /// The whole project file.
    body: shakedown_project::format::ProjectFile,
    /// Write now, without showing a diff first.
    confirm: Option<bool>,
}

/// No input.
#[derive(Deserialize, JsonSchema)]
pub(super) struct NoInput {}

/// What `project.init` takes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InitInput {
    /// The project's name.
    name: String,
    /// Track the folder with git.
    #[serde(default)]
    git: bool,
}

fn status<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move { Ok(status_of(&ctx.project.current()?)) })
}

fn init<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let InitInput { name, git } = input(value)?;
        let root = ctx.project.root().ok_or(NoProject::NotConfigured)?;
        let project = Project::init(root, &name, git)?;
        Ok(status_of(&project))
    })
}

fn update<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let UpdateInput { body, confirm } = input(value)?;
        let config: ProjectFile = parse("shakedown.yaml", &as_yaml(&body)?)?;
        let project = ctx.project.current()?;
        let (old, new) = (to_yaml(&project.config)?, to_yaml(&config)?);
        let policy = project.config.permissions.edit;
        if let ControlFlow::Break(proposal) = diff_first(policy, &old, &new, confirm) {
            return Ok(proposal);
        }
        ctx.project.change(|p| p.set_config(config))??;
        Ok(status_of(&ctx.project.current()?))
    })
}

fn status_of(project: &Project) -> Value {
    let count = |kind: Kind| project.list(kind).len();
    let problems: Vec<Value> = project.problems.iter().map(view::problem).collect();
    json!({
        "name": project.config.name,
        "counts": {
            "worlds": count(Kind::World),
            "scenarios": count(Kind::Scenario),
            "setups": count(Kind::Setup),
            "experiments": count(Kind::Experiment),
        },
        "problems": problems,
        "settings": project.config,
        "root": project.root.display().to_string(),
    })
}
