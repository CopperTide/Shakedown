//! The items a project holds, and the problems found reading or cross-checking them.

use crate::format::{parse, to_yaml, ExperimentFile, Kind, ScenarioFile, SetupFile, WorldFile};
use crate::{FormatError, Id};

/// The parsed contents of one item's file.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemBody {
    World(WorldFile),
    Scenario(ScenarioFile),
    Setup(SetupFile),
    Experiment(ExperimentFile),
}

impl ItemBody {
    /// Parses `text` from `file` as an item of `kind`.
    pub fn parse(kind: Kind, file: &str, text: &str) -> Result<ItemBody, FormatError> {
        Ok(match kind {
            Kind::World => ItemBody::World(parse(file, text)?),
            Kind::Scenario => ItemBody::Scenario(parse(file, text)?),
            Kind::Setup => ItemBody::Setup(parse(file, text)?),
            Kind::Experiment => ItemBody::Experiment(parse(file, text)?),
        })
    }

    /// The kind of item this body is.
    pub fn kind(&self) -> Kind {
        match self {
            ItemBody::World(_) => Kind::World,
            ItemBody::Scenario(_) => Kind::Scenario,
            ItemBody::Setup(_) => Kind::Setup,
            ItemBody::Experiment(_) => Kind::Experiment,
        }
    }

    /// The display name.
    pub fn label(&self) -> &str {
        match self {
            ItemBody::World(b) => &b.label,
            ItemBody::Scenario(b) => &b.label,
            ItemBody::Setup(b) => &b.label,
            ItemBody::Experiment(b) => &b.label,
        }
    }

    /// The body as YAML, as it would be written.
    pub fn to_yaml(&self) -> Result<String, FormatError> {
        match self {
            ItemBody::World(b) => to_yaml(b),
            ItemBody::Scenario(b) => to_yaml(b),
            ItemBody::Setup(b) => to_yaml(b),
            ItemBody::Experiment(b) => to_yaml(b),
        }
    }
}

/// One world, scenario, setup or experiment.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub id: Id,
    pub label: String,
    pub body: ItemBody,
    /// SHA-256 of the file's bytes, hex-encoded; changes whenever the file does.
    pub hash: String,
}

/// Something wrong with a file or between files. Problems never stop a project from opening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The kind of the item at fault, if it is an item.
    pub kind: Option<Kind>,
    /// The item's id, or the file name when the id itself is the problem.
    pub id: Option<String>,
    /// A JSON pointer into the item (`""` for the whole file).
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

/// The display name of a kind in messages.
pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::World => "world",
        Kind::Scenario => "scenario",
        Kind::Setup => "setup",
        Kind::Experiment => "experiment",
    }
}
