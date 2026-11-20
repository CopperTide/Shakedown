//! The world tool `service_deploy {service, image, sha, ref}`: the project runs `image` for
//! `service` from now on. The call answers `{accepted: true}` at once; in the background the
//! service is recreated with its new `IMAGE_<SERVICE>` (`compose up -d --no-deps`) and watched
//! until compose says it is healthy (or running, for a service without a health check), at most
//! the config's `deploy_wait_s` (300 s). The next advance then reports `deployed {by:
//! "pipeline", result: {job_id: <ref>, service, image, sha, ok, error?}}`. A deploy that fails
//! leaves the new image in place, as a real failed rollout does.

use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use arena_part::helpers::compose::{valid_image, Project};
use arena_part::proto::{Effect, ToolSchema, WorldEvent};
use serde::Deserialize;
use serde_json::{json, Value};

pub const TOOL: &str = "service_deploy";

pub fn schema() -> ToolSchema {
    ToolSchema {
        name: TOOL.into(),
        description:
            "Run a new image for one of the project's services (a pipeline's deploy step): \
                      answers at once, and a deployed event reports whether it came up healthy."
                .into(),
        input_schema: json!({"type": "object", "required": ["service", "image"], "properties": {
            "service": {"type": "string"}, "image": {"type": "string"},
            "sha": {"type": "string"}, "ref": {"type": "string", "description": "the caller's id for this deploy (a CI job), given back as job_id"}}}),
        effect: Effect::Write,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deploy {
    pub service: String,
    pub image: String,
    #[serde(default)]
    pub sha: Value,
    #[serde(default, rename = "ref")]
    pub reference: Value,
}

impl Deploy {
    pub fn parse(args: &Value) -> Result<Deploy, String> {
        let d: Deploy = serde_json::from_value(args.clone()).map_err(|e| format!("{TOOL}: {e}"))?;
        if !valid_image(&d.image) {
            return Err(format!("{:?} is not an image name", d.image));
        }
        Ok(d)
    }

    /// The `deployed` event of this deploy, `outcome` its error if it failed.
    pub fn event(&self, outcome: &Result<()>) -> WorldEvent {
        let mut result = json!({"job_id": self.reference, "service": self.service,
                                "image": self.image, "sha": self.sha, "ok": outcome.is_ok()});
        let text = match outcome {
            Ok(()) => format!("{} runs {}", self.service, self.image),
            Err(e) => {
                result["error"] = json!(format!("{e:#}"));
                format!("deploying {} to {} failed: {e:#}", self.image, self.service)
            }
        };
        WorldEvent {
            kind: "deployed".into(),
            text,
            data: json!({"by": "pipeline", "result": result}),
        }
    }
}

/// Recreates `service` on `project` (whose environment has the new image) and waits until it
/// is healthy, at most `wait`.
pub fn roll_out(project: &Project, service: &str, wait: Duration) -> Result<()> {
    project.up_no_deps(&[service])?;
    let since = Instant::now();
    loop {
        let state = project.state(service)?;
        match judge(&state) {
            Some(true) => return Ok(()),
            Some(false) => bail!("{service} is {state}"),
            None if since.elapsed() >= wait => {
                bail!(
                    "{service} was not healthy within {} s ({state})",
                    wait.as_secs()
                )
            }
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// Waits until every container of `project` is up (healthy, or running without a health check)
/// or finished well (a one-shot that exited 0), at most `wait`; a container that failed (exited
/// otherwise, or unhealthy) fails it at once.
pub fn wait_up(project: &Project, wait: Duration) -> Result<()> {
    let since = Instant::now();
    loop {
        let states = project.states()?;
        let mut waiting = None;
        for [service, state, health, code] in &states {
            match (state.as_str(), judge(&format!("{state} {health}"))) {
                ("exited", _) if code == "0" => {}
                ("exited", _) => bail!("{service} exited with {code}"),
                (_, Some(true)) => {}
                (_, Some(false)) => bail!("{service} is {state} {health}"),
                (_, None) => waiting = Some(format!("{service} is {state} {health}")),
            }
        }
        let Some(why) = waiting else { return Ok(()) };
        if since.elapsed() >= wait {
            bail!("the project was not up within {} s: {why}", wait.as_secs());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Whether compose's `<state> <health>` is up (Some(true)), failed (Some(false)) or not yet.
fn judge(state: &str) -> Option<bool> {
    let mut words = state.split_whitespace();
    match (words.next(), words.next()) {
        (Some("running"), Some("healthy") | None) => Some(true),
        (Some("running"), Some("unhealthy")) => Some(false),
        (Some("exited" | "dead"), _) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_states_are_judged() {
        assert_eq!(judge("running healthy"), Some(true));
        assert_eq!(judge("running"), Some(true));
        assert_eq!(judge("running starting"), None);
        assert_eq!(judge("restarting"), None);
        assert_eq!(judge("running unhealthy"), Some(false));
        assert_eq!(judge("exited"), Some(false));
    }
}
