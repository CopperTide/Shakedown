//! The operation registry: every operation Shakedown offers, defined once with its name, access
//! class and input schema. The HTTP routes and the control MCP server are both generated from it,
//! so the buttons, the API, the assistant and any agent can do the same things.

mod analytics_ops;
mod data_ops;
mod delete_ops;
mod deploy;
mod deploy_ops;
mod error;
mod example_ops;
mod explore_ops;
mod group_ops;
mod history_ops;
pub mod http;
mod import_git_ops;
mod import_ops;
mod item_ops;
mod madl_ops;
pub mod mcp;
mod migrate;
mod observability_ops;
mod project_ops;
mod restore_ops;
mod runtime_ops;
pub mod stdio;
mod step_ops;
mod view;

use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock};

use futures_util::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use shakedown_project::format::{EditPolicy, Kind};

use crate::app::AppState;
use crate::auth::Caller;
use crate::project_state::ProjectState;

pub(crate) use deploy::world_package;
pub(crate) use deploy_ops::register_imports;
pub use error::OpError;

/// What an operation can do, which decides who may call it without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Reads anything; always allowed.
    Read,
    /// Changes project files.
    Edit,
    /// Starts runs or costs money.
    Spend,
    /// Deletes something.
    Delete,
}

/// The future an operation's handler returns.
pub type OpFuture<'a> = BoxFuture<'a, Result<Value, OpError>>;

/// An operation's handler: its context, the operation itself (for its target kind) and the input.
pub type OpFn = for<'a> fn(&'a OpCtx<'a>, &'static Operation, Value) -> OpFuture<'a>;

/// What an operation runs against: the project, and the running shaked when there is one.
/// `shaked mcp` (stdio) runs project operations with no shaked, so no database is needed to
/// edit files; operations that start runs need `app`.
pub struct OpCtx<'a> {
    /// The caller's project folder.
    pub project: Arc<ProjectState>,
    /// The running shaked, if any.
    pub app: Option<&'a Arc<AppState>>,
    /// Who is calling: the person at this machine unless an API token says otherwise.
    pub caller: Arc<Caller>,
}

impl<'a> OpCtx<'a> {
    /// The context of a running shaked, for the person at this machine.
    pub fn of(app: &'a Arc<AppState>) -> OpCtx<'a> {
        OpCtx::as_caller(app, Arc::new(Caller::person()))
    }

    /// The context of a running shaked for `caller`.
    pub fn as_caller(app: &'a Arc<AppState>, caller: Arc<Caller>) -> OpCtx<'a> {
        OpCtx {
            project: app.project_for(&caller),
            app: Some(app),
            caller,
        }
    }

    /// The context of a project alone, with no shaked.
    pub fn project_only(project: Arc<ProjectState>) -> OpCtx<'a> {
        OpCtx {
            project,
            app: None,
            caller: Arc::new(Caller::person()),
        }
    }

    /// The running shaked, or 409 for an operation that needs one (over stdio there is none).
    pub fn app(&self, what: &str) -> Result<&'a Arc<AppState>, OpError> {
        let needs = || {
            OpError::new(
                axum::http::StatusCode::CONFLICT,
                format!("{what} needs a running shaked"),
            )
        };
        self.app.ok_or_else(needs)
    }
}

/// One operation.
#[derive(Debug)]
pub struct Operation {
    /// `<area>.<verb>`, e.g. `scenarios.put`.
    pub name: &'static str,
    /// The group it is listed under, e.g. `Scenarios`.
    pub area: &'static str,
    /// One line on what it does.
    pub summary: &'static str,
    pub access: Access,
    /// The HTTP method of its route.
    pub method: &'static str,
    /// The path of its route; `{id}` is taken into the input.
    pub path: &'static str,
    /// The kind of item it acts on, for the generic item operations.
    pub target: Option<Kind>,
    /// The JSON Schema of its input object.
    pub input_schema: fn() -> Value,
    pub run: OpFn,
}

/// Every operation, in a stable order.
pub fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut ops = project_ops::ops();
        ops.extend(item_ops::ops());
        ops.extend(history_ops::ops());
        ops.extend(data_ops::ops());
        ops.extend(group_ops::ops());
        ops.extend(restore_ops::ops());
        ops.extend(deploy_ops::ops());
        ops.extend(madl_ops::ops());
        ops.extend(import_ops::ops());
        ops.extend(example_ops::ops());
        ops.extend(import_git_ops::ops());
        ops.extend(runtime_ops::ops());
        ops.extend(step_ops::ops());
        ops.extend(observability_ops::ops());
        ops.extend(analytics_ops::ops());
        ops.extend(explore_ops::ops());
        ops
    })
}

/// The operation named `name`.
pub fn find(name: &str) -> Option<&'static Operation> {
    all().iter().find(|op| op.name == name)
}

/// Names one item.
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct IdInput {
    /// The item's id: lowercase letters, digits and dashes.
    id: String,
}

/// Reads an operation's input; a malformed input is 400.
fn input<T: DeserializeOwned>(value: Value) -> Result<T, OpError> {
    serde_json::from_value(value).map_err(|e| OpError::bad_request(e.to_string()))
}

/// A yes-or-no input, as JSON's `true`/`false` or, from a query string, `"true"`/`"false"`.
fn flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        Text(String),
    }
    match <Flag as serde::Deserialize>::deserialize(d)? {
        Flag::Bool(b) => Ok(b),
        Flag::Text(t) if t == "true" => Ok(true),
        Flag::Text(t) if t == "false" => Ok(false),
        Flag::Text(t) => Err(serde::de::Error::custom(format!(
            "expected true or false, got {t:?}"
        ))),
    }
}

/// The JSON Schema of `T`, as a value.
fn schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// An input body as YAML, to be parsed as a file would be; one that can't be is 400.
fn as_yaml(body: &Value) -> Result<String, OpError> {
    serde_yaml_ng::to_string(body).map_err(|e| OpError::bad_request(e.to_string()))
}

/// The unified diff of an edit from `old` to `new` YAML. When the project's edit `policy` shows a
/// diff first and the edit is not confirmed, breaks with the reply to give instead of writing.
fn diff_first(
    policy: EditPolicy,
    old: &str,
    new: &str,
    confirm: bool,
) -> ControlFlow<Value, String> {
    let diff = similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .to_string();
    if policy == EditPolicy::DiffFirst && !confirm {
        return ControlFlow::Break(json!({ "proposed": true, "diff": diff }));
    }
    ControlFlow::Continue(diff)
}

/// A request that names something unusable (422).
fn unprocessable(message: String) -> OpError {
    OpError::new(axum::http::StatusCode::UNPROCESSABLE_ENTITY, message)
}
