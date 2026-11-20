//! The `service` integration: each instance is a Docker Compose project `arena_<instance>`, and a
//! save holds copies of the volumes the config names, each its own way (`volumes::*`).
//!
//! A plain volume name is saved as before: the project stops, each volume is written to
//! `<save>/<volume>.tar` exactly as the helper container streams it (owners, modes and mtimes
//! kept) and the project starts again, so the save is exact. A `postgres` volume is dumped while
//! it runs; a `hardlink` one copied as a tree linked to the instance's previous save (by the same
//! helper image); a `live_copy` one copied without stopping anything; a `skip` one not saved.
//! Starting from a save makes a new project whose volumes are filled from it. Each running
//! instance's config (with the images it runs now) is kept in `.live/<instance>.json` and its
//! last save in `.last/<instance>.json`, so a restarted program still knows them. shaked saves
//! the part at save points (`save_at: "points"`). Live volumes are not counted by `usage`: `stop`
//! removes them.
//!
//! The same program is two packages. `service` has no clock and no tools. `live-service` runs a
//! live platform (`live`): a world clock file for its containers (`clock`), deploys as the world
//! tool `service_deploy` (`deploy`) and watches as world events (`watches`), on the world clock.
//! Both read probes as docs (`probes`) and pass `images` to compose as `IMAGE_<SERVICE>`.
mod clock;
mod config;
mod deploy;
mod live;
mod probes;
mod routes;
mod view;
mod volumes;
mod watches;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arena_part::helpers::blocking;
use arena_part::helpers::compose::Project;
use arena_part::helpers::instances::{KeyedLocks, LiveRecords, Registry};
use arena_part::helpers::save_dirs::SaveDirs;
use arena_part::proto::*;
use arena_part::{Part, PartError};
use async_trait::async_trait;
use config::{image_var, Config};
use serde_json::{json, Value};
use volumes::{Instance, Manifest};

/// Which package the program is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Plain,
    Live,
}

impl Mode {
    fn package(self) -> &'static str {
        match self {
            Mode::Plain => "service",
            Mode::Live => "live-service",
        }
    }
}

pub struct Service {
    mode: Mode,
    saves: SaveDirs,
    /// The config each running instance started with (its images as deploys left them), in
    /// `.live/<instance>.json`.
    live: LiveRecords<Value>,
    /// Each instance's last save (or the save it started from), `{"save"}` in
    /// `.last/<instance>.json`: what its hard-linked trees link to.
    last: LiveRecords<Value>,
    /// What each live instance has to report on the next advance, and what its watches saw.
    runtime: Arc<Registry<live::Runtime>>,
    /// One deploy at a time per instance.
    deploys: Arc<KeyedLocks>,
}

impl Service {
    /// The `service` package, saves under env `ARENA_SAVES`.
    pub fn from_env() -> Service {
        Service::package_from_env(Mode::Plain)
    }

    /// The `live-service` package, saves under env `ARENA_SAVES`.
    pub fn live_from_env() -> Service {
        Service::package_from_env(Mode::Live)
    }

    fn package_from_env(mode: Mode) -> Service {
        let saves = SaveDirs::new(mode.package());
        saves.sweep_scratch();
        Service::with(mode, saves)
    }

    /// The `service` package, saves in `package_folder` (`<ARENA_SAVES>/service`).
    pub fn new(package_folder: PathBuf) -> Service {
        Service::with(Mode::Plain, SaveDirs::under(package_folder))
    }

    /// The `live-service` package, saves in `package_folder`.
    pub fn live(package_folder: PathBuf) -> Service {
        Service::with(Mode::Live, SaveDirs::under(package_folder))
    }

    fn with(mode: Mode, saves: SaveDirs) -> Service {
        Service {
            mode,
            live: LiveRecords::files(saves.root().join(".live")),
            last: LiveRecords::files(saves.root().join(".last")),
            saves,
            runtime: Arc::default(),
            deploys: Arc::default(),
        }
    }

    /// The running instance `id` and its config, rebuilt from what was recorded when it started.
    fn instance(&self, id: &str) -> Result<(Instance, Config), PartError> {
        let config = self
            .live
            .get(id)
            .ok_or_else(|| PartError::unknown(format!("no instance {id}")))?;
        let config = Config::parse(&config)?;
        Ok((self.new_instance(id, &config)?, config))
    }

    fn clock_dir(&self, id: &str) -> PathBuf {
        clock::dir_of(self.saves.root(), id)
    }

