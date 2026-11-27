//! `runtimes.*`: the packages that run worlds (`crate::runtimes`). Installing one from a git repo
//! (or a folder of it) or an uploaded archive shows the plan first; confirmed, it installs the
//! Python dependencies with uv, registers the packages and runs their conformance check. A
//! runtime is code that runs on this machine, so hosted Shakedown refuses to install, check or
//! update one, as it refuses Import from git.

use base64::Engine;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::import_git_ops::{op_error, refuse_hosted};
use super::{input, schema, Access, IdInput, OpCtx, OpError, OpFn, OpFuture, Operation};
use crate::git_import::{self, record, Request, Source};
use crate::project_state::NoProject;
use crate::runtimes;

const RUNS: &str = "installing a runtime runs its code";

/// An operation of the Runtimes area.
fn op(
    name: &'static str,
    summary: &'static str,
    access: Access,
    route: (&'static str, &'static str),
    input_schema: fn() -> Value,
    run: OpFn,
) -> Operation {
    let (method, path) = route;
    Operation {
        name,
        area: "Runtimes",
        summary,
        access,
        method,
        path,
        target: None,
        input_schema,
        run,
    }
}

pub(super) fn ops() -> Vec<Operation> {
    vec![
        op("runtimes.list", "Lists the runtimes: each install (its source, packages, versions and conformance check) and the packages installed from folders, with the worlds each runs", Access::Read, ("GET", "/api/runtimes"), schema::<Empty>, list),
        op("runtimes.install", "Installs (or updates, with the id of an install) a runtime from a git URL, optionally a folder of the repo, or an uploaded .zip/.tar.gz: shows the plan first; confirmed, installs its Python dependencies with uv, registers its packages and runs their conformance check. Its code runs on this machine", Access::Edit, ("POST", "/api/runtimes"), schema::<InstallInput>, install),
        op("runtimes.check", "Runs the conformance check of an installed runtime's packages again", Access::Edit, ("POST", "/api/runtimes/{id}/check"), schema::<IdInput>, check),
        op("runtimes.delete", "Removes an installed runtime: unregistered, its files deleted; refused while a world uses it", Access::Delete, ("DELETE", "/api/runtimes/{id}"), schema::<IdInput>, remove),
    ]
}

/// No input.
#[derive(Deserialize, JsonSchema)]
struct Empty {}

/// What `runtimes.install` takes: `url` or `archive`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InstallInput {
    /// A git URL (https://, ssh://, git@host:path, file://) or an absolute path to a repo.
    #[serde(default)]
    url: Option<String>,
    /// A branch, a tag or a full commit SHA; default `HEAD`.
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    /// An uploaded .zip, .tar.gz or .tgz of a package folder or a repo holding packages.
    #[serde(default)]
    archive: Option<ArchiveInput>,
    /// The folder of the repo or archive whose packages to install, e.g. `packages/desk`; default
    /// all of them.
    #[serde(default)]
    path: Option<String>,
    /// The install's id (an install's id updates it); default the repo's or archive's name.
    #[serde(default)]
    id: Option<String>,
    /// Install now; without it, only the plan is answered.
    #[serde(default)]
    confirm: bool,
}

/// An uploaded archive.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArchiveInput {
    /// Its file name, which says its format (.zip, .tar.gz or .tgz).
    name: String,
    /// Its bytes, base64.
    data: String,
}

fn source(
    url: Option<String>,
    git_ref: Option<String>,
    archive: Option<ArchiveInput>,
) -> Result<Source, OpError> {
    match (url.filter(|u| !u.trim().is_empty()), archive) {
        (Some(url), None) => Ok(Source::Git { url, git_ref }),
        (None, Some(a)) => {
            let bytes = (base64::engine::general_purpose::STANDARD.decode(a.data.trim()))
                .map_err(|e| OpError::bad_request(format!("the archive is not base64: {e}")))?;
            Ok(Source::Archive {
                name: a.name,
                bytes,
            })
        }
        _ => Err(OpError::bad_request(
            "give a git url or an archive, not both",
        )),
    }
}

fn list<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, _: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let app = ctx.app("listing runtimes")?;
        let project = ctx.project.current().await.ok();
        let mut out = runtimes::list(&app.hub, ctx.project.root(), project.as_ref());
        out["hosted"] = json!(app.store.layer().policy().serves_others);
        Ok(out)
    })
}

fn install<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let InstallInput {
            url,
            git_ref,
            archive,
            path,
            id,
            confirm,
        } = input(value)?;
        refuse_hosted(ctx, RUNS)?;
        let app = ctx.app("installing a runtime")?;
        let root = ctx.project.root().ok_or(NoProject::NotConfigured)?;
        let source = source(url, git_ref, archive)?;
        let req = Request {
            source,
            id,
            packages_only: Some(path.unwrap_or_default()),
        };
        let mut out = git_import::import(root, req, confirm, Some(app.hub.as_ref()))
            .await
            .map_err(op_error)?;
        if confirm {
            let id = out["id"].as_str().unwrap_or_default().to_string();
            out["checks"] = json!(checked(app, root, &id).await?);
        }
        Ok(out)
    })
}

/// Runs the check of the install `id` in the project at `root`.
async fn checked(
    app: &crate::app::AppState,
    root: &std::path::Path,
    id: &str,
) -> Result<Vec<Value>, OpError> {
    let dir = record::imports_dir(root).join(id);
    let rec = (record::read(&dir).filter(|_| shakedown_project::Id::parse(id).is_ok()))
        .ok_or_else(|| {
            op_error(git_import::ImportError::NotFound(format!(
                "no runtime {id} is installed"
            )))
        })?;
    Ok(runtimes::check::run(&app.hub, &dir, &rec).await)
}

fn check<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = input::<IdInput>(value)?.id;
        refuse_hosted(ctx, RUNS)?;
        let app = ctx.app("checking a runtime")?;
        let root = ctx.project.root().ok_or(NoProject::NotConfigured)?;
        Ok(json!({"id": id, "checks": checked(app, root, &id).await?}))
    })
}

fn remove<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let id = input::<IdInput>(value)?.id;
        refuse_hosted(ctx, RUNS)?;
        let app = ctx.app("removing a runtime")?;
        let root = ctx.project.root().ok_or(NoProject::NotConfigured)?;
        let project = ctx.project.current().await.ok();
        (runtimes::remove(&app.hub, root, project.as_ref(), &id).await).map_err(op_error)
    })
}
