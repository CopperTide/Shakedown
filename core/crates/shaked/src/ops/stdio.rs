//! `shaked mcp`: the control MCP server over stdio, one JSON-RPC message per line. It runs as
//! the person at this machine on the project folder alone; no database or server is started.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    mcp::{handle, Asker},
    OpCtx,
};
use crate::{jsonrpc::rpc_error, project_state::ProjectState};

/// Serves stdin until it closes.
pub async fn run(project: &ProjectState) -> anyhow::Result<()> {
    let ctx = OpCtx::project_only(project);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&ctx, &Asker::Local, &req).await,
            Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
        };
        if let Some(reply) = reply {
            stdout.write_all(format!("{reply}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
