//! A live platform on the world clock (the `live-service` package): the clock file each
//! advance, the deploys it rolls out in the background and reports on the next advance, and the
//! watches it polls. A save keeps the images (`save.json`), the probes' docs and what the
//! watches saw.

use std::path::Path;

use arena_part::helpers::blocking;
use arena_part::helpers::instances::Registry;
use arena_part::proto::{Advanced, StartParams, ToolOutput, WorldEvent};
use arena_part::PartError;
use serde_json::{json, Map, Value};

use crate::clock::{wall_now_ms, Clock};
use crate::config::Config;
use crate::deploy::{self, Deploy};
use crate::volumes::{Instance, Manifest};
use crate::watches::{self, Seen, Watching};
use crate::{probes, Service};

/// What a live instance has to report on the next advance, and what its watches saw.
#[derive(Debug, Clone, Default)]
pub struct Runtime {
    pub deployed: Vec<WorldEvent>,
    /// One per watch of the config, in its order.
    pub watching: Vec<Watching>,
}

impl Runtime {
    /// Watches that poll first at `clock_ms`, knowing what the save `from` saw.
    pub fn at_start(config: &Config, clock_ms: i64, from: Option<&Path>) -> Runtime {
        let mut saw = from.map(watches::read).unwrap_or_default().into_iter();
        let watching = (config.watches.iter())
            .map(|_| Watching {
                next_ms: clock_ms,
                seen: saw.next().unwrap_or_default(),
            })
            .collect();
        Runtime {
            deployed: vec![],
            watching,
        }
    }
}

/// The record to keep for a start with `given` (the config as shaked gave it): the images of
/// the save `from` win over the config's, in `config` too.
pub fn record_with_saved_images(given: &Value, config: &mut Config, from: Option<&Path>) -> Value {
    let saved = from.and_then(Manifest::read).map(|m| m.images);
    let Some(saved) = saved.filter(|i| !i.is_empty()) else {
        return given.clone();
    };
    config.images.extend(saved);
    let mut record = given.clone();
    record["images"] = json!(config.images);
    record
}

/// The world's unix ms as the clock folder `dir` last said it, else the wall clock's.
pub fn world_ms(dir: &Path) -> i64 {
    Clock::read(dir).map_or_else(wall_now_ms, |c| c.world_ms)
}

/// What each watch of instance `id` saw, in config order.
pub fn seen(runtime: &Registry<Runtime>, id: &str) -> Vec<Seen> {
    let rt = runtime.get(id).unwrap_or_default();
    rt.watching.into_iter().map(|w| w.seen).collect()
}

/// Writes the probes' docs and what the watches saw into the save `dir`, when the config has any.
pub fn write_views(
    dir: &Path,
    config: &Config,
    docs: &Map<String, Value>,
    seen: &[Seen],
) -> anyhow::Result<()> {
    if !config.probes.is_empty() {
        probes::write(dir, docs)?;
    }
    if !config.watches.is_empty() {
        watches::write(dir, seen)?;
    }
    Ok(())
}

impl Service {
    /// A live instance's first clock file, before its project comes up (its containers mount it).
    pub(crate) fn write_first_clock(
        &self,
        id: &str,
        config: &Config,
        p: &StartParams,
    ) -> Result<(), PartError> {
        if !config.live {
            return Ok(());
        }
        let clock = Clock::at_start(p.world_time, p.clock_ms);
        clock
            .write(&self.clock_dir(id))
            .map_err(PartError::internal_from)
    }

    /// `service_deploy`: records the new image, answers at once, and rolls it out in the background.
    pub(crate) fn deploy(
        &self,
        id: &str,
        inst: Instance,
        mut config: Config,
        args: &Value,
    ) -> Result<ToolOutput, PartError> {
        let refused = |error: String| {
            Ok(ToolOutput {
                error: Some(error),
                ..ToolOutput::default()
            })
        };
        let d = match Deploy::parse(args) {
            Ok(d) => d,
            Err(e) => return refused(e),
        };
        if !config.images.contains_key(&d.service) {
            return refused(format!("the project has no service {}", d.service));
        }
        config.images.insert(d.service.clone(), d.image.clone());
        let mut record = self.live.get(id).unwrap_or_else(|| json!({}));
        record["images"] = json!(config.images);
        self.live
            .record(id, &record)
            .map_err(PartError::internal_from)?;
        let inst = Instance {
            project: self.new_instance(id, &config)?.project,
            ..inst
        };
        self.roll_out_later(id, inst, &config, d.clone());
        Ok(ToolOutput {
            result: Some(json!({"accepted": true})),
            writes: vec![format!("image/{}", d.service)],
            ..ToolOutput::default()
        })
    }

    /// Rolls `d` out on `inst` (one deploy at a time per instance) and queues its `deployed` event.
    fn roll_out_later(&self, id: &str, inst: Instance, config: &Config, d: Deploy) {
        let (lock, runtime) = (self.deploys.lock_for(id), self.runtime.clone());
        let (id, wait) = (id.to_string(), config.deploy_wait());
        if !runtime.contains(&id) {
            runtime.insert(id.clone(), Runtime::at_start(config, 0, None));
        }
        tokio::spawn(async move {
            let _one = lock.lock().await;
            let (project, service) = (inst.project, d.service.clone());
            let out: anyhow::Result<()> =
                blocking(move || deploy::roll_out(&project, &service, wait)).await;
            runtime.update(&id, |rt| rt.deployed.push(d.event(&out)));
        });
    }

    /// Moves a live instance's clock to `clock_ms` and reports its deploys and watches.
    pub(crate) async fn advance_live(
        &self,
        id: &str,
        inst: &Instance,
        config: &Config,
        clock_ms: i64,
    ) -> Result<Advanced, PartError> {
        let dir = self.clock_dir(id);
        if config.live {
            let last = Clock::read(&dir).unwrap_or_else(|| Clock::at_start(None, clock_ms));
            last.moved_to(clock_ms)
                .write(&dir)
                .map_err(PartError::internal_from)?;
        }
        if !self.runtime.contains(id) {
            self.runtime
                .insert(id.to_string(), Runtime::at_start(config, clock_ms, None));
        }
        let mut events = vec![];
        self.runtime
            .update(id, |rt| events.append(&mut rt.deployed));
        events.extend(self.poll_watches(id, inst, config, clock_ms).await);
        Ok(Advanced {
            changed: config.live,
            events,
        })
    }

    /// The events of the watches due at `clock_ms`.
    async fn poll_watches(
        &self,
        id: &str,
        inst: &Instance,
        config: &Config,
        clock_ms: i64,
    ) -> Vec<WorldEvent> {
        let mut events = vec![];
        let world = world_ms(&self.clock_dir(id));
        for (i, w) in config.watches.iter().enumerate() {
            let mut state = self
                .runtime
                .get(id)
                .and_then(|rt| rt.watching.get(i).cloned())
                .unwrap_or_default();
            if state.next_ms > clock_ms {
                continue;
            }
            state.next_ms = clock_ms + w.every_ms.max(1);
            let probe = config.probes.iter().find(|p| p.key == w.probe);
            let doc = match probe {
                Some(p) => probes::read(&inst.project, p, world).await.ok(),
                None => None,
            };
            if let Some(doc) = doc {
                events.extend(w.step(&mut state, &doc));
            }
            self.runtime.update(id, |rt| {
                if let Some(slot) = rt.watching.get_mut(i) {
                    *slot = state;
                }
            });
        }
        events
    }
}
