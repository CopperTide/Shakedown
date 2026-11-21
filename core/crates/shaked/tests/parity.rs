//! Every registry operation is reachable both ways: as an HTTP route with a route-table row, and
//! as a tool of the control MCP server. A new operation that misses either surface fails here.

mod support;

use serde_json::{json, Value};
use shaked::harness::all_routes;
use shaked::ops::{self, mcp, OpCtx};
use shaked::project_state::ProjectState;
use support::run;

async fn mcp_tool_names() -> Vec<String> {
    let project = ProjectState::new(None);
    let ctx = OpCtx::project_only(std::sync::Arc::new(project));
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let reply = mcp::handle(&ctx, &mcp::Asker::Local, &req)
        .await
        .expect("a reply");
    let tools = reply["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    tools
        .iter()
        .filter_map(|t| t["name"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn every_op_on_both_surfaces() {
    run(async {
        let tools = mcp_tool_names().await;
        assert_eq!(tools.len(), ops::all().len(), "one tool per operation");
        for op in ops::all() {
            assert!(ops::find(op.name).is_some(), "{} can't be found", op.name);
            let routed = all_routes().any(|r| r.method == op.method && r.path == op.path);
            assert!(routed, "{} has no route row", op.name);
            assert!(
                tools.contains(&mcp::tool_name(op)),
                "{} is not an MCP tool",
                op.name
            );
        }
    });
}

#[test]
fn routes_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for op in ops::all() {
        assert!(
            seen.insert((op.method, op.path)),
            "{} {} served twice",
            op.method,
            op.path
        );
    }
}

#[test]
fn tool_schemas_describe_their_ids() {
    let get = ops::find("worlds.get").unwrap();
    let schema: Value = (get.input_schema)();
    assert_eq!(schema["required"], json!(["id"]), "{schema}");
}
