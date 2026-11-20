//! The config of a `service` instance.
use std::collections::BTreeMap;
use std::path::PathBuf;

use arena_part::helpers::compose::{valid_image, valid_volume, DEFAULT_HELPER};
use arena_part::PartError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::probes::ProbeSpec;
pub use crate::watches::WatchSpec;

/// A compose service name: lowercase letters, digits, `_` and `-`.
fn valid_service(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && (s.bytes())
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The compose variable that names `service`'s image: `IMAGE_<SERVICE>`, upper case, `-` as `_`.
pub fn image_var(service: &str) -> String {
    format!("IMAGE_{}", service.to_ascii_uppercase().replace('-', "_"))
}

/// How a save holds a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveHow {
    /// A tar archive of the volume, copied with the services that use it stopped (the default).
    #[default]
    Tar,
    /// A plain `pg_dump` of the database the volume holds, made while it runs.
    Postgres,
    /// A copy of the volume's files hard-linked to the instance's previous save where unchanged.
    Hardlink,
    /// Not saved: a start from a save makes it empty (a cache).
    Skip,
}

impl SaveHow {
    fn word(self) -> &'static str {
        match self {
            SaveHow::Tar => "tar",
            SaveHow::Postgres => "postgres",
            SaveHow::Hardlink => "hardlink",
            SaveHow::Skip => "skip",
        }
    }
}

/// A volume a save holds, and how. A plain name in the config is a `tar` volume.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "VolumeEntry")]
pub struct VolumeSpec {
    pub name: String,
    pub save: SaveHow,
    /// For `tar` and `hardlink`: the compose services to stop while the volume is copied (none:
    /// the whole project).
    pub services: Vec<String>,
    /// For `postgres`: the compose service running the database.
    pub service: Option<String>,
    /// For `postgres`: the database (default `postgres`) and user (default `postgres`).
    pub database: Option<String>,
    pub user: Option<String>,
    /// For `tar` and `hardlink`: copied while everything runs (a store whose own recovery reads
    /// a crash-consistent copy), so nothing stops for it.
    pub live_copy: bool,
}

/// A volume as the config gives it: a name, or a spec.
#[derive(Deserialize)]
#[serde(untagged)]
enum VolumeEntry {
    Name(String),
    Spec {
        name: String,
        #[serde(default)]
        save: SaveHow,
        #[serde(default)]
        services: Vec<String>,
        #[serde(default)]
        service: Option<String>,
        #[serde(default)]
        database: Option<String>,
        #[serde(default)]
        user: Option<String>,
        #[serde(default)]
        live_copy: bool,
    },
}

impl From<VolumeEntry> for VolumeSpec {
    fn from(e: VolumeEntry) -> VolumeSpec {
        match e {
            VolumeEntry::Name(name) => VolumeSpec::plain(name),
            VolumeEntry::Spec {
                name,
                save,
                services,
                service,
                database,
                user,
                live_copy,
            } => VolumeSpec {
                name,
                save,
                services,
                service,
                database,
                user,
                live_copy,
            },
        }
    }
}

impl VolumeSpec {
    /// A volume saved as a tar archive with the whole project stopped (a plain name).
    pub fn plain(name: String) -> VolumeSpec {
        VolumeSpec {
            name,
            save: SaveHow::Tar,
            services: vec![],
            service: None,
            database: None,
            user: None,
            live_copy: false,
        }
    }

    pub fn database(&self) -> &str {
        self.database.as_deref().unwrap_or("postgres")
    }

    pub fn user(&self) -> &str {
        self.user.as_deref().unwrap_or("postgres")
    }

    /// True for a volume copied as files (`tar`, `hardlink`): the services using it stop.
    pub fn is_file_copy(&self) -> bool {
        matches!(self.save, SaveHow::Tar | SaveHow::Hardlink)
    }

    /// True for a file copy that stops the services using it (not `live_copy`).
    pub fn stops(&self) -> bool {
        self.is_file_copy() && !self.live_copy
    }
}

