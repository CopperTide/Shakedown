//! Which Postgres server a package uses when it is not told: the same choice shaked makes.
use std::sync::OnceLock;
use std::time::Duration;

use tokio_postgres::NoTls;

/// The server packages use when `SHAKEDOWN_PG_URL` is not set (`deploy/compose.yml`'s).
pub const DEFAULT_URL: &str = "postgres://shakedown:shakedown@127.0.0.1:5544/postgres";
/// The default login until 2026-11-19; a server made before then may have only this one.
pub const OLD_DEFAULT_URL: &str = "postgres://arena:arena@127.0.0.1:5544/postgres";

/// How long one login is tried for.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// `SHAKEDOWN_PG_URL` (or its old name), else [`DEFAULT_URL`], else [`OLD_DEFAULT_URL`] when the
/// new login is refused and the old one connects. Decided once per process; safe to call inside a
/// runtime (the logins are tried on a thread of their own).
pub fn configured_url() -> String {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| pick_url(crate::settings::var("SHAKEDOWN_PG_URL").ok(), connects))
        .clone()
}

/// `set`, else the default login that `connects`, the new one first (the new one when neither does).
fn pick_url(set: Option<String>, connects: impl Fn(&str) -> bool) -> String {
    if let Some(url) = set {
        return url;
    }
    if !connects(DEFAULT_URL) && connects(OLD_DEFAULT_URL) {
        eprintln!("Postgres user arena (the default until 2026-11-19); see README.md, Old names");
        return OLD_DEFAULT_URL.to_string();
    }
    DEFAULT_URL.to_string()
}

/// Whether `url` accepts a connection within [`CONNECT_TIMEOUT`].
fn connects(url: &str) -> bool {
    let url = url.to_string();
    let attempt = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(async {
            let connected =
                tokio::time::timeout(CONNECT_TIMEOUT, tokio_postgres::connect(&url, NoTls));
            connected.await.ok()?.ok().map(drop)
        })
    });
    attempt.join().ok().flatten().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn a_set_url_is_used_without_trying_a_login() {
        let tried = RefCell::new(vec![]);
        let url = pick_url(Some("postgres://x/y".into()), |u| {
            tried.borrow_mut().push(u.to_string());
            true
        });
        assert_eq!(url, "postgres://x/y");
        assert!(tried.borrow().is_empty());
    }

    #[test]
    fn the_old_login_is_used_when_the_new_one_is_refused() {
        assert_eq!(pick_url(None, |u| u == OLD_DEFAULT_URL), OLD_DEFAULT_URL);
    }

    #[test]
    fn the_new_login_is_used_when_it_connects_or_neither_does() {
        assert_eq!(pick_url(None, |_| true), DEFAULT_URL);
        assert_eq!(pick_url(None, |_| false), DEFAULT_URL);
    }

    #[test]
    fn a_refused_login_does_not_connect() {
        assert!(!connects(
            "postgres://no_such_login:wrong@127.0.0.1:1/postgres"
        ));
    }
}
