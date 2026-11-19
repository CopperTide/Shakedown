//! Docker Compose projects as instances: run them, and move their volumes to and from folders.
//!
//! Volumes are copied as tar streams through a throwaway helper container, so no root access to
//! Docker's own folders is needed and owners, modes and mtimes survive in the archive.
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};

use super::docker_name::valid_docker_name;

/// The image whose `tar` moves volume contents (any image with `tar` will do).
pub const DEFAULT_HELPER: &str = "redis:6.2-alpine";

fn name_ok(s: &str, extra: &[u8]) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || extra.contains(&b)
        })
}

/// A volume name from a compose file: letters, digits, `_` and `-`.
pub fn valid_volume(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// An image name or reference: not empty, no leading `-`, only `[A-Za-z0-9._/:@-]`.
pub fn valid_image(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/:@-".contains(&b))
}

#[derive(Debug, Clone)]
pub struct Project {
    name: String,
    compose_file: PathBuf,
    env: BTreeMap<String, String>,
    helper: String,
}

/// Runs `cmd`, failing with its stderr when it exits unsuccessfully.
fn run(cmd: Command, what: &str) -> Result<()> {
    run_out(cmd, what).map(drop)
}

/// Runs `cmd`; its stdout, or an error with its stderr when it exits unsuccessfully.
fn run_out(mut cmd: Command, what: &str) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("{what}: cannot run"))?;
    check(&out, what)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn check(out: &Output, what: &str) -> Result<()> {
    if out.status.success() {
        return Ok(());
    }
    bail!(
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    )
}

impl Project {
    /// The project `name` (lowercase letters, digits and `_`) of `compose_file`, run with `env`
    /// added to the environment. `COMPOSE_PROJECT_NAME` is never passed on.
    pub fn new(name: &str, compose_file: &Path, env: &BTreeMap<String, String>) -> Result<Project> {
        if !name_ok(name, b"") {
            bail!("{name:?} is not a project name");
        }
        Ok(Project {
            name: name.into(),
            compose_file: compose_file.into(),
            env: env.clone(),
            helper: DEFAULT_HELPER.into(),
        })
    }

    /// A project `name` that only runs helper containers (it has no compose file, so its compose
    /// commands fail), with `env` added to `docker`'s environment.
    pub fn helpers_only(name: &str, env: &BTreeMap<String, String>) -> Result<Project> {
        Project::new(name, Path::new(""), env)
    }

    /// Moves volumes through `image` instead of the default helper.
    pub fn with_helper(mut self, image: &str) -> Result<Project> {
        if !valid_image(image) {
            bail!("{image:?} is not an image name");
        }
        self.helper = image.into();
        Ok(self)
    }

    /// A `docker` command with the project's environment (and never `COMPOSE_PROJECT_NAME`).
    fn docker(&self) -> Command {
        let mut cmd = Command::new("docker");
        cmd.env_remove("COMPOSE_PROJECT_NAME").envs(&self.env);
        cmd
    }

    fn compose(&self, args: &[&str]) -> Command {
        let mut cmd = self.docker();
        cmd.args(["compose", "-p", &self.name, "-f"])
            .arg(&self.compose_file)
            .args(args);
        cmd
    }

    /// Builds and starts the project.
    pub fn up(&self) -> Result<()> {
        run(self.compose(&["up", "-d", "--wait"]), "docker compose up")
    }

    /// Creates the project's volumes and containers without starting them.
    pub fn create(&self) -> Result<()> {
        run(self.compose(&["up", "--no-start"]), "docker compose create")
    }

    pub fn stop(&self) -> Result<()> {
        run(self.compose(&["stop"]), "docker compose stop")
    }

    pub fn start(&self) -> Result<()> {
        run(self.compose(&["start"]), "docker compose start")
    }

    /// Stops only the compose services `s`.
    pub fn stop_services(&self, s: &[&str]) -> Result<()> {
        run(self.compose_on(&["stop"], s)?, "docker compose stop")
    }

    /// Starts only the compose services `s` (stopped ones).
    pub fn start_services(&self, s: &[&str]) -> Result<()> {
        run(self.compose_on(&["start"], s)?, "docker compose start")
    }

