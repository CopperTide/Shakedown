//! Search over every run's steps against fixture runs in a fresh store: what a step is found by
//! (and what is never found), the snippet's marked match, the kind, agent, run and experiment
//! filters, pages, the planner using the index, and the `analytics.search` operation over HTTP.
mod support;

use serde_json::{json, Value};
use shaked::{
    analytics::SearchInput,
    model::*,
    ops::{self, Access},
    store::{FacetCount, SearchResults, Store},
};
use support::{pg, records, run, team_of, AppFx};

/// One run's steps, `[agent, kind, payload]`: a crew in every searchable way, and in two that are
/// not searchable (a model call's body, a world event).
fn steps() -> Value {
    json!([
        ["ops", "reasoning", {"text": "The crews are short at AUS; swap the reserve."}],
        ["ops", "message", {"to": "crew", "text": "Find a legal crew for UA525."}],
        ["crew", "tool_call", {"name": "crew_status", "args": {"crew_unit": "P0033"}, "result": {"station": "AUS"}, "error": null}],
        ["crew", "tool_call", {"name": "assign", "args": {}, "error": "refused: pilot over duty limit"}],
        ["ops", "model_call", {"request": {"messages": [{"role": "user", "content": "crew"}]}}],
        [null, "world_event", {"data": {"note": "crew"}}]
    ])
}

/// Records one run of [`steps`] for scenario `task`: `(run id, branch id)`.
async fn record(store: &Store, task: &str) -> (String, String) {
    let s = records(store);
    let agents = ["ops", "crew"].map(|a| AgentSpec::new(a, AgentKind::Open));
    let t = team_of(agents.into());
    let run = s.create_run(task, &t).await.unwrap();
    let b = s
        .create_branch(&run, None, &t, &json!([]), None)
        .await
        .unwrap();
    for step in steps().as_array().unwrap() {
        let kind: EventKind = serde_json::from_value(step[1].clone()).unwrap();
        s.append(&b, step[0].as_str(), kind, step[2].clone())
            .await
            .unwrap();
    }
    s.set_status(&b, BranchStatus::Passed, None).await.unwrap();
    (run, b)
}

async fn search(store: &Store, v: Value) -> SearchResults {
    let q = serde_json::from_value::<SearchInput>(v)
        .unwrap()
        .resolve()
        .unwrap();
    records(store).search_steps(&q).await.unwrap()
}

fn kinds(r: &SearchResults) -> Vec<&str> {
    r.hits.iter().map(|h| h.kind.as_str()).collect()
}

fn counts(f: &[FacetCount]) -> Vec<(&str, i64)> {
    f.iter().map(|f| (f.name.as_str(), f.count)).collect()
}

#[test]
fn steps_are_found_by_their_text_newest_first_with_the_match_marked() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("search_text").await;
        let (run_id, b) = record(&store, "refund").await;
        let r = search(&store, json!({"q": "crew"})).await;
        let want = ["tool_call", "message", "reasoning"];
        assert_eq!(
            kinds(&r),
            want,
            "newest first; no model call or world event"
        );
        assert_eq!((r.total, r.total_capped, r.next_offset), (3, false, None));
        let first = r.hits.last().unwrap();
        assert_eq!((&first.run_id, &first.branch_id), (&run_id, &b));
        assert_eq!(first.run_name, "refund");
        assert_eq!(first.agent.as_deref(), Some("ops"));
        assert!(first.seq > 0);
        let marked = "\u{E000}crews\u{E001}";
        assert!(first.snippet.contains(marked), "{}", first.snippet);
    });
}

#[test]
fn errors_arguments_phrases_and_left_out_words_are_searched() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("search_syntax").await;
        record(&store, "refund").await;
        let found = async |q: &str| search(&store, json!({ "q": q })).await;
        let refused = found("duty limit").await;
        assert_eq!(kinds(&refused), ["tool_call"], "a tool call's error");
        assert_eq!(kinds(&found("\"legal crew\"").await), ["message"]);
        assert_eq!(found("P0033").await.total, 1, "tool arguments");
        assert_eq!(found("crew -AUS").await.total, 1);
        assert_eq!(found("nothing-like-this").await.total, 0);
    });
}