/// A host the instance serves through shaked's proxy: requests to `host` go to `port` of the
/// compose service `service`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HttpSpec {
    pub host: String,
    pub service: String,
    pub port: u16,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub reads: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub compose_file: PathBuf,
    /// The compose file's volumes that a save holds, and how.
    pub volumes: Vec<VolumeSpec>,
    /// The image whose `tar`, `sh`, `cp` and `find` copy volumes, archives and hard-linked trees
    /// alike (default `redis:6.2-alpine`, never pulled).
    #[serde(default)]
    pub helper_image: Option<String>,
    /// The hosts the instance serves.
    #[serde(default)]
    pub http: Vec<HttpSpec>,
    /// Extra environment for `docker compose` (variables the compose file uses).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Writes the world clock to `clock.json` in a folder the compose file mounts
    /// (`ARENA_CLOCK_DIR`) at start and each advance (the `live-service` package only).
    #[serde(default)]
    pub live: bool,
    /// The image each compose service runs, passed as `IMAGE_<SERVICE>`; `service_deploy`
    /// changes one, and a save keeps them.
    #[serde(default)]
    pub images: BTreeMap<String, String>,
    /// Docs read over HTTP from the project's services.
    #[serde(default)]
    pub probes: Vec<ProbeSpec>,
    /// Probes polled on the world clock whose items appearing and going are world events (the
    /// `live-service` package only).
    #[serde(default)]
    pub watches: Vec<WatchSpec>,
    /// How long a deployed service may take to come up healthy (default 300 s).
    #[serde(default)]
    pub deploy_wait_s: Option<u64>,
}

impl Config {
    pub fn parse(v: &Value) -> Result<Config, PartError> {
        let c: Config = serde_json::from_value(v.clone())
            .map_err(|e| PartError::invalid(format!("config: {e}")))?;
        if !c.compose_file.is_file() {
            return Err(PartError::invalid(format!(
                "compose_file {:?} is not a file",
                c.compose_file
            )));
        }
        c.check_volumes()?;
        c.check_images()?;
        c.check_watches()?;
        Ok(c)
    }

    /// Each volume a name, with what its way of saving needs.
    fn check_volumes(&self) -> Result<(), PartError> {
        for v in &self.volumes {
            if !valid_volume(&v.name) {
                return Err(PartError::invalid(format!(
                    "{:?} is not a volume name",
                    v.name
                )));
            }
            let (name, how) = (&v.name, v.save.word());
            if v.save == SaveHow::Postgres && v.service.is_none() {
                return Err(PartError::invalid(format!(
                    "volume {name} saves by {how}, which needs service (the compose service running the database)"
                )));
            }
        }
        Ok(())
    }

    /// The helper image and every service's image a name, and every service a compose name.
    fn check_images(&self) -> Result<(), PartError> {
        let images = self.helper_image.iter().chain(self.images.values());
        if let Some(i) = images.into_iter().find(|i| !valid_image(i)) {
            return Err(PartError::invalid(format!("{i:?} is not an image name")));
        }
        match self.images.keys().find(|s| !valid_service(s)) {
            Some(s) => Err(PartError::invalid(format!("{s:?} is not a service name"))),
            None => Ok(()),
        }
    }

    /// Each watch reads a probe the config has.
    fn check_watches(&self) -> Result<(), PartError> {
        let probe = |key: &str| self.probes.iter().any(|p| p.key == key);
        match self.watches.iter().find(|w| !probe(&w.probe)) {
            Some(w) => Err(PartError::invalid(format!(
                "a watch reads the probe {}, which the config does not have",
                w.probe
            ))),
            None => Ok(()),
        }
    }

    /// What the instance needs the world clock for, when it needs it.
    pub fn needs_clock(&self) -> Option<&'static str> {
        match (self.live, self.watches.is_empty()) {
            (true, _) => Some("live"),
            (false, false) => Some("watches"),
            (false, true) => None,
        }
    }

    pub fn deploy_wait(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.deploy_wait_s.unwrap_or(300))
    }

    pub fn helper(&self) -> &str {
        self.helper_image.as_deref().unwrap_or(DEFAULT_HELPER)
    }
}
