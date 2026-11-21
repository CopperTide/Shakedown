//! Names Docker accepts for containers, volumes and network aliases.

/// A container or volume name: `[A-Za-z0-9][A-Za-z0-9_.-]*`, at most 63 characters.
pub fn valid_docker_name(s: &str) -> bool {
    s.len() <= 63
        && s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// True when Docker has a container called `name`, running or not (false without Docker).
pub fn container_exists(name: &str) -> bool {
    std::process::Command::new("docker")
        .args(["container", "inspect", "--format", "{{.Id}}", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// True when a long-lived resource renamed on 2026-11-19 keeps its `old` name: only it exists, so
/// an install made before keeps its server and the saves in it. Else the `new` name is used.
pub fn keep_old(new: &str, old: &str, exists: impl Fn(&str) -> bool) -> bool {
    !exists(new) && exists(old)
}

#[cfg(test)]
mod old_names {
    use super::keep_old;

    #[test]
    fn the_old_resource_is_kept_only_while_there_is_no_new_one() {
        let exists = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
        assert!(!keep_old("new", "old", exists(&[])));
        assert!(keep_old("new", "old", exists(&["old"])));
        assert!(!keep_old("new", "old", exists(&["new", "old"])));
        assert!(!keep_old("new", "old", exists(&["new"])));
    }
}
