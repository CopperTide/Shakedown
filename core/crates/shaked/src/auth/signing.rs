//! `signing` v2: the Cloud gateway signs each browser request it forwards for a user in an org,
//! over `METHOD\npath_and_query\nuser\norg\nts` with HMAC-SHA256. shaked checks it itself so a local
//! build needs no other repository; [`V2_VECTOR`] is the fixed case the CopperTide `signing` crate
//! must pass too. The header names live here only (a later rename may change them).

use ring::hmac;

pub const USER_HEADER: &str = "x-sculptured-user";
pub const ORG_HEADER: &str = "x-sculptured-org";
pub const TS_HEADER: &str = "x-sculptured-ts";
pub const SIG_HEADER: &str = "x-sculptured-sig";
/// Every header the gateway sets starts with this; a local shaked drops inbound ones.
pub const HEADER_PREFIX: &str = "x-sculptured-";
/// How far a signature's time may be from shaked's clock, in seconds.
pub const MAX_SKEW_SECS: i64 = 60;

/// (secret, method, path, user, org, ts, expected hex signature).
pub const V2_VECTOR: (&str, &str, &str, &str, &str, i64, &str) = (
    "shakedown-test-signing-secret-v2",
    "GET",
    "/api/runs?limit=5",
    "0a000000-0000-0000-0000-0000000000a1",
    "0a000000-0000-0000-0000-000000000001",
    1_763_600_000,
    "5c571b6c8ecf566aae6f259f60a20d8bc756d9010a38159a792a601ac02469a1",
);

fn message(method: &str, path_and_query: &str, user: &str, org: &str, ts: i64) -> String {
    format!("{method}\n{path_and_query}\n{user}\n{org}\n{ts}")
}

/// The hex HMAC-SHA256 of the request's signed string.
pub fn sign_v2(
    secret: &[u8],
    method: &str,
    path_and_query: &str,
    user: &str,
    org: &str,
    ts: i64,
) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(
        &key,
        message(method, path_and_query, user, org, ts).as_bytes(),
    );
    tag.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// True when `sig` signs the request and `ts` is within [`MAX_SKEW_SECS`] of `now` (constant time).
#[allow(clippy::too_many_arguments)]
pub fn verify_v2(
    secret: &[u8],
    method: &str,
    path_and_query: &str,
    user: &str,
    org: &str,
    ts: i64,
    sig: &str,
    now: i64,
) -> bool {
    if (now - ts).abs() > MAX_SKEW_SECS {
        return false;
    }
    let Some(tag) = unhex(sig) else {
        return false;
    };
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(
        &key,
        message(method, path_and_query, user, org, ts).as_bytes(),
        &tag,
    )
    .is_ok()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The gateway's signing secret; never printed.
#[derive(Clone)]
pub struct GatewaySecret(pub Vec<u8>);

impl std::fmt::Debug for GatewaySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GatewaySecret(••••)")
    }
}