    /// Runs the compose services `s` (every service when empty), detached, without waiting.
    pub fn up_services(&self, s: &[&str]) -> Result<()> {
        run(self.compose_on(&["up", "-d"], s)?, "docker compose up")
    }

    /// A compose command `args` followed by the service names `s`, each checked.
    fn compose_on(&self, args: &[&str], s: &[&str]) -> Result<Command> {
        if let Some(bad) = s.iter().find(|n| !name_ok(n, b"-")) {
            bail!("{bad:?} is not a service name");
        }
        let mut cmd = self.compose(args);
        cmd.args(s);
        Ok(cmd)
    }

    /// Where `port` of `service` is published on this machine (`127.0.0.1:49153`): the port
    /// Docker picked, on 127.0.0.1 whatever address it was published on.
    pub fn port(&self, service: &str, port: u16) -> Result<String> {
        let cmd = self.compose_on(&["port"], &[service])?;
        let mut cmd = cmd;
        cmd.arg(port.to_string());
        let out = run_out(cmd, "docker compose port")?;
        let first = out.lines().next().unwrap_or("").trim();
        let published = first.rsplit_once(':').map(|(_, p)| p).unwrap_or("");
        match published.parse::<u16>() {
            Ok(p) => Ok(format!("127.0.0.1:{p}")),
            Err(_) => bail!("{service} publishes no port {port} (docker said {first:?})"),
        }
    }

    /// Runs `args` in the running container of `service` with the file `stdin` as its input.
    pub fn exec_in(&self, service: &str, args: &[&str], stdin: &Path) -> Result<()> {
        let mut cmd = self.compose_on(&["exec", "-T"], &[service])?;
        cmd.args(args).stdin(File::open(stdin)?);
        run(cmd, &format!("{service}: {}", args.first().unwrap_or(&"")))
    }

    /// A throwaway container of the helper image (never pulled) running `args`, with no network
    /// and each `(source, target, read_only)` mounted (a source is a host path or one of the
    /// project's volumes); its stdout.
    pub fn run_helper(&self, mounts: &[(String, String, bool)], args: &[&str]) -> Result<String> {
        let mut cmd = self.helper_run("", mounts, false);
        cmd.args(args);
        run_out(cmd, &format!("helper {}", args.first().unwrap_or(&"")))
    }

    /// `docker run` of a throwaway helper container (never pulled, no network; stdin open when
    /// `interactive`) with `entrypoint` and `mounts`, up to its image.
    fn helper_run(
        &self,
        entrypoint: &str,
        mounts: &[(String, String, bool)],
        interactive: bool,
    ) -> Command {
        let mut cmd = self.docker();
        cmd.args(["run", "--rm", "--pull", "never", "--network", "none"]);
        if interactive {
            cmd.arg("-i");
        }
        cmd.args(["--entrypoint", entrypoint]);
        for (source, target, read_only) in mounts {
            let mode = if *read_only { ":ro" } else { "" };
            cmd.args(["-v", &format!("{source}:{target}{mode}")]);
        }
        cmd.arg(&self.helper);
        cmd
    }

    /// Docker's name for the project's volume `volume`, to mount it in a helper.
    pub fn volume(&self, volume: &str) -> Result<String> {
        self.volume_name(volume)
    }

    /// Removes the project's containers, networks and volumes.
    pub fn down_with_volumes(&self) -> Result<()> {
        run(self.compose(&["down", "-v"]), "docker compose down")
    }

    /// Runs `args` in the running container of `service` (no TTY, no stdin); its stdout.
    pub fn exec(&self, service: &str, args: &[&str]) -> Result<String> {
        if !name_ok(service, b"-") {
            bail!("{service:?} is not a service name");
        }
        let mut cmd = self.compose(&["exec", "-T", service]);
        cmd.args(args).stdin(std::process::Stdio::null());
        run_out(cmd, &format!("{service}: {}", args.first().unwrap_or(&"")))
    }

    /// The project's default network.
    pub fn network(&self) -> String {
        format!("{}_default", self.name)
    }

