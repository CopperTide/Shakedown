//! The operation registry: every operation Shakedown offers, defined once with its name, access
//! class and input schema. The HTTP routes and the control MCP server are both generated from it,
//! so the buttons, the API, the assistant and any agent can do the same things.

mod error;
pub mod http;
mod item_ops;
pub mod mcp;
mod project_ops;
pub mod stdio;
mod view;

use std::sync::OnceLock;

use futures_util::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde_json::Value;
use shakedown_project::format::Kind;

use crate::app::AppState;
use crate::project_state::ProjectState;

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
    /// The project folder.
    pub project: &'a ProjectState,
    /// The running shaked, if any.
    pub app: Option<&'a AppState>,
}

impl<'a> OpCtx<'a> {
    /// The context of a running shaked.
    pub fn of(app: &'a AppState) -> OpCtx<'a> {
        OpCtx {
            project: &app.project,
            app: Some(app),
        }
    }

    /// The context of a project alone, with no shaked.
    pub fn project_only(project: &'a ProjectState) -> OpCtx<'a> {
        OpCtx { project, app: None }
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
        ops
    })
}

/// The operation named `name`.
pub fn find(name: &str) -> Option<&'static Operation> {
    all().iter().find(|op| op.name == name)
}

/// Reads an operation's input; a malformed input is 400.
fn input<T: DeserializeOwned>(value: Value) -> Result<T, OpError> {
    serde_json::from_value(value).map_err(|e| OpError::bad_request(e.to_string()))
}

/// The JSON Schema of `T`, as a value.
fn schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}
