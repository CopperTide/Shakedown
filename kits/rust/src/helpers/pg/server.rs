//! Which Postgres server a package uses when it is not told: the same choice shaked makes.

/// The server packages use when `SHAKEDOWN_PG_URL` is not set (`deploy/compose.yml`'s).
pub const DEFAULT_URL: &str = "postgres://shakedown:shakedown@127.0.0.1:5544/postgres";

/// `SHAKEDOWN_PG_URL`, else [`DEFAULT_URL`].
pub fn configured_url() -> String {
    pick_url(crate::settings::var("SHAKEDOWN_PG_URL").ok())
}

/// `set` (when not empty), else [`DEFAULT_URL`].
fn pick_url(set: Option<String>) -> String {
    set.filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_else_the_default_server() {
        assert_eq!(pick_url(Some("postgres://x/y".into())), "postgres://x/y");
        assert_eq!(pick_url(None), DEFAULT_URL);
        assert_eq!(pick_url(Some(String::new())), DEFAULT_URL);
    }
}
