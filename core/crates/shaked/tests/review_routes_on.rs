//! The review routes with AI review on: a `POST` answers its parse (400), then its subject's
//! checks (404, 409) before any review starts. No case reaches the model.

#[path = "review_routes/setup.rs"]
mod setup;
mod support;

use serde_json::{json, Value};
use shaked::model::BranchStatus;
use support::{run, AppFx};

async fn post(fx: &AppFx, path: &str) -> (u16, String) {
    let (st, v): (u16, Value) = fx.post(path, json!({})).await;
    (st, v["error"].as_str().unwrap_or_default().to_string())
}

#[test]
fn with_review_on_posts_check_their_subject_before_starting() {
    setup::review("on");
    run(async {
        let fx = setup::app("on").await;
        let (_, s) = fx.get("/api/review/settings").await;
        assert_eq!(s["enabled"], true, "{s}");
        let (st, e) = post(&fx, "/api/compare/review?a=x").await;
        assert_eq!((st, e.as_str()), (400, "a and b are required"));
        let (st, e) = post(&fx, "/api/branches/missing/review").await;
        assert_eq!((st, e.as_str()), (404, "there is no branch \"missing\""));
        let running = setup::branch(&fx, None).await;
        let finished = setup::branch(&fx, Some(BranchStatus::Passed)).await;
        let (st, e) = post(&fx, &format!("/api/branches/{running}/review")).await;
        assert_eq!(st, 409, "{e}");
        assert!(e.contains("still running"), "{e}");
        // The pair is checked in order: a running `a` is 409 before a missing `b`.
        let (st, e) = post(&fx, &format!("/api/compare/review?a={running}&b=missing")).await;
        assert_eq!(st, 409, "{e}");
        let (st, e) = post(&fx, &format!("/api/compare/review?a={finished}&b=missing")).await;
        assert_eq!((st, e.as_str()), (404, "there is no branch \"missing\""));
        let (st, e) = post(&fx, "/api/suites/compare/review?a=missing-a&b=missing-b").await;
        assert_eq!((st, e.as_str()), (404, "there is no suite \"missing-a\""));
        drop(fx);
    })
}
