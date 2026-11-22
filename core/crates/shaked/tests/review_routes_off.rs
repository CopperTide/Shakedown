//! The review routes with AI review off: every `POST` is 409 before its subject is parsed or
//! checked, and `GET` still reads (400 without a pair, 404 before any review).

#[path = "review_routes/setup.rs"]
mod setup;
mod support;

use support::run;

#[test]
fn with_review_off_posts_are_409_first_and_gets_still_read() {
    setup::review("off");
    run(async {
        let fx = setup::app("off").await;
        let (_, s) = fx.get("/api/review/settings").await;
        assert_eq!(s["enabled"], false, "{s}");
        let off = "AI review is off: SHAKEDOWN_REVIEW is off";
        for path in [
            "/api/compare/review",
            "/api/branches/nope/review",
            "/api/suites/compare/review?a=x&b=y",
        ] {
            let (st, v) = fx.post(path, serde_json::json!({})).await;
            assert_eq!((st, v["error"].as_str()), (409, Some(off)), "{path}: {v}");
        }
        let (st, v) = fx.get("/api/compare/review").await;
        assert_eq!(
            (st, v["error"].as_str()),
            (400, Some("a and b are required"))
        );
        let b = setup::branch(&fx, None).await;
        let (st, v) = fx.get(&format!("/api/branches/{b}/review")).await;
        assert_eq!(
            (st, v["error"].as_str()),
            (404, Some("there is no review yet"))
        );
        drop(fx);
    })
}
