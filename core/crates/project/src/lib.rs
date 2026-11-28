//! Shakedown projects: the file formats for worlds, scenarios, setups and experiments, and safe
//! reads and writes in a folder or a [`Snapshot`] whose home chooses file and table kinds.

mod dependents;
pub mod error;
pub mod export;
pub mod format;
mod groups;
pub mod history;
mod ids;
mod item;
mod load;
mod project;
mod rows;
pub mod secrets;
mod setup_changes;
pub mod store;
mod tables;
mod trash;
mod validate;
mod views;

pub use dependents::{Dependent, Expected, Fate, Removal};
pub use error::{FormatError, ProjectError};
pub use ids::Id;
pub use item::{kind_name, Item, ItemBody, KindFile, Problem};
pub use load::item_file;
pub use project::Project;
pub use trash::{Edited, Manifest};
pub use views::{world_views, WorldView};

pub use tables::{
    Base, ChangeSet, FileChange, ItemChange, Row, SettingsChange, Snapshot, TableKinds,
};
