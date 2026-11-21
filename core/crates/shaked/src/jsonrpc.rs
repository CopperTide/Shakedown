//! JSON-RPC 2.0 envelopes shared by shaked's two MCP servers: the per-run world tools an agent
//! under test calls, and the control server that operates Shakedown itself.

use serde_json::{json, Value};

/// An error reply to request `id`.
pub fn rpc_error(id: Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}

/// A successful reply to request `id`.
pub fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// A tool's outcome as an MCP `tools/call` result: a failed tool is a result with `isError`,
/// not a protocol error.
pub fn tool_result(res: Result<Value, String>) -> Value {
    match res {
        Ok(v) => {
            let mut r =
                json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false});
            if v.is_object() {
                r["structuredContent"] = v;
            }
            r
        }
        Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
    }
}
