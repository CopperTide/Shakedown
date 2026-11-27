//! What every provider adapter shares on the wire: where a path under a provider's base is,
//! and sending a request to get back its status, JSON body and `retry-after` (for a rate limit
//! with none, the wait until the limit resets, as its `x-ratelimit-reset` gives it).

use serde_json::{json, Value};

/// The URL of `path` under `base` (one `/` between them).
pub(super) fn url(base: &str, path: &str) -> String {
    format!("{}/{path}", base.trim_end_matches('/'))
}

/// A provider's answer as it came.
pub(super) struct Answer {
    pub status: u16,
    /// Its JSON body, `{"raw": text}` when it isn't JSON.
    pub body: Value,
    /// The seconds its `retry-after` header asks to wait, when it gives a number; for a 429
    /// without one, the seconds until its rate limit resets ([`until_reset`]).
    pub retry_after: Option<u64>,
}

/// Sends `req`; `Err` when no answer came. Messages never carry the URL, so never a credential
/// in it.
pub(super) async fn exchange(req: reqwest::RequestBuilder) -> Result<Answer, String> {
    let r = req
        .send()
        .await
        .map_err(|e| format!("upstream request failed: {}", e.without_url()))?;
    let status = r.status().as_u16();
    let asked = retry_after(r.headers());
    let reset = header_text(r.headers(), RATE_LIMIT_RESET);
    let text = r
        .text()
        .await
        .map_err(|e| format!("upstream response unreadable: {}", e.without_url()))?;
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
    let limited = (status == 429).then(|| until_reset(reset.as_deref(), &body, now_ms()));
    let retry_after = asked.or(limited.flatten());
    Ok(Answer {
        status,
        body,
        retry_after,
    })
}

/// The header a rate-limited answer says when its limit resets in (OpenRouter's, also quoted in
/// its error body under `error.metadata.headers`).
const RATE_LIMIT_RESET: &str = "x-ratelimit-reset";

/// The whole seconds of a `retry-after` header (its HTTP-date form is not read).
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    header_text(headers, reqwest::header::RETRY_AFTER.as_str())?
        .trim()
        .parse()
        .ok()
}

fn header_text(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    Some(headers.get(name)?.to_str().ok()?.to_string())
}

/// The whole seconds (rounded up) until a rate limit resets, at `now_ms`: from the answer's
/// `x-ratelimit-reset` header, else the one its error body quotes. The reset is a Unix time in
/// milliseconds or seconds; a small number is seconds to wait.
fn until_reset(header: Option<&str>, body: &Value, now_ms: u64) -> Option<u64> {
    let quoted = body["error"]["metadata"]["headers"]
        .as_object()
        .and_then(|h| {
            let mut each = h.iter();
            let reset = each.find(|(k, _)| k.eq_ignore_ascii_case(RATE_LIMIT_RESET));
            reset.and_then(|(_, v)| v.as_str())
        });
    let n: u64 = header.or(quoted)?.trim().parse().ok()?;
    let at_ms = match n {
        n if n >= 1_000_000_000_000 => n,
        n if n >= 1_000_000_000 => n * 1000,
        n => return Some(n),
    };
    Some(at_ms.saturating_sub(now_ms).div_ceil(1000))
}

fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn paths_join_their_base_once() {
        assert_eq!(url("http://a/v1/", "models"), "http://a/v1/models");
        assert_eq!(url("http://a/v1", "messages"), "http://a/v1/messages");
    }

    #[test]
    fn retry_after_is_read_in_seconds() {
        let mut h = HeaderMap::new();
        assert_eq!(retry_after(&h), None);
        h.insert(RETRY_AFTER, HeaderValue::from_static(" 7"));
        assert_eq!(retry_after(&h), Some(7));
        h.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(retry_after(&h), None);
    }

    /// OpenRouter's 429 quotes `X-RateLimit-Reset` (Unix ms) in its error body; a header wins.
    #[test]
    fn a_rate_limit_waits_until_its_reset() {
        let now = 1_795_759_787_831;
        let body = json!({"error": {"code": 429, "metadata": {"headers": {
            "X-RateLimit-Limit": "20", "X-RateLimit-Reset": "1795759800000"}}}});
        assert_eq!(
            until_reset(None, &body, now),
            Some(13),
            "12.2 s, rounded up"
        );
        assert_eq!(
            until_reset(Some("1795759790"), &body, now),
            Some(3),
            "Unix seconds"
        );
        assert_eq!(
            until_reset(Some("5"), &body, now),
            Some(5),
            "seconds to wait"
        );
        assert_eq!(
            until_reset(None, &body, now + 60_000),
            Some(0),
            "already reset"
        );
        assert_eq!(until_reset(None, &json!({"error": "slow down"}), now), None);
        assert_eq!(
            until_reset(Some("4"), &json!({}), now),
            Some(4),
            "a header alone"
        );
    }
}
