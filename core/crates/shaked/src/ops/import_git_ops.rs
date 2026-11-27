//! `project.import_git`: imports a git repo (a URL or a local path) into the project. Without
//! `confirm` it fetches the ref and answers what it found and would do; with it, it installs the
//! repo's Python packages, registers its packages and copies its project items
//! (`crate::git_import`).

use axum::http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{input, schema, Access, OpCtx, OpError, OpFuture, Operation};
use crate::git_import::{self, ImportError, Request};
use crate::project_state::NoProject;

pub(super) fn ops() -> Vec<Operation> {
    vec![Operation {
        name: "project.import_git",
        area: "Project",
        summary: "Imports a git repo (a URL or a local path, at a ref): installs and registers its packages and copies its worlds, scenarios, setups and experiments into the project; shows the plan first",
        access: Access::Edit,
        method: "POST",
        path: "/api/project/import-git",
        target: None,
        input_schema: schema::<ImportGitInput>,
        run: import_git,
    }]
}

/// What `project.import_git` takes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportGitInput {
    /// A git URL (https://, ssh://, git@host:path, file://) or an absolute path to a repo.
    url: String,
    /// A branch, a tag or a full commit SHA; default `HEAD`.
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    /// The import's id; default the repo's name.
    #[serde(default)]
    id: Option<String>,
    /// Install, register and write now; without it, only the plan is answered.
    #[serde(default)]
    confirm: bool,
}

fn import_git<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let ImportGitInput {
            url,
            git_ref,
            id,
            confirm,
        } = input(value)?;
        refuse_hosted(ctx, "importing from git runs the repo's code")?;
        let root = ctx.project.root().ok_or(NoProject::NotConfigured)?;
        let hub = ctx.app.map(|a| a.hub.as_ref());
        let req = Request::git(url, git_ref, id);
        let out = git_import::import(root, req, confirm, hub)
            .await
            .map_err(op_error)?;
        ctx.project.forget();
        Ok(out)
    })
}

/// 403 where this shaked serves others (hosted), for an operation that `runs` outside code (e.g.
/// "importing from git runs the repo's code").
pub(super) fn refuse_hosted(ctx: &OpCtx<'_>, runs: &str) -> Result<(), OpError> {
    if !ctx
        .app
        .is_some_and(|a| a.store.layer().policy().serves_others)
    {
        return Ok(());
    }
    Err(OpError::new(
        StatusCode::FORBIDDEN,
        format!("{runs} on this server: hosted Shakedown will run it in a run's sandbox, which is not built yet"),
    ))
}

pub(super) fn op_error(e: ImportError) -> OpError {
    let status = match e {
        ImportError::Bad(_) => StatusCode::BAD_REQUEST,
        ImportError::Conflict(_) => StatusCode::CONFLICT,
        ImportError::Failed(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ImportError::NotFound(_) => StatusCode::NOT_FOUND,
        ImportError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    OpError::new(status, e.message().to_string())
}