#[test]
fn kind_and_agent_narrow_the_hits_and_facets_stay_whole() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("search_facets").await;
        record(&store, "refund").await;
        record(&store, "exchange").await;
        let r = search(&store, json!({"q": "crew", "kind": "reasoning"})).await;
        assert_eq!(kinds(&r), ["reasoning", "reasoning"]);
        let r = search(&store, json!({"q": "crew", "agent": ["crew"]})).await;
        assert_eq!(kinds(&r), ["tool_call", "tool_call"]);
        let by_kind = [("message", 2), ("reasoning", 2), ("tool_call", 2)];
        assert_eq!(counts(&r.kinds), by_kind, "facets ignore kind and agent");
        assert_eq!(counts(&r.agents), [("ops", 4), ("crew", 2)]);
    });
}

/// Puts run `run` in experiment `exp-1` (one setup, one slot).
async fn in_experiment(db: &str, run: &str) {
    let sql = format!(
        "INSERT INTO shakedown.suites (id, name, team, tasks, repeats, runner, benchmark, experiment_id)
           VALUES ('s1', 'x', '{{}}', '[]', 1, 'local', 'retail', 'exp-1');
         INSERT INTO shakedown.suite_runs (suite_id, ord, task_id, repeat, run_id)
           VALUES ('s1', 0, 'refund', 0, '{run}');"
    );
    pg::execute(db, &sql).await;
}

#[test]
fn run_and_experiment_narrow_the_hits_to_their_runs() {
    run(async {
        let (db, store, _clean) = pg::fresh_store("search_runs").await;
        let (run_a, _) = record(&store, "refund").await;
        let (run_b, _) = record(&store, "exchange").await;
        let only =
            |r: &SearchResults, id: &str| r.total == 3 && r.hits.iter().all(|h| h.run_id == id);
        let r = search(&store, json!({"q": "crew", "run": run_b})).await;
        assert!(only(&r, &run_b));
        in_experiment(&db, &run_a).await;
        let r = search(&store, json!({"q": "crew", "experiment": "exp-1"})).await;
        assert!(only(&r, &run_a));
        let other = search(&store, json!({"q": "crew", "experiment": "other"})).await;
        assert_eq!(other.total, 0);
    });
}

#[test]
fn pages_follow_next_offset_to_the_end() {
    run(async {
        let (_db, store, _clean) = pg::fresh_store("search_pages").await;
        record(&store, "refund").await;
        record(&store, "exchange").await;
        let mut seen = vec![];
        let mut offset = Some(0);
        while let Some(o) = offset {
            let r = search(&store, json!({"q": "crew", "limit": 4, "offset": o})).await;
            assert_eq!(r.total, 6);
            seen.extend(r.hits.iter().map(|h| (h.branch_id.clone(), h.seq)));
            offset = r.next_offset;
        }
        assert_eq!(seen.len(), 6);
        seen.dedup();
        assert_eq!(seen.len(), 6, "no step twice");
    });
}

#[test]
fn the_search_uses_the_index() {
    run(async {
        let (db, store, _clean) = pg::fresh_store("search_index").await;
        record(&store, "refund").await;
        let plan = pg::batch_last_text(
            &db,
            "SET enable_seqscan = off; EXPLAIN (FORMAT JSON)
            SELECT e.seq FROM shakedown.events e
             WHERE to_tsvector('english'::regconfig, shakedown.step_text(e.kind, e.payload))
                   @@ websearch_to_tsquery('english', 'crew')",
        )
        .await
        .unwrap();
        assert!(plan.contains("events_search"), "{plan}");
    });
}

#[test]
fn the_search_operation_reads_as_the_caller_over_http() {
    run(async {
        let op = ops::find("analytics.search").unwrap();
        assert_eq!((op.access, op.method), (Access::Read, "GET"));
        let fx = AppFx::idle("search", "op").start().await;
        record(&fx.state.store, "refund").await;
        let (code, r) = fx
            .get("/api/analytics/search?q=crew&kind=tool_call,message&limit=1")
            .await;
        assert_eq!(code, 200, "{r}");
        assert_eq!(
            (r["total"].as_i64(), r["next_offset"].as_i64()),
            (Some(2), Some(1))
        );
        assert_eq!(r["hits"][0]["kind"], "tool_call");
        assert_eq!(fx.get("/api/analytics/search").await.0, 400, "no words");
        assert_eq!(
            fx.get("/api/analytics/search?q=x&kind=model_call").await.0,
            400
        );
        assert_eq!(fx.get("/api/analytics/search?q=x&limit=ten").await.0, 400);
    });
}
