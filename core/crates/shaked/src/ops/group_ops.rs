//! World groups: renaming one moves every world in it, in one change. Like a put, it shows the
//! diff first under the project's "show a diff first" permission until sent with `confirm: true`.

use std::ops::ControlFlow;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use shakedown_project::format::Kind;
use shakedown_project::{item_file, ItemBody, Project, ProjectError};

use super::{diff_first, flag, input, schema, Access, OpCtx, OpError, OpFuture, Operation};

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "worlds.rename_group",
        area: "Worlds",
        summary: "Renames a world group, moving every world in it; an empty name ungroups them",
        access: Access::Edit,
        method: "POST",
        path: "/api/project/world-groups/rename",
        target: None,
        input_schema: schema::<RenameInput>,
        run: rename,
    }]
}

/// What a rename takes.
#[derive(Deserialize, JsonSchema)]
struct RenameInput {
    /// The group's name now.
    from: String,
    /// Its new name; empty or missing takes its worlds out of any group.
    #[serde(default)]
    to: Option<String>,
    /// Write now, without showing a diff first.
    #[serde(default, deserialize_with = "flag")]
    confirm: bool,
}

fn rename<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let RenameInput { from, to, confirm } = input(value)?;
        let to = to.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let project = ctx.project.current().await?;
        let (old, new) = both_sides(&project, &from, to)?;
        let policy = project.config.permissions.edit;
        let diff = match diff_first(policy, &old, &new, confirm) {
            ControlFlow::Continue(diff) => diff,
            ControlFlow::Break(proposal) => return Ok(proposal),
        };
        let moved = ctx.project.change(|p| p.rename_group(&from, to)).await??;
        let moved: Vec<&str> = moved.iter().map(|id| id.as_str()).collect();
        Ok(json!({ "moved": moved, "diff": diff }))
    })
}

/// Every world in the group as YAML before and after, one file after another under its name.
fn both_sides(
    project: &Project,
    from: &str,
    to: Option<&str>,
) -> Result<(String, String), OpError> {
    let (mut old, mut new) = (String::new(), String::new());
    for (id, world) in project.regrouped(from, to) {
        let head = format!("# {}\n", item_file(Kind::World, &id));
        let before = project.get(Kind::World, &id).map(|i| i.body.to_yaml());
        old += &(head.clone() + &before.transpose()?.unwrap_or_default());
        new += &(head + &ItemBody::World(world).to_yaml()?);
    }
    if old.is_empty() {
        return Err(ProjectError::no_group(from).into());
    }
    Ok((old, new))
}
