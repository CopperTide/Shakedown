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
