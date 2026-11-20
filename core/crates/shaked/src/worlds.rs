//! Built worlds: how far each one's preparation got (its base saves made in the background, then
//! made current), and the current benchmark remembered across a restart in the worlds folder.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

use parts::Hub;
use serde::Serialize;

use crate::app::AppState;

/// The file in the worlds folder that names the current benchmark.
const CURRENT_FILE: &str = ".current";

/// A world's preparation: `preparing`, `ready` or `failed` (with why), and how long it took or has
/// taken so far.
#[derive(Debug, Clone, Serialize)]
pub struct BuildState {
    pub state: &'static str,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

struct Build {
    started: Instant,
    /// Once over: how long it took, and why it failed.
    ended: Option<(u64, Option<String>)>,
}

impl Build {
    fn state(&self) -> BuildState {
        let since = || self.started.elapsed().as_millis() as u64;
        let (state, elapsed_ms, error) = match &self.ended {
            None => ("preparing", since(), None),
            Some((ms, None)) => ("ready", *ms, None),
            Some((ms, Some(e))) => ("failed", *ms, Some(e.clone())),
        };
        BuildState {
            state,
            elapsed_ms,
            error,
        }
    }
}

/// The preparations since shaked started, by world name.
#[derive(Default)]
pub struct Builds(Mutex<HashMap<String, Build>>);

impl Builds {
    /// Starts timing `name`'s preparation now; false while one is under way.
    pub fn begin(&self, name: &str) -> bool {
        self.begin_since(name, Instant::now())
    }

    /// Like `begin`, timed from `started` (when the build was asked for).
    pub fn begin_since(&self, name: &str, started: Instant) -> bool {
        let mut builds = self.0.lock().unwrap();
        if builds.get(name).is_some_and(|b| b.ended.is_none()) {
            return false;
        }
        let b = Build {
            started,
            ended: None,
        };
        builds.insert(name.to_string(), b);
        true
    }

    pub fn state(&self, name: &str) -> Option<BuildState> {
        self.0.lock().unwrap().get(name).map(Build::state)
    }

    fn end(&self, name: &str, error: Option<String>) {
        if let Some(b) = self.0.lock().unwrap().get_mut(name) {
            b.ended = Some((b.started.elapsed().as_millis() as u64, error));
        }
    }
}

/// When a world's preparation was asked for, and which benchmark was current then.
#[derive(Debug, Clone)]
pub struct Asked {
    pub at: Instant,
    pub current: String,
}

impl Asked {
    /// Asked now.
    pub fn now(state: &AppState) -> Asked {
        Asked {
            at: Instant::now(),
            current: state.benchmark(),
        }
    }
}

/// Prepares `name` on a task of its own unless it is being prepared (false then).
pub fn spawn_prepare(state: &Arc<AppState>, name: &str, asked: Asked) -> bool {
    if !state.builds.begin_since(name, asked.at) {
        return false;
    }
    tokio::spawn(prepare(state.clone(), name.to_string(), asked.current));
    true
}

/// Makes the base saves of the benchmark `name` (held off by a cleanup, as a starting run is),
/// then makes it the current benchmark unless another was chosen since `was` was (that choice
/// stands); the outcome is recorded in `state.builds`.
pub async fn prepare(state: Arc<AppState>, name: String, was: String) {
    let _no_cleanup = state.cleanup.clone().read_owned().await;
    let made = async {
        let bench = state.bench_named(&name).await?;
        crate::world::ensure_base(&state.hub, &bench).await?;
        state
            .use_benchmark_if(&name, &was)
            .map_err(anyhow::Error::msg)
    };
    let error = made.await.err().map(|e| format!("{e:#}"));
    state.builds.end(&name, error);
}

/// The benchmark named in `dir`'s current file, when it is installed.
pub fn remembered(hub: &Hub, dir: Option<&Path>) -> Option<String> {
    let text = std::fs::read_to_string(dir?.join(CURRENT_FILE)).ok()?;
    let name = text.trim();
    let registry = hub.registry();
    let m = registry.get(name)?;
    (m.kind == parts::Kind::Benchmark).then(|| name.to_string())
}

/// Writes `name` to `dir`'s current file (a failure is logged).
pub fn remember(dir: Option<&Path>, name: &str) {
    let Some(dir) = dir else {
        return;
    };
    if let Err(e) = std::fs::write(dir.join(CURRENT_FILE), format!("{name}\n")) {
        eprintln!(
            "shaked: could not remember the current benchmark in {}: {e}",
            dir.display()
        );
    }
}

/// Removes the temporary folders a build left in `dir` (`.<name>.tmp-<hex>`), before it is scanned.
pub fn remove_leftovers(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && name.contains(".tmp-") {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_is_prepared_once_at_a_time_and_again_after_it_ended() {
        let builds = Builds::default();
        assert!(builds.begin("w"));
        assert!(!builds.begin("w"));
        assert_eq!(builds.state("w").unwrap().state, "preparing");
        builds.end("w", Some("no".into()));
        let failed = builds.state("w").unwrap();
        assert_eq!(
            (failed.state, failed.error.as_deref()),
            ("failed", Some("no"))
        );
        assert!(builds.begin("w"));
        builds.end("w", None);
        assert_eq!(builds.state("w").unwrap().state, "ready");
        assert!(builds.state("other").is_none());
    }
}