    /// Connects `container` (one made outside the project) to the project's default network,
    /// reachable there as `alias`. Nothing to do when it is connected already.
    pub fn connect(&self, container: &str, alias: &str) -> Result<()> {
        if !valid_docker_name(container) || !valid_docker_name(alias) {
            bail!("{container:?} or {alias:?} is not a container name or alias");
        }
        let mut cmd = self.docker();
        let fmt = "{{range .Containers}}{{.Name}} {{end}}";
        cmd.args(["network", "inspect", "-f", fmt, &self.network()]);
        let joined = run_out(cmd, "docker network inspect")?;
        if joined.split_whitespace().any(|n| n == container) {
            return Ok(());
        }
        let mut cmd = self.docker();
        cmd.args([
            "network",
            "connect",
            "--alias",
            alias,
            &self.network(),
            container,
        ]);
        run(cmd, "docker network connect")
    }

    /// Docker's name for the project's volume `volume`.
    fn volume_name(&self, volume: &str) -> Result<String> {
        if !valid_volume(volume) {
            bail!("{volume:?} is not a volume name");
        }
        Ok(format!("{}_{volume}", self.name))
    }

    /// The helper's `tar -C /v <flags>` with `mount` at `/v`.
    fn helper_tar(&self, mount: (String, bool), flags: &[&str], interactive: bool) -> Command {
        let mounts = [(mount.0, "/v".to_string(), mount.1)];
        let mut cmd = self.helper_run("tar", &mounts, interactive);
        cmd.args(["-C", "/v"]).args(flags);
        cmd
    }

    /// `tar` of what is mounted at `/v` (read-only when `read_only`) to the archive `dest`, as
    /// streamed by the helper: owners (numeric), modes and mtimes are kept.
    fn export(&self, source: String, read_only: bool, dest: &Path) -> Result<()> {
        let flags = ["-cpf", "-", "--numeric-owner", "."];
        let mut cmd = self.helper_tar((source, read_only), &flags, false);
        cmd.stdout(File::create(dest)?);
        run(cmd, "export volume")
    }

    /// Writes the volume to the tar archive `dest` (see `export`).
    pub fn export_volume(&self, volume: &str, dest: &Path) -> Result<()> {
        self.export(self.volume_name(volume)?, false, dest)
    }

    /// Writes the folder `dir` (a host path, mounted read-only) to the tar archive `dest` with
    /// the same helper and `tar` as `export_volume`, so a copy of a volume archives as it does.
    pub fn export_dir(&self, dir: &Path, dest: &Path) -> Result<()> {
        self.export(dir.to_string_lossy().into_owned(), true, dest)
    }

    /// Fills the volume from the tar archive `src` (made by `export_volume`), as root, so files
    /// keep their original owners.
    pub fn import_volume(&self, volume: &str, src: &Path) -> Result<()> {
        let mount = (self.volume_name(volume)?, false);
        let mut cmd = self.helper_tar(mount, &["-xpf", "-", "--numeric-owner"], true);
        cmd.stdin(File::open(src)?);
        run(cmd, "import volume")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked() {
        assert!(valid_volume("data") && valid_volume("pg-data_1"));
        assert!(valid_image("redis:6.2-alpine") && valid_image("ghcr.io/a/b@sha256"));
        assert!(!valid_image("") && !valid_image("-v") && !valid_image("a b"));
        assert!(!valid_volume("../x") && !valid_volume("a b") && !valid_volume(""));
        let p = |n| Project::new(n, Path::new("c.yml"), &BTreeMap::new());
        assert!(p("arena_t1").is_ok());
        assert!(p("Arena").is_err() && p("a;b").is_err() && p("").is_err());
        assert_eq!(p("arena_t1").unwrap().network(), "arena_t1_default");
        assert!(valid_docker_name("arena-erpnext-db") && valid_docker_name("db"));
        assert!(!valid_docker_name("-x") && !valid_docker_name("a b") && !valid_docker_name(""));
        let bad = p("arena_t1").unwrap();
        assert!(bad.connect("a b", "db").is_err() && bad.exec("Web;x", &["ls"]).is_err());
    }
}
