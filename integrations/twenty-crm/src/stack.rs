//! The database server and the Twenty servers on it, as compose projects.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use shakedown_part::helpers::blocking;
use shakedown_part::helpers::compose::Project;
use shakedown_part::helpers::pg::Templates;

use crate::settings::Settings;

pub struct Stack {
    pub settings: Settings,
    pub pg: Templates,
    db: Project,
}

/// How a Twenty server runs.
pub struct ServerSpec<'a> {
    pub instance: &'a str,
    pub db: &'a str,
    pub port: u16,
    /// Run the image's migrations at boot: only on an empty database, as they take a minute.
    pub migrate: bool,
}

/// A port free now on 127.0.0.1.
pub fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").context("finding a free port")?;
    Ok(listener.local_addr()?.port())
}

impl Stack {
    pub fn new(settings: Settings) -> Result<Stack> {
        let db = Project::new(
            &settings.project,
            &settings.files.join("compose.yml"),
            &settings_env(&settings),
        )?;
        Ok(Stack {
            pg: Templates::new(&settings.admin_url()),
            settings,
            db,
        })
    }

    /// Starts the database server, unless it runs, and waits until it is healthy.
    pub async fn up(&self) -> Result<()> {
        let db = self.db.clone();
        blocking(move || db.up()).await
    }

    /// The compose project of a server; nothing is started.
    pub fn server(&self, spec: &ServerSpec) -> Result<Project> {
        let mut env = settings_env(&self.settings);
        let values = [
            ("SHAKEDOWN_TWENTY_INSTANCE", spec.instance.to_string()),
            ("SHAKEDOWN_TWENTY_DB", spec.db.to_string()),
            ("SHAKEDOWN_TWENTY_PORT", spec.port.to_string()),
            (
                "SHAKEDOWN_TWENTY_NO_MIGRATIONS",
                (!spec.migrate).to_string(),
            ),
            ("SHAKEDOWN_TWENTY_IMAGE", self.settings.image.clone()),
        ];
        env.extend(
            values
                .into_iter()
                .flat_map(|(k, v)| shakedown_part::settings::pair(k, v)),
        );
        let name = format!("{}_{}", self.settings.project, spec.instance);
        Project::new(&name, &self.settings.files.join("instance.yml"), &env)
    }
}

fn settings_env(s: &Settings) -> BTreeMap<String, String> {
    [
        ("SHAKEDOWN_TWENTY_NAME", s.docker_name()),
        ("SHAKEDOWN_TWENTY_DB_PORT", s.db_port.to_string()),
    ]
    .into_iter()
    .flat_map(|(k, v)| shakedown_part::settings::pair(k, v))
    .collect()
}

/// Removes a server and its Redis.
pub async fn take_down(project: Project) -> Result<()> {
    blocking(move || project.down_with_volumes()).await
}

/// Starts a server and waits until it answers.
pub async fn bring_up(project: Project, port: u16, timeout: Duration) -> Result<()> {
    blocking(move || project.up()).await?;
    wait_healthy(port, timeout).await
}

async fn wait_healthy(port: u16, timeout: Duration) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("http://127.0.0.1:{port}/healthz");
    let started = Instant::now();
    while started.elapsed() < timeout {
        let answer = client.get(&url).send().await;
        if answer.is_ok_and(|r| r.status().is_success()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    bail!("Twenty on port {port} did not answer within {timeout:?}")
}
