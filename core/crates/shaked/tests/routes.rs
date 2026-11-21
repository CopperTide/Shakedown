//! Every `/api` route is in the route table, and asks the layer before it acts: with a layer that
//! denies everything, each answers 404.

use std::{collections::BTreeSet, sync::Arc};

use serde_json::json;
use shaked::{
    auth::Decision,
    harness::{all_routes, SpyLayer, ROUTES},
};
use support::{run, AppFx};

mod support;

/// Every `(METHOD, path)` the sources of `src/api.rs` and `src/api/*.rs` route.
fn routed() -> BTreeSet<(String, String)> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("api.rs")];
    for e in std::fs::read_dir(src.join("api")).unwrap().flatten() {
        files.push(e.path());
    }
    let mut out = BTreeSet::new();
    for f in files
        .iter()
        .filter(|f| f.extension().is_some_and(|x| x == "rs"))
    {
        let text = std::fs::read_to_string(f).unwrap();
        out.extend(routes_in(&text));
    }
    out
}

/// The index in `s` of the `)` that closes a call opened before `s` starts.
fn closing(s: &str) -> usize {
    let mut depth = 1;
    for (k, c) in s.char_indices() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth == 0 {
            return k;
        }
    }
    s.len()
}

/// True when `call` names `m(` as a word (`get(`, not `budget(`).
fn names_method(call: &str, m: &str) -> bool {
    let b = call.as_bytes();
    call.match_indices(&format!("{m}("))
        .any(|(at, _)| at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_'))
}

/// The routes of one file: `.route("<path>", <methods>)`, the methods read inside the call.
fn routes_in(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(".route(") {
        rest = rest[i + 7..].trim_start();
        let Some(quoted) = rest.strip_prefix('"') else {
            // A path built at run time: the table cannot be checked against it.
            out.push((
                "?".into(),
                format!("a route whose path is not a literal: {:.40}", rest),
            ));
            continue;
        };
        rest = quoted;
        let end = rest.find('"').unwrap();
        let path = rest[..end].to_string();
        let call = &rest[end..end + closing(&rest[end..])];
        for m in ["get", "post", "put", "delete", "patch"] {
            if names_method(call, m) {
                out.push((m.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    out
}

#[test]
fn the_route_table_names_every_route() {
    let table: BTreeSet<(String, String)> = ROUTES
        .iter()
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    let routed = routed();
    let missing: Vec<_> = routed.difference(&table).collect();
    assert!(missing.is_empty(), "routes the table lacks: {missing:?}");
    let extra: Vec<_> = table.difference(&routed).collect();
    assert!(extra.is_empty(), "table rows no source routes: {extra:?}");
}

/// `path` with every `{param}` filled in and the catch-all made concrete.
fn fill(path: &str) -> String {
    path.split('/')
        .map(|s| match s {
            "{*rest}" => "no-such-thing",
            s if s.starts_with('{') => "x1",
            s => s,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn every_api_route_asks_the_layer() {
    run(async {
        let spy = SpyLayer::deny(Decision::NotFound);
        let fx = AppFx::idle("routes", "deny")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        for r in all_routes().filter(|r| !r.public) {
            let url = format!("{}{}", fx.base, fill(r.path));
            let req = match r.method {
                "GET" => fx.http.get(&url),
                "POST" => fx.http.post(&url).json(&json!({})),
                "PUT" => fx.http.put(&url).json(&json!({})),
                "DELETE" => fx.http.delete(&url),
                m => panic!("method {m}"),
            };
            let status = req.send().await.unwrap().status().as_u16();
            assert_eq!(status, 404, "{} {}", r.method, r.path);
            assert!(
                spy.asked(r.action),
                "{} {} never asked for {:?}",
                r.method,
                r.path,
                r.action
            );
        }
    })
}

#[test]
fn health_needs_no_caller() {
    run(async {
        let spy = SpyLayer::refuse_everyone();
        let fx = AppFx::idle("routes", "health")
            .layer(Arc::new(spy.clone()))
            .start()
            .await;
        assert_eq!(fx.get("/api/health").await.0, 200);
        assert_eq!(fx.get("/api/runs").await.0, 401);
    })
}

#[test]
fn every_op_has_a_route_row() {
    for op in shaked::ops::all() {
        let row = all_routes().find(|r| r.method == op.method && r.path == op.path);
        assert!(
            row.is_some(),
            "{} ({} {}) has no route row",
            op.name,
            op.method,
            op.path
        );
    }
}
