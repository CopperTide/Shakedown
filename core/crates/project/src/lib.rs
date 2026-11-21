//! Shakedown project folders: the file formats for worlds, scenarios, setups and experiments, and
//! safe reads and writes of the folder that holds them.

pub mod error;
pub mod format;
pub mod secrets;

pub use error::FormatError;
