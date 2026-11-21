//! The public base URL of a request that came through shaked's proxy, for the absolute URLs a part
//! mints (docs/protocol.md, "Serving HTTP").

/// `<proto>://<host>` from the request's `X-Forwarded-Proto` and `X-Forwarded-Host`; `fallback`
/// (the part's own address) when either is missing.
pub fn public_base(proto: Option<&str>, host: Option<&str>, fallback: &str) -> String {
    match (proto, host) {
        (Some(proto), Some(host)) if !proto.is_empty() && !host.is_empty() => {
            format!("{proto}://{host}")
        }
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_base_is_the_forwarded_scheme_and_host() {
        let fallback = "http://127.0.0.1:9";
        let base = public_base(Some("https"), Some("github.acme.test"), fallback);
        assert_eq!(base, "https://github.acme.test");
        assert_eq!(public_base(None, Some("h.test"), fallback), fallback);
        assert_eq!(public_base(Some("https"), None, fallback), fallback);
    }
}
