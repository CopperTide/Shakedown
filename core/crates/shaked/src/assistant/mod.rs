//! The assistant: a chat on the user's own model that can do anything the operation registry
//! offers. Reads run right away; edits come back as diffs to apply; anything that starts runs,
//! spends money or deletes comes back as a card the person confirms. Nothing it proposes is
//! applied without a person.

mod prompt;
mod tools;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ops::{OpCtx, OpError};
use crate::providers::Provider;

pub use tools::Card;

/// The most model turns one question may take.
pub const MAX_TURNS: usize = 8;

/// A question: the conversation so far (`{role, content}`) and the page it was asked on.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Value>,
    #[serde(default)]
    pub page: Option<String>,
}

/// A tool the assistant used, for the collapsed list under its answer.
#[derive(Debug, Serialize)]
pub struct ToolUse {
    pub name: String,
    pub summary: String,
}

/// The answer: the reply, the conversation to send next time, the tools used and the cards.
#[derive(Debug, Default, Serialize)]
pub struct ChatAnswer {
    pub reply: String,
    pub messages: Vec<Value>,
    pub tools: Vec<ToolUse>,
    pub cards: Vec<Card>,
}

/// Answers `req` with `model` on `provider`, using the registry's operations as tools.
pub async fn run_chat(
    ctx: &OpCtx<'_>,
    provider: &Provider,
    http: &reqwest::Client,
    model: &str,
    req: ChatRequest,
) -> Result<ChatAnswer, OpError> {
    let mut convo = vec![prompt::system(ctx, req.page.as_deref())];
    convo.extend(req.messages);
    let mut answer = ChatAnswer::default();
    for _ in 0..MAX_TURNS {
        let message = one_turn(provider, http, model, &convo).await?;
        convo.push(message.clone());
        let calls = message["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            answer.reply = message["content"].as_str().unwrap_or_default().to_string();
            return Ok(finish(answer, convo));
        }
        for call in &calls {
            convo.push(tools::dispatch(ctx, call, &mut answer).await);
        }
    }
    answer.reply =
        format!("Stopped after {MAX_TURNS} steps without an answer. Ask again more narrowly.");
    Ok(finish(answer, convo))
}

/// The conversation without the system prompt, for the next question.
fn finish(mut answer: ChatAnswer, convo: Vec<Value>) -> ChatAnswer {
    answer.messages = convo.into_iter().skip(1).collect();
    answer
}

/// One model call: the assistant's next message.
async fn one_turn(
    provider: &Provider,
    http: &reqwest::Client,
    model: &str,
    convo: &[Value],
) -> Result<Value, OpError> {
    let request = json!({ "model": model, "messages": convo, "tools": tools::specs() });
    let failed = |why: String| {
        OpError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("the model call failed: {why}"),
        )
    };
    let up = provider.send(http, &request).await.map_err(failed)?;
    if !(200..300).contains(&up.status) {
        return Err(failed(format!("{} {}", up.status, up.response)));
    }
    Ok(up.response["choices"][0]["message"].clone())
}
