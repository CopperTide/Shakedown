//! The config of a `service` instance.
use std::collections::BTreeMap;
use std::path::PathBuf;

use arena_part::helpers::compose::{valid_image, valid_volume, DEFAULT_HELPER};
use arena_part::PartError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
            } => VolumeSpec {
                name,
                save,
                services,
                service,
                database,
                user,
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
}

/// A host the instance serves through arenad's proxy: requests to `host` go to `port` of the
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

    /// The helper image a name.
    fn check_images(&self) -> Result<(), PartError> {
        match self.helper_image.as_deref().filter(|i| !valid_image(i)) {
            Some(i) => Err(PartError::invalid(format!("{i:?} is not an image name"))),
            None => Ok(()),
        }
    }

    pub fn helper(&self) -> &str {
        self.helper_image.as_deref().unwrap_or(DEFAULT_HELPER)
    }
}
