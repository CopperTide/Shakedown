//! Shakedown's settings from the environment: `SHAKEDOWN_<NAME>`. A program shaked starts is
//! handed the settings it needs ([`pair`], [`passed`]).

use std::{env::VarError, ffi::OsString};

/// The prefix of every setting.
pub const PREFIX: &str = "SHAKEDOWN_";

/// [`std::env::var_os`] of `name`.
pub fn var_os(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// [`std::env::var`] of `name`.
pub fn var(name: &str) -> Result<String, VarError> {
    std::env::var(name)
}

/// `name` set to `value`, as a program shaked starts is given it.
pub fn pair<V>(name: &str, value: V) -> [(String, V); 1] {
    [(name.to_string(), value)]
}

/// What a program is passed of this process's `name` when its environment is otherwise cleared:
/// nothing when unset.
pub fn passed(name: &str) -> Vec<(String, OsString)> {
    var_os(name)
        .map(|v| vec![(name.to_string(), v)])
        .unwrap_or_default()
}

/// `name` in a list of `(key, value)` settings, for settings handed over as a map rather than the
/// process's environment.
pub fn lookup<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter()
        .find(|(key, _)| key == name)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_read_and_passed_under_its_name() {
        std::env::set_var("SHAKEDOWN_KIT_TEST_PASSED", "v");
        assert_eq!(var("SHAKEDOWN_KIT_TEST_PASSED").as_deref(), Ok("v"));
        let names: Vec<String> = passed("SHAKEDOWN_KIT_TEST_PASSED")
            .into_iter()
            .map(|p| p.0)
            .collect();
        assert_eq!(names, ["SHAKEDOWN_KIT_TEST_PASSED"]);
        assert!(passed("SHAKEDOWN_KIT_TEST_UNSET").is_empty());
        assert_eq!(
            pair("SHAKEDOWN_SAVES", "/s"),
            [("SHAKEDOWN_SAVES".to_string(), "/s")]
        );
        let vars = vec![("SHAKEDOWN_X".to_string(), "1".to_string())];
        assert_eq!(lookup(&vars, "SHAKEDOWN_X"), Some("1"));
    }
}
