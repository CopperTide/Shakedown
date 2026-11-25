//! Asking about a step over HTTP, on a scripted model (no network): the question is checked, the
//! explainer reads what the agent had at that step, evidence whose quote is not in the record is
//! dropped, and the answer is stored under the step with what it cost.

#[path = "review_routes/setup.rs"]
mod setup;
mod support;

use serde_json::{json, Value};
use shaked::model::{BranchStatus, EventKind};
use support::{fake_llm::fake_llm, records, run, AppFx};

const POLICY: &str = "Refunds over $500 need a manager's approval.";

/// The explainer's answer: one quote the record has, one it does not.
fn explainer_answer() -> Value {
    let answer = json!({
        "answer": "The agent followed the refund rule in its system prompt.",
        "evidence": [
            {"source": "m1.0", "quote": POLICY, "why": "the rule it applied"},
            {"source": "m1.0", "quote": "Refunds are always fine.", "why": "invented"},
        ],
        "speculation": [{"text": "Likely anchored on the $500 line.", "based_on": ["m1.0"]}],
    });
    json!({"choices": [{"message": {"role": "assistant", "content": answer.to_string()}}],
           "usage": {"prompt_tokens": 900, "completion_tokens": 100}})
}

/// A finished branch: a model call whose system prompt is the policy, then a thought from it.
async fn branch(fx: &AppFx) -> String {
    let id = setup::branch(fx, Some(BranchStatus::Passed)).await;
    let store = records(&fx.state.store);
    let request =
        json!({"messages": [{"role": "system", "content": format!("You are solo. {POLICY}")}]});
    let response = json!({"choices": [{"message": {"content": "Asking a manager."}}]});
    let call = json!({"request": request, "response": response, "usage": {}, "cost_usd": null, "cost_source": null});
    let first = store
        .append(&id, Some("solo"), EventKind::ModelCall, call)
        .await
        .unwrap();
    assert_eq!(first.seq, 1, "the explainer's answer cites m1.0");
    let thought = json!({"text": "Over $500, so I ask.", "kind": "thinking", "source": "gateway", "model_call_seq": 1});
    store
        .append(&id, Some("solo"), EventKind::Reasoning, thought)
        .await
        .unwrap();
    id
}

#[test]
fn an_answer_keeps_only_evidence_the_record_has_and_is_stored_under_its_step() {
    setup::review("on");
    run(async {
        let llm = fake_llm(vec![(200, explainer_answer())]).await;
        let fx = setup::app("ask", Some(llm.base.clone())).await;
        let b = branch(&fx).await;
        let path = format!("/api/branches/{b}/questions");
        refused(&fx, &path).await;
        assert!(
            llm.seen.lock().unwrap().is_empty(),
            "no model call before the checks pass"
        );
        answered(&fx, &path).await;
        let seen = llm.seen.lock().unwrap().clone();
        let user = seen[0].body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            user.contains(POLICY) && user.contains("Why did it ask a manager?"),
            "{user}"
        );
        listed(&fx, &path).await;
        drop(fx);
    })
}

/// A blank question is 400; a step or branch there is not, 404.
async fn refused(fx: &AppFx, path: &str) {
    let (st, v) = fx.post(path, json!({"seq": 2, "question": "  "})).await;
    assert_eq!(st, 400, "{v}");
    let (st, v) = fx.post(path, json!({"seq": 99, "question": "Why?"})).await;
    assert_eq!(st, 404, "{v}");
    let missing = json!({"seq": 1, "question": "Why?"});
    let (st, v) = fx.post("/api/branches/missing/questions", missing).await;
    assert_eq!(st, 404, "{v}");
}

/// The answer, waited for: stored with its model and tokens, the invented quote dropped.
async fn answered(fx: &AppFx, path: &str) {
    let body = json!({"seq": 2, "question": "Why did it ask a manager?", "wait": true});
    let (st, v) = fx.post(path, body).await;
    assert_eq!((st, v["status"].as_str()), (200, Some("done")), "{v}");
    assert_eq!(
        (v["kind"].as_str(), v["model"].as_str()),
        (Some("ask"), Some("local:custom:m"))
    );
    assert_eq!(v["tokens"], 1000);
    let r = &v["review"];
    assert_eq!(r["question"], "Why did it ask a manager?");
    let asked = (r["step"].as_i64(), r["agent"].as_str(), r["mine"].as_bool());
    assert_eq!(asked, (Some(2), Some("solo"), Some(true)));
    let evidence = r["evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 1, "the invented quote is dropped: {r}");
    let e = &evidence[0];
    assert_eq!(e["quote"], POLICY);
    let at = (e["step"].as_i64(), e["field"].as_str());
    assert_eq!(at, (Some(1), Some("request.messages[0] (system)")));
    assert_eq!(r["speculation"][0]["based_on"], json!(["m1.0"]));
    let issues = v["issues"].as_array().unwrap();
    let said = issues[0].as_str().unwrap();
    assert!(
        issues.len() == 1 && said.contains("word for word"),
        "{issues:?}"
    );
}

/// Listed under its step only; and an operation, so the assistant and MCP can ask too.
async fn listed(fx: &AppFx, path: &str) {
    let (st, listed) = fx.get(&format!("{path}?seq=2")).await;
    assert_eq!(st, 200);
    assert_eq!(listed["questions"].as_array().unwrap().len(), 1, "{listed}");
    let (_, other) = fx.get(&format!("{path}?seq=1")).await;
    assert_eq!(other["questions"], json!([]));
    let op = shaked::ops::find("questions.ask").expect("an operation");
    assert_eq!(op.access, shaked::ops::Access::Spend);
}
