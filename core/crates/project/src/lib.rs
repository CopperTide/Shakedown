//! Shakedown projects: the file formats for worlds, scenarios, setups and experiments, and safe
//! reads and writes of the files, in a folder or any other [`store::Store`].

mod dependents;
pub mod error;
pub mod format;
mod groups;
mod ids;
mod item;
mod load;
mod project;
pub mod secrets;
pub mod store;
mod trash;
mod validate;

pub use dependents::{Dependent, Expected, Fate, Removal};
pub use error::{FormatError, ProjectError};
pub use ids::Id;
pub use item::{kind_name, Item, ItemBody, KindFile, Problem};
pub use load::item_file;
pub use project::Project;
pub use trash::{Edited, Manifest};