    /// The instance `id` with `config`, its project not yet run: compose gets the config's env,
    /// each image as `IMAGE_<SERVICE>` and, when live, the clock folder as `ARENA_CLOCK_DIR`.
    fn new_instance(&self, id: &str, config: &Config) -> Result<Instance, PartError> {
        let mut env = config.env.clone();
        env.extend((config.images.iter()).map(|(s, i)| (image_var(s), i.clone())));
        if config.live {
            let dir = self.clock_dir(id).to_string_lossy().into_owned();
            env.insert("ARENA_CLOCK_DIR".into(), dir);
        }
        let name = format!("arena_{id}");
        let project = Project::new(&name, &config.compose_file, &env);
        let project = project.and_then(|p| p.with_helper(config.helper()));
        Ok(Instance {
            project: project.map_err(PartError::internal_from)?,
            volumes: config.volumes.clone(),
            helper_image: config.helper().to_string(),
            env: config.env.clone(),
            images: config.images.clone(),
        })
    }

    /// The last save of `id` (what its hard-linked trees link to).
    fn last_save(&self, id: &str) -> Option<String> {
        let last = self.last.get(id)?;
        last["save"].as_str().map(str::to_string)
    }

    fn note_last(&self, id: &str, save: &str) {
        if let Err(e) = self.last.record(id, &json!({"save": save})) {
            eprintln!("service: recording the last save of {id}: {e}");
        }
    }

    /// The plain package has no clock: a config that needs one is refused.
    fn check_mode(&self, config: &Config) -> Result<(), PartError> {
        match (self.mode, config.needs_clock()) {
            (Mode::Plain, Some(what)) => Err(PartError::invalid(format!(
                "{what} needs the world clock: run it with the live-service package"
            ))),
            _ => Ok(()),
        }
    }

    /// The docs of a target: volume digests (only when asked for one, or for all), probes and
    /// the images each service runs.
    async fn docs(&self, t: &Target, keys: &[String]) -> Result<Docs, PartError> {
        let wanted = |k: &str| keys.is_empty() || keys.iter().any(|x| x == k);
        let volumes = keys.is_empty() || keys.iter().any(|k| k.starts_with("volume/"));
        let mut all = serde_json::Map::new();
        match t {
            Target::Save { save } => {
                let dir = self.saves.require(save)?;
                if volumes {
                    all.extend(view::volume_docs(&view::digests(&dir)?));
                }
                all.extend(probes::read_saved(&dir));
                let images = Manifest::read(&dir).map(|m| m.images).unwrap_or_default();
                all.extend(view::image_docs(&images));
            }
            Target::Instance { instance } => {
                let (inst, config) = self.instance(instance)?;
                if volumes {
                    all.extend(view::volume_docs(&self.live_digests(&inst).await?));
                }
                let world = live::world_ms(&self.clock_dir(instance));
                let read = probes::read_all(&inst.project, &config.probes, wanted, world);
                all.extend(read.await);
                all.extend(view::image_docs(&config.images));
            }
        }
        Ok(view::select(all, keys))
    }

    /// The digests of a copy of a live instance's volumes.
    async fn live_digests(&self, inst: &Instance) -> Result<view::DigestMap, PartError> {
        let (inst, scratch) = (inst.clone(), self.saves.scratch());
        blocking(move || {
            use arena_part::helpers::temp_dir::TempDir;
            let tmp = TempDir::new_in(&scratch, "inspect").map_err(PartError::internal_from)?;
            volumes::export_all(&inst, tmp.path()).map_err(PartError::internal_from)?;
            view::digests(tmp.path())
        })
        .await
    }

    /// The digests of a target's volumes, for `diff`.
    async fn digests(&self, t: &Target) -> Result<view::DigestMap, PartError> {
        match t {
            Target::Save { save } => view::digests(&self.saves.require(save)?),
            Target::Instance { instance } => self.live_digests(&self.instance(instance)?.0).await,
        }
    }

    /// Forgets the instance `id` (its records, clock folder and what it had to report).
    fn forget(&self, id: &str) {
        self.live.forget(id);
        self.last.forget(id);
        self.runtime.remove(id);
        self.deploys.forget(id);
        let _ = std::fs::remove_dir_all(self.clock_dir(id));
    }
}

/// Starts the project fresh, or from the folders of `save`; a failure leaves nothing running.
/// With `wait` (the live package) compose does not wait itself: the project is up once every
/// container is healthy or a one-shot that finished well (`deploy::wait_up`), at most `wait`.
fn bring_up(inst: &Instance, save: Option<&Path>, wait: Option<Duration>) -> Result<(), PartError> {
    let start = || {
        inst.project.down_with_volumes()?; // a leftover project would keep its old volumes
        match (save, wait) {
            (Some(dir), _) => volumes::restore_all(inst, dir)?,
            (None, Some(_)) => inst.project.up_services(&[])?,
            (None, None) => inst.project.up()?,
        }
        wait.map_or(Ok(()), |w| deploy::wait_up(&inst.project, w))
    };
    start().map_err(|e| {
        let _ = inst.project.down_with_volumes();
        PartError::internal_from(e)
    })
}

