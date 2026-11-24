//! Where a sandbox's `shaked worker` runs its CLI agents (Claude Code, Codex): each process in a
//! throwaway container of the agent image, as `agent` (uid 1000), on the Docker network that
//! reaches only the worker's agent port (its MCP endpoint and gateway).
//!
//! The container gets the agent's work folder, mounted at the same path, and only the variables
//! the agent's command sets: its routes and its agent token. They are passed by name (`-e NAME`,
//! the value in the `docker` process's environment), so no token shows in argv.

use std::{
    net::SocketAddr,
    path::Path,
    process::{Command, Stdio},
};

/// The uid and gid CLI agents run as in their container: `agent` in the agent image.
pub const AGENT_UID: u32 = 1000;

/// What one agent container may hold: memory, and processes and threads.
const AGENT_MEMORY: &str = "1g";
const AGENT_PIDS: &str = "256";

/// The variables a sandbox template sets to jail its worker's CLI agents: all three, or none.
const JAIL_ENV: [&str; 3] = [
    "SHAKED_AGENT_NETWORK",
    "SHAKED_AGENT_IMAGE",
    "SHAKED_AGENT_LISTEN",
];

/// How a worker runs its CLI agents in containers (`deploy/sandbox`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jail {
    /// The Docker network the containers join (`SHAKED_AGENT_NETWORK`); it reaches only `listen`.
    pub network: String,
    /// The image with the CLIs and user `agent` (`SHAKED_AGENT_IMAGE`).
    pub image: String,
    /// Where the worker serves its agents' MCP endpoint and gateway on that network
    /// (`SHAKED_AGENT_LISTEN`, the network's gateway address and a port).
    pub listen: SocketAddr,
}

impl Jail {
    /// The jail the environment names; None when it names none, an error when it names part of
    /// one or an address that is not one.
    pub fn from_env() -> Result<Option<Jail>, String> {
        Jail::from_vars(|k| parts::settings::var(k).ok().filter(|v| !v.is_empty()))
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Option<Jail>, String> {
        let [network, image, listen] = JAIL_ENV.map(&var);
        let (Some(network), Some(image), Some(listen)) = (network, image, listen) else {
            if JAIL_ENV.iter().any(|k| var(k).is_some()) {
                return Err(format!("set all of {} or none", JAIL_ENV.join(", ")));
            }
            return Ok(None);
        };
        let listen = listen
            .parse()
            .map_err(|_| format!("SHAKED_AGENT_LISTEN {listen:?} is not an address and port"))?;
        Ok(Some(Jail {
            network,
            image,
            listen,
        }))
    }

    /// The base URL the agents reach the worker at.
    pub fn base(&self) -> String {
        format!("http://{}", self.listen)
    }

    /// `inner` run as `agent` in a new container, in `workdir`, which is given to the agent. The
    /// container goes when the returned [`Container`] is dropped.
    pub fn contain(&self, inner: &Command, workdir: &Path) -> Result<(Command, Container), String> {
        give_to_agent(workdir)
            .map_err(|e| format!("cannot give {} to the agent: {e}", workdir.display()))?;
        let container = Container::new();
        Ok((self.docker_run(inner, workdir, &container.name), container))
    }

    /// `docker run` of `inner` in a container named `name`: as `agent`, with no capabilities and
    /// capped memory and processes, on the agent network, on a read-only root with a fresh `/tmp`
    /// and home, in `workdir` (mounted at its own path). The variables `inner` sets are passed in
    /// by name; those it removes are removed from `docker`'s environment too.
    pub(crate) fn docker_run(&self, inner: &Command, workdir: &Path, name: &str) -> Command {
        let mut c = Command::new("docker");
        let user = format!("{AGENT_UID}:{AGENT_UID}");
        let home = format!("/home/agent:uid={AGENT_UID},gid={AGENT_UID},mode=0700");
        let dir = workdir.display();
        c.args(["run", "--rm", "-i", "--name", name, "--user", &user])
            .args(["--network", &self.network, "--cap-drop", "ALL"])
            .args(["--security-opt", "no-new-privileges"])
            .args(["--memory", AGENT_MEMORY, "--pids-limit", AGENT_PIDS])
            .args(["--read-only", "--tmpfs", "/tmp", "--tmpfs", &home])
            .arg("--mount")
            .arg(format!("type=bind,src={dir},dst={dir}"))
            .arg("-w")
            .arg(workdir);
        for (k, v) in inner.get_envs() {
            match v {
                Some(v) => c.arg("-e").arg(k).env(k, v),
                None => c.env_remove(k),
            };
        }
        c.arg(&self.image)
            .arg(inner.get_program())
            .args(inner.get_args());
        c
    }
}

/// `cmd` as it runs in `workdir`: contained when there is a `jail` (the container lives as long
/// as the [`Container`] returned), else as it is.
pub(crate) fn confine(
    jail: Option<&Jail>,
    cmd: Command,
    workdir: &Path,
) -> Result<(Command, Option<Container>), String> {
    let Some(jail) = jail else {
        return Ok((cmd, None));
    };
    let (cmd, container) = jail.contain(&cmd, workdir)?;
    Ok((cmd, Some(container)))
}

/// `dir` and what is in it (the agent's MCP config) belong to the agent's uid.
fn give_to_agent(dir: &Path) -> std::io::Result<()> {
    let own = |p: &Path| std::os::unix::fs::chown(p, Some(AGENT_UID), Some(AGENT_UID));
    own(dir)?;
    for entry in std::fs::read_dir(dir)? {
        own(&entry?.path())?;
    }
    Ok(())
}

/// One agent process's container, removed when this is dropped: a `docker run` that is killed
/// (a CLI past its time limit, a session closed) leaves its container running.
pub struct Container {
    name: String,
}

impl Container {
    fn new() -> Container {
        let id = uuid::Uuid::new_v4().simple();
        Container {
            name: format!("shaked-agent-{id}"),
        }
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        let name = std::mem::take(&mut self.name);
        // On its own thread: a drop doesn't wait on docker.
        std::thread::spawn(move || {
            let _ = Command::new("docker")
                .args(["rm", "-f", &name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Jail;

    fn vars<'a>(set: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |k| {
            set.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn a_jail_is_named_whole_or_not_at_all() {
        assert_eq!(Jail::from_vars(vars(&[])), Ok(None));
        let whole = [
            ("SHAKED_AGENT_NETWORK", "shaked-agents"),
            ("SHAKED_AGENT_IMAGE", "shakedown-agent:1"),
            ("SHAKED_AGENT_LISTEN", "10.250.0.1:4610"),
        ];
        let jail = Jail::from_vars(vars(&whole)).unwrap().unwrap();
        assert_eq!(jail.base(), "http://10.250.0.1:4610");
        assert!(Jail::from_vars(vars(&whole[..2])).is_err());
        let bad = [whole[0], whole[1], ("SHAKED_AGENT_LISTEN", "10.250.0.1")];
        assert!(Jail::from_vars(vars(&bad)).is_err());
    }
}
