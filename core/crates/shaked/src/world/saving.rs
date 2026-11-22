//! Saving a world's parts: after a step (the part written to and the clock parts that changed),
//! all of them for a final state or under fixed names, and comparing them with saves. Parts that
//! keep no saves (only replay) are never saved; parts with replay are not in the shared saves,
//! so they are not compared with them.

use parts::proto::{is_internal_key, DiffParams, Differing, SaveAt, SaveParams, Target};
use serde_json::Value;

use super::{new_save_id, request, Part, Saves, World, WorldError};

/// What saving after a step gave: the new saves, and the first failure (the other parts are
/// still saved).
#[derive(Debug, Default)]
pub struct StepSaves {
    pub saves: Saves,
    pub error: Option<WorldError>,
}

impl World {
    async fn save_as(&self, p: &Part, save: &str) -> Result<(), WorldError> {
        let dir = self.hub.save_dir(&p.integration, save);
        let params = SaveParams {
            instance: p.instance.clone(),
            save: save.to_string(),
            dir: dir.to_string_lossy().into_owned(),
        };
        let _: Value = request(&p.client, &p.label(), &params).await?;
        Ok(())
    }

    /// Saves `p` under a new id and records it as the part's current state; a part that serves
    /// hosts is then asked where it serves them now.
    async fn save_new(&self, p: &Part) -> Result<String, WorldError> {
        let save = new_save_id();
        let saved = self.save_as(p, &save).await;
        if saved.is_ok() && p.serves_http {
            self.refresh_routes(p).await;
        }
        let mut t = self.track.lock().unwrap();
        match saved {
            Ok(()) => {
                t.last.insert(p.name.clone(), save.clone());
                t.dirty.remove(&p.name);
                t.changed.remove(&p.name);
                Ok(save)
            }
            Err(e) => {
                t.dirty.insert(p.name.clone());
                Err(e)
            }
        }
    }

    /// The parts to save after a step: the part written to, and every clock part that changed
    /// since its last save; only parts that keep saves and save after each step.
    fn changed_parts(&self, written_part: Option<&str>) -> Vec<&Part> {
        let t = self.track.lock().unwrap();
        self.parts
            .iter()
            .filter(|p| written_part == Some(p.name.as_str()) || t.changed.contains(&p.name))
            .filter(|p| p.keeps_saves() && p.describe.save_at == SaveAt::Step)
            .collect()
    }

    /// The parts that save at points and changed (or failed to save) since their last save.
    fn unsaved_points(&self) -> Vec<&Part> {
        let t = self.track.lock().unwrap();
        let unsaved = |p: &&Part| t.changed.contains(&p.name) || t.dirty.contains(&p.name);
        let points = self
            .parts
            .iter()
            .filter(|p| p.describe.save_at == SaveAt::Points);
        points.filter(|p| p.keeps_saves()).filter(unsaved).collect()
    }

    /// True when a part that saves at points changed since its last save.
    pub fn has_unsaved_points(&self) -> bool {
        !self.unsaved_points().is_empty()
    }

    /// Saves the parts that save at points and changed since their last save (a save point).
    pub async fn save_points(&self) -> StepSaves {
        self.save_each(self.unsaved_points()).await
    }

    /// Saves the parts that changed with a step (see `changed_parts`); a part that saves at
    /// points and was written is only marked changed, for the next save point. A part whose save
    /// fails stays unsaved until its next one.
    pub async fn save_changed(&self, written_part: Option<&str>) -> StepSaves {
        self.mark_points_written(written_part);
        self.save_each(self.changed_parts(written_part)).await
    }

    /// Marks `written_part` changed when it saves at points.
    fn mark_points_written(&self, written_part: Option<&str>) {
        let written = self
            .parts
            .iter()
            .find(|p| Some(p.name.as_str()) == written_part);
        if let Some(p) = written.filter(|p| p.describe.save_at == SaveAt::Points) {
            self.track.lock().unwrap().changed.insert(p.name.clone());
        }
    }

    /// Saves each of `parts` under a new id; the new saves and the first failure.
    async fn save_each(&self, parts: Vec<&Part>) -> StepSaves {
        let mut out = StepSaves::default();
        for p in parts {
            match self.save_new(p).await {
                Ok(save) => {
                    out.saves.insert(p.name.clone(), save);
                }
                Err(e) => {
                    out.error.get_or_insert(e);
                }
            }
        }
        out
    }

    /// The part's last save, when nothing changed since.
    fn reusable_save(&self, p: &Part) -> Option<String> {
        let t = self.track.lock().unwrap();
        let unchanged = !t.dirty.contains(&p.name) && !t.changed.contains(&p.name);
        t.last.get(&p.name).filter(|_| unchanged).cloned()
    }

    /// A save of every part's current state: its last save when nothing changed since, else a
    /// new one. Parts that keep no saves are left out.
    pub async fn save_all(&self) -> Result<Saves, WorldError> {
        let mut out = Saves::new();
        for p in self.parts.iter().filter(|p| p.keeps_saves()) {
            let save = match self.reusable_save(p) {
                Some(s) => s,
                None => self.save_new(p).await?,
            };
            out.insert(p.name.clone(), save);
        }
        Ok(out)
    }

    /// Saves every part `names` names under that name.
    pub(super) async fn save_all_as(&self, names: &Saves) -> Result<(), WorldError> {
        for p in &self.parts {
            if let Some(save) = names.get(&p.name) {
                self.save_as(p, save).await?;
            }
        }
        Ok(())
    }

    /// Keys that differ between each part's live instance and `other[part]` (shared saves: base
    /// or gold), sorted, internal keys left out; prefixed `part:` only when the world has more
    /// than one part. Parts not in the shared saves are skipped (see `unchecked_parts`).
    pub async fn diff_with(&self, other: &Saves) -> Result<Vec<String>, WorldError> {
        let many = self.parts.len() > 1;
        let mut out = vec![];
        for p in self.parts.iter().filter(|p| p.in_shared_saves()) {
            let save = other.get(&p.name).ok_or_else(|| {
                WorldError::other(format!(
                    "there is no save of the part {} to compare with",
                    p.name
                ))
            })?;
            let params = DiffParams {
                a: p.target(),
                b: Target::Save { save: save.clone() },
            };
            // A malformed answer is an error, never "nothing differs".
            let d: Differing = request(&p.client, &p.label(), &params).await?;
            // A part's internal keys (a disruption's schedule) are its bookkeeping, not the world.
            let world = d.differing.into_iter().filter(|k| !is_internal_key(k));
            out.extend(world.map(|k| if many { format!("{}:{k}", p.name) } else { k }));
        }
        out.sort();
        out.dedup();
        Ok(out)
    }
}