#[async_trait]
impl Part for Service {
    async fn describe(&self) -> Result<Describe, PartError> {
        let live = self.mode == Mode::Live;
        Ok(Describe {
            name: self.mode.package().into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL,
            restore: Restore {
                exact: ExactKind::Save,
                approximate: false,
            },
            uses_clock: live,
            fingerprint: json!({"integration": self.mode.package()}),
            tools: if live { vec![deploy::schema()] } else { vec![] },
            system: Some(SystemInfo::real("api", "HTTP service", None)),
            save_at: SaveAt::Points,
        })
    }

    async fn start(&self, p: StartParams) -> Result<(), PartError> {
        let mut config = Config::parse(&p.config)?;
        self.check_mode(&config)?;
        let from = match &p.from {
            Some(f) => Some(self.saves.require(&f.save)?),
            None => None,
        };
        let record = live::record_with_saved_images(&p.config, &mut config, from.as_deref());
        let (id, inst) = (&p.instance, self.new_instance(&p.instance, &config)?);
        // recorded first, so a project that comes up is never unknown to a restarted program
        self.live
            .record(id, &record)
            .map_err(PartError::internal_from)?;
        let up = match self.write_first_clock(id, &config, &p) {
            Ok(()) => {
                let wait = (self.mode == Mode::Live).then(|| config.deploy_wait());
                blocking(move || bring_up(&inst, from.as_deref(), wait)).await
            }
            Err(e) => Err(e),
        };
        match (&up, &p.from) {
            (Err(_), _) => self.forget(id),
            (Ok(()), Some(f)) => self.note_last(id, &f.save),
            (Ok(()), None) => self.last.forget(id),
        }
        if up.is_ok() {
            let saved = p.from.as_ref().and_then(|f| self.saves.find(&f.save));
            let rt = live::Runtime::at_start(&config, p.clock_ms, saved.as_deref());
            self.runtime.insert(id.clone(), rt);
        }
        up
    }

    async fn call_tool(&self, p: CallToolParams) -> Result<ToolOutput, PartError> {
        let (inst, config) = self.instance(&p.instance)?;
        if self.mode == Mode::Live && p.name == deploy::TOOL {
            return self.deploy(&p.instance, inst, config, &p.args);
        }
        Ok(ToolOutput {
            error: Some(format!("the service integration has no tool {}", p.name)),
            ..ToolOutput::default()
        })
    }

    async fn advance(&self, p: AdvanceParams) -> Result<Advanced, PartError> {
        let (inst, config) = self.instance(&p.instance)?;
        self.advance_live(&p.instance, &inst, &config, p.clock_ms)
            .await
    }

    async fn save(&self, p: SaveParams) -> Result<Saved, PartError> {
        let (inst, config) = self.instance(&p.instance)?;
        let world = live::world_ms(&self.clock_dir(&p.instance));
        let docs = probes::read_all(&inst.project, &config.probes, |_| true, world).await;
        let seen = live::seen(&self.runtime, &p.instance);
        let dir = PathBuf::from(&p.dir);
        let (to, prev) = (dir.clone(), self.last_save(&p.instance));
        let save = move || {
            let bytes = volumes::save_all(&inst, &to, prev.as_deref())?;
            live::write_views(&to, &config, &docs, &seen)?;
            anyhow::Ok(bytes)
        };
        let saved = blocking(move || save().map_err(PartError::internal_from)).await;
        let bytes = saved?;
        self.saves.note(&p.save, &dir);
        self.note_last(&p.instance, &p.save);
        Ok(Saved { bytes, exact: true })
    }

    async fn inspect(&self, p: InspectParams) -> Result<Docs, PartError> {
        self.docs(&p.target, &p.keys).await
    }

    async fn diff(&self, p: DiffParams) -> Result<Differing, PartError> {
        let (a, b) = (self.digests(&p.a).await?, self.digests(&p.b).await?);
        Ok(Differing {
            differing: view::diff(&a, &b),
        })
    }

    async fn drop_save(&self, p: SaveId) -> Result<(), PartError> {
        if let Some(dir) = self.saves.find(&p.save) {
            let trees = move || volumes::remove_trees(&dir).map_err(PartError::internal_from);
            blocking(trees).await?;
        }
        self.saves.remove(&p.save).map_err(PartError::internal_from)
    }

    async fn stop(&self, p: IdParams) -> Result<(), PartError> {
        let (inst, _) = self.instance(&p.instance)?;
        blocking(move || {
            inst.project
                .down_with_volumes()
                .map_err(PartError::internal_from)
        })
        .await?;
        self.forget(&p.instance);
        Ok(())
    }

    async fn http_routes(&self, p: IdParams) -> Result<HttpRoutes, PartError> {
        let (inst, config) = self.instance(&p.instance)?;
        let routes = move || routes::routes(&inst.project, &config.http);
        blocking(move || routes().map_err(PartError::internal_from)).await
    }

    /// Nothing is kept outside `ARENA_SAVES`: live volumes are removed by `stop`.
    async fn usage(&self) -> Result<Usage, PartError> {
        Ok(Usage { bytes: 0 })
    }
}
