//! The items a project holds, and the problems found reading or cross-checking them.

use serde::Serialize;

use crate::format::{parse, to_yaml, ExperimentFile, Kind, ScenarioFile, SetupFile, WorldFile};
use crate::{FormatError, Id};

/// The parsed contents of one item's file. It serializes as the file itself, with no tag.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
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
        to_yaml(self)
    }
}

/// The file format of one kind of item, for reading an item's body as that format.
pub trait KindFile: Sized {
    /// The kind of item whose body this format is.
    const KIND: Kind;

    /// `body` as this format, if it is one.
    fn of(body: &ItemBody) -> Option<&Self>;
}

macro_rules! kind_files {
    ($($variant:ident($file:ty),)+) => {
        $(impl KindFile for $file {
            const KIND: Kind = Kind::$variant;

            fn of(body: &ItemBody) -> Option<&Self> {
                match body {
                    ItemBody::$variant(b) => Some(b),
                    _ => None,
                }
            }
        })+
    };
}

kind_files! {
    World(WorldFile),
    Scenario(ScenarioFile),
    Setup(SetupFile),
    Experiment(ExperimentFile),
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

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/larkspur/");

    fn body(kind: Kind, file: &str) -> ItemBody {
        let text = std::fs::read_to_string(format!("{FIXTURE}{file}")).unwrap();
        ItemBody::parse(kind, file, &text).unwrap()
    }

    /// A body serializes byte for byte as its file does, as JSON and as YAML.
    fn same_as_file<B: Serialize>(body: &ItemBody, file: &B) {
        let (json, yaml) = (serde_json::to_string(body), body.to_yaml());
        assert_eq!(json.unwrap(), serde_json::to_string(file).unwrap());
        assert_eq!(yaml.unwrap(), to_yaml(file).unwrap());
    }

    #[test]
    fn a_body_serializes_as_its_file_with_no_tag() {
        let world = body(Kind::World, "worlds/larkspur-plant/world.yaml");
        let scenario = body(Kind::Scenario, "scenarios/supplier-slip.yaml");
        let setup = body(Kind::Setup, "setups/solo-planner.yaml");
        let experiment = body(
            Kind::Experiment,
            "experiments/supplier-slip-solo-vs-team.yaml",
        );
        same_as_file(&world, WorldFile::of(&world).unwrap());
        same_as_file(&scenario, ScenarioFile::of(&scenario).unwrap());
        same_as_file(&setup, SetupFile::of(&setup).unwrap());
        same_as_file(&experiment, ExperimentFile::of(&experiment).unwrap());
        assert!(SetupFile::of(&world).is_none());
        let json = serde_json::to_value(&world).unwrap();
        assert_eq!(json["label"], world.label());
        assert!(json.get("World").is_none());
    }
}
