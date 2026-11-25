//! Asking about a step of a run ([`crate::review::asking`]): the questions asked about a branch's
//! steps with their checked answers, and asking a new one. Both act on the branch their `{id}`
//! names, authorized as its routes are.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{flag, input, schema, Access, OpCtx, OpFuture, Operation};
use crate::review::asking::{branch_prefix, listed, start, step_prefix, Question};

/// Where both operations are served.
const PATH: &str = "/api/branches/{id}/questions";

pub(super) fn ops() -> Vec<Operation> {
    let op = |name, summary, access, method, input_schema, run| Operation {
        name,
        area: "Runs",
        summary,
        access,
        method,
        path: PATH,
        target: None,
        input_schema,
        run,
    };
    vec![
        op("questions.list", "The questions asked about a branch's steps (or one step's), oldest first, each with its checked answer: evidence quoted from the record with its step and field, and speculation labelled as such",
            Access::Read, "GET", schema::<ListInput>, list as super::OpFn),
        op("questions.ask", "Asks a model, on your own key, a question about one step of a branch (such as why the agent said or did it); the answer quotes what the agent had at that step as evidence, checked against the record, apart from speculation. Costs a model call",
            Access::Spend, "POST", schema::<AskInput>, ask),
    ]
}

/// A step number, from JSON or a query string.
#[derive(Deserialize)]
#[serde(untagged)]
enum Seq {
    Number(i64),
    Text(String),
}

impl Seq {
    fn value(&self) -> Result<i64, super::OpError> {
        match self {
            Seq::Number(n) => Ok(*n),
            Seq::Text(t) => t.trim().parse().map_err(|_| {
                super::OpError::bad_request(format!("seq must be a step number, not {t:?}"))
            }),
        }
    }
}

/// Which questions to list.
#[derive(Deserialize, JsonSchema)]
struct ListInput {
    /// The branch.
    id: String,
    /// Only the questions about this step.
    #[serde(default)]
    #[schemars(with = "Option<i64>")]
    seq: Option<Seq>,
}

/// A question to ask.
#[derive(Deserialize, JsonSchema)]
struct AskInput {
    /// The branch.
    id: String,
    /// The step asked about.
    #[schemars(with = "i64")]
    seq: Seq,
    /// The question, such as "Why did the agent make this reasoning statement?".
    question: String,
    /// The explainer, as `<provider>:<model>` (or `claude_code:<model>` locally); the AI review
    /// model when missing.
    #[serde(default)]
    model: Option<String>,
    /// Answer once the explainer has answered, instead of at once with the question `running`.
    #[serde(default, deserialize_with = "flag")]
    #[schemars(with = "bool")]
    wait: bool,
}

fn list<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let ListInput { id, seq } = input(value)?;
        let app = ctx.app("listing questions")?;
        let prefix = match seq {
            Some(s) => step_prefix(&id, s.value()?),
            None => branch_prefix(&id),
        };
        let rows = app.store.at(&ctx.caller).asks(&prefix).await?;
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|r| listed(app, &ctx.caller, r))
            .collect();
        Ok(json!({ "questions": rows }))
    })
}

fn ask<'a>(ctx: &'a OpCtx<'a>, _: &'static Operation, value: Value) -> OpFuture<'a> {
    Box::pin(async move {
        let i: AskInput = input(value)?;
        let app = ctx.app("asking about a step")?;
        let q = Question {
            branch: &i.id,
            seq: i.seq.value()?,
            question: &i.question,
            model: i.model.as_deref(),
        };
        let (row, task) = start(app, &ctx.caller, q).await?;
        if !i.wait {
            return Ok(listed(app, &ctx.caller, row));
        }
        let _ = task.await;
        let done = app.store.at(&ctx.caller).review(&row.key).await?;
        Ok(done.map_or(Value::Null, |r| listed(app, &ctx.caller, r)))
    })
}
