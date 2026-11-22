//! A branch's records: what it appends and reads back through `BranchRecords`, in the store.
use std::sync::Arc;

use serde_json::json;
use shaked::{
    auth::Caller,
    exec::{BranchRecords, StoreRecords},
    model::*,
    store::Store,
};
use support::{pg, records, run};

mod support;

fn team() -> Team {
    Team {
        id: "t".into(),
        name: "T".into(),
        agents: vec![AgentSpec::new("a", AgentKind::Open)
            .with_name("A")
            .with_prompt("p")],
    }
}

/// A store on a fresh database, and a branch of a new run in it.
async fn fresh_branch(clean: &pg::Cleanup) -> (Arc<Store>, String) {
    let name = pg::fresh_db("records", clean).await;
    let store = Arc::new(Store::open_named(&pg::admin(), &name).await.unwrap());
    let run_id = records(&store).create_run("task", &team()).await.unwrap();
    let id = records(&store)
        .create_branch(&run_id, None, &team(), &json!([]), None)
        .await
        .unwrap();
    (store, id)
}

#[test]
fn store_records_append_and_read_back_in_order() {
    run(async {
        let clean = pg::Cleanup::new();
        let (store, id) = fresh_branch(&clean).await;
        let caller = Arc::new(Caller::person());
        let branch = StoreRecords::new(store.clone(), caller, id.clone());

        let sent = [
            (EventKind::ToolCall, json!({"i": 1})),
            (EventKind::Message, json!({"i": 2})),
            (EventKind::ToolCall, json!({"i": 3})),
        ];
        for (kind, payload) in &sent {
            branch
                .append(Some("a"), *kind, payload.clone(), None)
                .await
                .unwrap();
        }

        let events = branch.events().await.unwrap();
        let seen: Vec<_> = events.iter().map(|e| (e.seq, e.kind, &e.payload)).collect();
        let want: Vec<_> = sent
            .iter()
            .zip(1..)
            .map(|((kind, payload), seq)| (seq, *kind, payload))
            .collect();
        assert_eq!(seen, want);

        branch.set_status(BranchStatus::Passed, None).await.unwrap();
        let stored = records(&store).branch(&id).await.unwrap();
        assert_eq!(stored.status, BranchStatus::Passed);
    });
}
