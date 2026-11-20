//! Quotas: per-plan presets (`docs/quotas.toml`, built in; `SHAKEDOWN_QUOTAS` names another file),
//! the meters they limit, and what a check says.

use std::{collections::BTreeMap, path::Path, sync::OnceLock};

use anyhow::{Context, Result};
use serde::Deserialize;

/// What is metered or limited (`shakedown.usage.meter`, `shakedown.quotas.meter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Meter {
    ConcurrentSandboxes,
    SandboxSeconds,
    BranchSecondsMax,
    SavesBytes,
    ModelUsdMicros,
    TrialUsdMicros,
    Runs,
    Seats,
    ModelTokens,
}

const METERS: [Meter; 9] = [
    Meter::ConcurrentSandboxes,
    Meter::SandboxSeconds,
    Meter::BranchSecondsMax,
    Meter::SavesBytes,
    Meter::ModelUsdMicros,
    Meter::TrialUsdMicros,
    Meter::Runs,
    Meter::Seats,
    Meter::ModelTokens,
];

impl Meter {
    pub fn as_str(self) -> &'static str {
        match self {
            Meter::ConcurrentSandboxes => "concurrent_sandboxes",
            Meter::SandboxSeconds => "sandbox_seconds",
            Meter::BranchSecondsMax => "branch_seconds_max",
            Meter::SavesBytes => "saves_bytes",
            Meter::ModelUsdMicros => "model_usd_micros",
            Meter::TrialUsdMicros => "trial_usd_micros",
            Meter::Runs => "runs",
            Meter::Seats => "seats",
            Meter::ModelTokens => "model_tokens",
        }
    }

    pub fn parse(s: &str) -> Option<Meter> {
        METERS.into_iter().find(|m| m.as_str() == s)
    }
}

/// A monthly total, or a gauge or per-item cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Month,
    None,
}

/// One limit: its value, whether going over refuses (hard) or warns, and its period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Limit {
    pub value: u64,
    #[serde(default = "hard")]
    pub hard: bool,
    pub period: Period,
}

fn hard() -> bool {
    true
}

/// plan -> meter -> limit.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Presets {
    plans: BTreeMap<String, BTreeMap<Meter, Limit>>,
}

const BUILT_IN: &str = include_str!("../../../../docs/quotas.toml");

impl Presets {
    /// `docs/quotas.toml` as shaked was built with it.
    pub fn built_in() -> &'static Presets {
        static P: OnceLock<Presets> = OnceLock::new();
        P.get_or_init(|| {
            toml::from_str(BUILT_IN).expect("docs/quotas.toml parses (tests check it)")
        })
    }

    /// The presets in `path`, else the built-in ones.
    pub fn load(path: Option<&Path>) -> Result<Presets> {
        match path {
            None => Ok(Presets::built_in().clone()),
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .with_context(|| format!("reading {}", p.display()))?;
                toml::from_str(&text).with_context(|| format!("parsing {}", p.display()))
            }
        }
    }

    /// The presets in force: `SHAKEDOWN_QUOTAS`'s file when set (read once), else the built-in.
    pub fn active() -> &'static Presets {
        static P: OnceLock<Presets> = OnceLock::new();
        P.get_or_init(|| {
            let set = parts::settings::var("SHAKEDOWN_QUOTAS")
                .ok()
                .filter(|v| !v.is_empty());
            Presets::load(set.as_deref().map(Path::new)).unwrap_or_else(|e| {
                eprintln!("shaked: SHAKEDOWN_QUOTAS: {e:#}; using the built-in quotas");
                Presets::built_in().clone()
            })
        })
    }

    /// `plan`'s limit on `m`; None: unlimited.
    pub fn limit(&self, plan: &str, m: Meter) -> Option<Limit> {
        self.plans.get(plan)?.get(&m).copied()
    }
}

/// What a check found.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok,
    /// Over a soft limit: go on, saying so.
    Warn(String),
    /// Over a hard limit: refuse, naming it.
    Refuse(String),
}

/// The verdict for `used + adding` against `limit`.
pub fn verdict(m: Meter, limit: Option<Limit>, used: u64, adding: u64) -> Verdict {
    let Some(l) = limit else {
        return Verdict::Ok;
    };
    if used + adding <= l.value {
        return Verdict::Ok;
    }
    let per = match l.period {
        Period::Month => " this month",
        Period::None => "",
    };
    let why = format!(
        "over the {} quota: {} of {}{per}",
        m.as_str(),
        used + adding,
        l.value
    );
    match l.hard {
        true => Verdict::Refuse(why),
        false => Verdict::Warn(why),
    }
}
