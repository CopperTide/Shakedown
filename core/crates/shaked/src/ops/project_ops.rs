//! The project itself: its status, and creating one.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::Project;

use super::{input, schema, view, Access, OpFuture, Operation};
use crate::app::AppState;
use crate::project_state::NoProject;

pub(super) fn ops() -> Vec<Operation> {
    vec![
        Operation {
            name: "project.status",
            area: "Project",
            summary:
                "The project's name, how many of each item it holds, and any problems in its files",
            access: Access::Read,
            method: "GET",
            path: "/api/project",
            target: None,
            input_schema: schema::<NoInput>,
            run: status,
        },
        Operation {
            name: "project.init",
            area: "Project",
            summary: "Creates a project in the configured folder",
            access: Access::Edit,
            method: "POST",
            path: "/api/project",
            target: None,
            input_schema: schema::<InitInput>,
            run: init,
        },
    ]
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

fn status<'a>(state: &'a AppState, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move { Ok(status_of(&state.project()?)) })
}

fn init<'a>(state: &'a AppState, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let InitInput { name, git } = input(value)?;
        let root = state.project.root().ok_or(NoProject::NotConfigured)?;
        let project = Project::init(root, &name, git)?;
        Ok(status_of(&project))
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
        "root": project.root.display().to_string(),
    })
}
