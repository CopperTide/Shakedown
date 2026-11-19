//! How a proxied request and its answer are recorded: bodies as text when short, else their size
//! and hash; headers without secrets.

use serde_json::{json, Map, Value};

/// Bodies up to this size that are UTF-8 are recorded as text.
pub const BODY_KEPT: usize = 64 * 1024;
/// Request bodies above this size are refused (413).
pub const MAX_BODY: usize = 256 << 20;
/// What a secret header is recorded as.
pub const REDACTED: &str = "(redacted)";

/// Headers whose values are secrets.
const SECRET_HEADERS: [&str; 5] = [
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "private-token",
];

/// `{bytes, sha256, text?}`: the body's size, its SHA-256 (hex) and, when it is UTF-8 and at
/// most `BODY_KEPT` long, its text.
pub fn body_record(b: &[u8]) -> Value {
    let mut v = json!({"bytes": b.len(), "sha256": sha256_hex(b)});
    let text = (b.len() <= BODY_KEPT).then(|| std::str::from_utf8(b).ok());
    if let Some(Some(text)) = text {
        v["text"] = json!(text);
    }
    v
}

/// The headers as recorded: names in lower case, secrets `REDACTED`, `proxy-*` left out; a name
/// given twice has its values joined with `, `.
pub fn kept_headers(h: &[(String, String)]) -> Value {
    let mut out = Map::new();
    for (name, value) in h {
        let name = name.to_ascii_lowercase();
        if name.starts_with("proxy-") {
            continue;
        }
        let value = match SECRET_HEADERS.contains(&name.as_str()) {
            true => REDACTED.to_string(),
            false => value.clone(),
        };
        match out.get_mut(&name) {
            Some(Value::String(was)) if value != REDACTED => *was = format!("{was}, {value}"),
            Some(_) => {}
            None => drop(out.insert(name, json!(value))),
        }
    }
    Value::Object(out)
}

fn sha256_hex(b: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, b);
    d.as_ref().iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_text_body_is_kept_as_text() {
        let v = body_record(b"hi");
        let sha = "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4";
        assert_eq!(v, json!({"bytes": 2, "sha256": sha, "text": "hi"}));
        assert_eq!(body_record(&vec![b'a'; BODY_KEPT])["bytes"], BODY_KEPT);
        assert!(body_record(&vec![b'a'; BODY_KEPT]).get("text").is_some());
    }

    #[test]
    fn a_long_or_binary_body_is_kept_as_size_and_hash() {
        let long = body_record(&vec![b'a'; BODY_KEPT + 1]);
        assert_eq!(long["bytes"], BODY_KEPT + 1);
        assert!(long.get("text").is_none() && long["sha256"].is_string());
        assert!(body_record(&[0xff, 0xfe]).get("text").is_none());
    }

    #[test]
    fn secrets_are_redacted_and_proxy_headers_left_out() {
        let h = [
            ("Authorization", "token abc"),
            ("Proxy-Authorization", "Basic x"),
            ("Accept", "a"),
            ("accept", "b"),
            ("Set-Cookie", "s=1"),
        ];
        let h: Vec<(String, String)> = h.map(|(k, v)| (k.into(), v.into())).to_vec();
        let want = json!({"authorization": REDACTED, "accept": "a, b", "set-cookie": REDACTED});
        assert_eq!(kept_headers(&h), want);
    }
}
