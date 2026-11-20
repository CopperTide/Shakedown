//! Shakedown's settings from the environment: `SHAKEDOWN_<NAME>`, else the old name `ARENA_<NAME>`
//! (the prefix until 2026-11-19), which is still read and warned about once per name. A program
//! shaked starts gets both names ([`both`]), so a part or driver written for either reads its own.

use std::{
    collections::BTreeSet,
    env::VarError,
    ffi::OsString,
    sync::{Mutex, OnceLock},
};

/// The prefix of every setting.
pub const PREFIX: &str = "SHAKEDOWN_";
/// The prefix settings had until 2026-11-19, still read.
pub const OLD_PREFIX: &str = "ARENA_";

/// `name`'s old spelling: `SHAKEDOWN_X` → `ARENA_X` (a name without the prefix is its own).
pub fn old_name(name: &str) -> String {
    name.strip_prefix(PREFIX)
        .map_or_else(|| name.to_string(), |rest| format!("{OLD_PREFIX}{rest}"))
}

/// `name`'s new spelling: `ARENA_X` → `SHAKEDOWN_X` (any other name is its own).
pub fn new_name(name: &str) -> String {
    name.strip_prefix(OLD_PREFIX)
        .map_or_else(|| name.to_string(), |rest| format!("{PREFIX}{rest}"))
}

/// True the first time `old` is passed: the old names read so far.
fn first_read(old: &str) -> bool {
    static READ: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    let read = READ.get_or_init(Default::default);
    let mut read = read
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    read.insert(old.to_string())
}

/// Says once per name, on stderr, that the old name was read.
fn warn_old(name: &str, old: &str) {
    if first_read(old) {
        eprintln!("shakedown: {old} is an old name, read for now; set {name} instead");
    }
}

/// [`std::env::var_os`] of `name` (`SHAKEDOWN_X`), else of its old name.
pub fn var_os(name: &str) -> Option<OsString> {
    if let Some(v) = std::env::var_os(name) {
        return Some(v);
    }
    let old = old_name(name);
    let v = std::env::var_os(&old)?;
    warn_old(name, &old);
    Some(v)
}

/// [`std::env::var`] of `name` (`SHAKEDOWN_X`), else of its old name.
pub fn var(name: &str) -> Result<String, VarError> {
    var_os(name)
        .ok_or(VarError::NotPresent)?
        .into_string()
        .map_err(VarError::NotUnicode)
}

/// `name` and its old name, both set to `value`: what a program shaked starts is given.
pub fn both<V: Clone>(name: &str, value: V) -> [(String, V); 2] {
    [(name.to_string(), value.clone()), (old_name(name), value)]
}

/// What a program is passed of this process's `name` when its environment is otherwise cleared:
/// nothing when unset; a setting (`SHAKEDOWN_X`, read as [`var_os`] does) under both its names.
pub fn passed(name: &str) -> Vec<(String, OsString)> {
    let Some(v) = var_os(name) else {
        return vec![];
    };
    match name.starts_with(PREFIX) {
        true => both(name, v).into(),
        false => vec![(name.to_string(), v)],
    }
}

/// `name` in a list of `(key, value)` settings (new name first, then the old), as [`var`] reads the
/// environment; for settings handed over as a map rather than the process's environment.
pub fn lookup<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let old = old_name(name);
    let find = |k: &str| {
        vars.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    find(name).or_else(|| find(&old))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_names_swap_the_prefix() {
        assert_eq!(old_name("SHAKEDOWN_SAVES"), "ARENA_SAVES");
        assert_eq!(old_name("PATH"), "PATH");
    }

    #[test]
    fn the_new_name_wins_and_the_old_one_is_read_without_it() {
        std::env::set_var("ARENA_KIT_TEST_A", "old");
        assert_eq!(var("SHAKEDOWN_KIT_TEST_A").as_deref(), Ok("old"));
        std::env::set_var("SHAKEDOWN_KIT_TEST_A", "new");
        assert_eq!(var("SHAKEDOWN_KIT_TEST_A").as_deref(), Ok("new"));
        assert_eq!(var("SHAKEDOWN_KIT_TEST_NONE"), Err(VarError::NotPresent));
    }

    #[test]
    fn an_old_name_is_warned_about_once() {
        assert!(first_read("ARENA_KIT_TEST_ONCE"));
        assert!(!first_read("ARENA_KIT_TEST_ONCE"));
    }

    #[test]
    fn programs_get_both_names() {
        let [(a, x), (b, y)] = both("SHAKEDOWN_SAVES", "/s");
        assert_eq!(
            (a.as_str(), b.as_str(), x, y),
            ("SHAKEDOWN_SAVES", "ARENA_SAVES", "/s", "/s")
        );
        assert_eq!(new_name("ARENA_SAVES"), "SHAKEDOWN_SAVES");
        std::env::set_var("ARENA_KIT_TEST_PASSED", "v");
        let names: Vec<String> = passed("SHAKEDOWN_KIT_TEST_PASSED")
            .into_iter()
            .map(|p| p.0)
            .collect();
        assert_eq!(
            names,
            ["SHAKEDOWN_KIT_TEST_PASSED", "ARENA_KIT_TEST_PASSED"]
        );
        assert!(passed("SHAKEDOWN_KIT_TEST_UNSET").is_empty());
        let vars = vec![("ARENA_X".to_string(), "1".to_string())];
        assert_eq!(lookup(&vars, "SHAKEDOWN_X"), Some("1"));
    }
}
